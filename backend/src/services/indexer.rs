//! `SQLite` FTS5 trigram 検索インデクサー
//!
//! ファイルシステムのエントリを `SQLite` に格納し、FTS5 trigram トークナイザで
//! ファイル名・相対パスの部分一致検索を提供する。
//!
//! - 3 文字以上のクエリ: FTS5 MATCH で高速検索
//! - 2 文字のクエリ: LIKE フォールバック
//! - 接続パターン: connection-per-call (WAL モード)
//! - 状態フラグ: `AtomicBool` でロックフリーの状態チェック

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{Connection, params};

use crate::services::extensions::classify_for_index;
use crate::services::parallel_walk::{self, WalkEntry};
use crate::services::path_security::PathSecurity;

/// FTS5 trigram トークナイザが要求する最小文字数
const TRIGRAM_MIN_CHARS: usize = 3;

/// バッチ INSERT のサイズ
const BATCH_SIZE: usize = 1000;

/// インデクサーエラー
#[derive(Debug, thiserror::Error)]
pub(crate) enum IndexerError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("{0}")]
    Other(String),
}

/// 検索インデックスに登録するエントリ
pub(crate) struct IndexEntry {
    pub relative_path: String,
    pub name: String,
    pub kind: String,
    pub size_bytes: Option<i64>,
    pub mtime_ns: i64,
}

/// 検索結果の 1 件
pub(crate) struct SearchHit {
    pub relative_path: String,
    pub name: String,
    pub kind: String,
    pub size_bytes: Option<i64>,
}

/// `parallel_walk` コールバック引数 (`DirIndex` 連携用)
pub(crate) struct WalkCallbackArgs {
    pub walk_entry_path: String,
    pub root_dir: String,
    pub mount_id: String,
    pub dir_mtime_ns: i64,
    pub subdirs: Vec<(String, i64)>,
    pub files: Vec<(String, i64, i64)>,
}

/// `SQLite` FTS5 trigram 検索インデクサー
///
/// - `init_db` でスキーマ作成 (冪等)
/// - `add_entry` / `remove_entry` でエントリ操作
/// - `search` で FTS5 or LIKE 検索
/// - `check_mount_fingerprint` / `save_mount_fingerprint` でマウント変更検出
pub(crate) struct Indexer {
    db_path: String,
    is_ready: AtomicBool,
    is_stale: AtomicBool,
    is_rebuilding: AtomicBool,
}

impl Indexer {
    /// 新しいインデクサーを生成する (DB 未初期化状態)
    pub(crate) fn new(db_path: &str) -> Self {
        Self {
            db_path: db_path.to_owned(),
            is_ready: AtomicBool::new(false),
            is_stale: AtomicBool::new(false),
            is_rebuilding: AtomicBool::new(false),
        }
    }

    /// WAL モード + パフォーマンス PRAGMA を設定した接続を開く
    fn connect(&self) -> Result<Connection, IndexerError> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;\
             PRAGMA busy_timeout=5000;\
             PRAGMA synchronous=NORMAL;\
             PRAGMA cache_size=-8192;\
             PRAGMA temp_store=MEMORY;",
        )?;
        Ok(conn)
    }

    /// スキーマを作成する (冪等)
    ///
    /// - `entries` テーブル + インデックス
    /// - `entries_fts` FTS5 仮想テーブル (trigram トークナイザ)
    /// - 自動同期トリガー (INSERT/UPDATE/DELETE)
    /// - `schema_meta` にバージョン "2" を記録
    pub(crate) fn init_db(&self) -> Result<(), IndexerError> {
        let conn = self.connect()?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_meta (\
                 key TEXT PRIMARY KEY, \
                 value TEXT NOT NULL\
             );\
             \
             CREATE TABLE IF NOT EXISTS entries (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT, \
                 relative_path TEXT NOT NULL UNIQUE, \
                 name TEXT NOT NULL, \
                 kind TEXT NOT NULL, \
                 size_bytes INTEGER, \
                 mtime_ns INTEGER NOT NULL\
             );\
             \
             CREATE INDEX IF NOT EXISTS idx_entries_kind \
                 ON entries(kind);\
             CREATE INDEX IF NOT EXISTS idx_entries_relative_path \
                 ON entries(relative_path);\
             \
             CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(\
                 name, relative_path, \
                 content=entries, content_rowid=id, \
                 tokenize='trigram'\
             );\
             \
             CREATE TRIGGER IF NOT EXISTS entries_ai AFTER INSERT ON entries BEGIN \
                 INSERT INTO entries_fts(rowid, name, relative_path) \
                     VALUES (new.id, new.name, new.relative_path); \
             END;\
             \
             CREATE TRIGGER IF NOT EXISTS entries_ad AFTER DELETE ON entries BEGIN \
                 INSERT INTO entries_fts(entries_fts, rowid, name, relative_path) \
                     VALUES('delete', old.id, old.name, old.relative_path); \
             END;\
             \
             CREATE TRIGGER IF NOT EXISTS entries_au AFTER UPDATE ON entries BEGIN \
                 INSERT INTO entries_fts(entries_fts, rowid, name, relative_path) \
                     VALUES('delete', old.id, old.name, old.relative_path); \
                 INSERT INTO entries_fts(rowid, name, relative_path) \
                     VALUES (new.id, new.name, new.relative_path); \
             END;",
        )?;

        // スキーマバージョンを記録
        conn.execute(
            "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('schema_version', '2')",
            [],
        )?;

        Ok(())
    }

    /// キーワード検索を実行する
    ///
    /// - 3 文字以上のトークンがあれば FTS5 MATCH で検索
    /// - なければ LIKE フォールバック (`%query%`)
    /// - `limit + 1` 件取得して `has_more` を判定
    #[allow(clippy::cast_possible_wrap, reason = "limit/offset は i64 範囲内")]
    pub(crate) fn search(
        &self,
        query: &str,
        kind: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<SearchHit>, bool), IndexerError> {
        let conn = self.connect()?;
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok((Vec::new(), false));
        }

        let fts_query = build_fts_query(trimmed);
        let fetch_limit = limit + 1;

        let mut hits = if fts_query.is_empty() {
            // LIKE フォールバック (全トークンが 3 文字未満)
            Self::search_like(&conn, trimmed, kind, fetch_limit, offset)?
        } else {
            Self::search_fts(&conn, &fts_query, kind, fetch_limit, offset)?
        };

        let has_more = hits.len() > limit;
        hits.truncate(limit);

        Ok((hits, has_more))
    }

    /// FTS5 MATCH による検索
    #[allow(clippy::cast_possible_wrap, reason = "limit/offset は i64 範囲内")]
    fn search_fts(
        conn: &Connection,
        fts_query: &str,
        kind: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<SearchHit>, IndexerError> {
        let (sql, has_kind) = if kind.is_some() {
            (
                "SELECT e.relative_path, e.name, e.kind, e.size_bytes \
                 FROM entries_fts f \
                 JOIN entries e ON e.id = f.rowid \
                 WHERE entries_fts MATCH ?1 AND e.kind = ?2 \
                 LIMIT ?3 OFFSET ?4",
                true,
            )
        } else {
            (
                "SELECT e.relative_path, e.name, e.kind, e.size_bytes \
                 FROM entries_fts f \
                 JOIN entries e ON e.id = f.rowid \
                 WHERE entries_fts MATCH ?1 \
                 LIMIT ?2 OFFSET ?3",
                false,
            )
        };

        let mut stmt = conn.prepare(sql)?;

        let rows = if has_kind {
            let kind_val = kind.unwrap_or_default();
            stmt.query_map(
                params![fts_query, kind_val, limit as i64, offset as i64],
                map_search_hit,
            )?
        } else {
            stmt.query_map(
                params![fts_query, limit as i64, offset as i64],
                map_search_hit,
            )?
        };

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(IndexerError::from)
    }

    /// LIKE フォールバック検索 (2 文字クエリ等)
    #[allow(clippy::cast_possible_wrap, reason = "limit/offset は i64 範囲内")]
    fn search_like(
        conn: &Connection,
        query: &str,
        kind: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<SearchHit>, IndexerError> {
        let pattern = format!("%{query}%");

        let (sql, has_kind) = if kind.is_some() {
            (
                "SELECT relative_path, name, kind, size_bytes \
                 FROM entries \
                 WHERE (name LIKE ?1 OR relative_path LIKE ?1) AND kind = ?2 \
                 LIMIT ?3 OFFSET ?4",
                true,
            )
        } else {
            (
                "SELECT relative_path, name, kind, size_bytes \
                 FROM entries \
                 WHERE name LIKE ?1 OR relative_path LIKE ?1 \
                 LIMIT ?2 OFFSET ?3",
                false,
            )
        };

        let mut stmt = conn.prepare(sql)?;

        let rows = if has_kind {
            let kind_val = kind.unwrap_or_default();
            stmt.query_map(
                params![pattern, kind_val, limit as i64, offset as i64],
                map_search_hit,
            )?
        } else {
            stmt.query_map(
                params![pattern, limit as i64, offset as i64],
                map_search_hit,
            )?
        };

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(IndexerError::from)
    }

    /// エントリを追加 (UPSERT: `relative_path` が重複する場合は上書き)
    pub(crate) fn add_entry(&self, entry: &IndexEntry) -> Result<(), IndexerError> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT OR REPLACE INTO entries (relative_path, name, kind, size_bytes, mtime_ns) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                entry.relative_path,
                entry.name,
                entry.kind,
                entry.size_bytes,
                entry.mtime_ns,
            ],
        )?;
        Ok(())
    }

    /// エントリを削除する
    pub(crate) fn remove_entry(&self, relative_path: &str) -> Result<(), IndexerError> {
        let conn = self.connect()?;
        conn.execute(
            "DELETE FROM entries WHERE relative_path = ?1",
            params![relative_path],
        )?;
        Ok(())
    }

    /// 登録済みエントリ数を返す
    #[allow(clippy::cast_possible_wrap, reason = "エントリ数は i64 範囲内")]
    pub(crate) fn entry_count(&self) -> Result<usize, IndexerError> {
        let conn = self.connect()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?;
        #[allow(clippy::cast_sign_loss, reason = "COUNT(*) は非負")]
        Ok(count as usize)
    }

    /// インデックスが使用可能かどうか
    pub(crate) fn is_ready(&self) -> bool {
        self.is_ready.load(Ordering::Relaxed)
    }

    /// インデックスが古い (再構築が必要) かどうか
    pub(crate) fn is_stale(&self) -> bool {
        self.is_stale.load(Ordering::Relaxed)
    }

    /// インデックス再構築中かどうか
    pub(crate) fn is_rebuilding(&self) -> bool {
        self.is_rebuilding.load(Ordering::Relaxed)
    }

    /// ウォームスタートを示す状態にする
    ///
    /// 既存インデックスを使いつつバックグラウンドで再構築する場合に呼ぶ。
    pub(crate) fn mark_warm_start(&self) {
        self.is_ready.store(true, Ordering::Relaxed);
        self.is_stale.store(true, Ordering::Relaxed);
    }

    /// 保存済みマウントフィンガープリントと現在のマウント ID リストを比較する
    ///
    /// 一致すれば `true` を返す。未保存の場合は `false`。
    pub(crate) fn check_mount_fingerprint(&self, mount_ids: &[&str]) -> Result<bool, IndexerError> {
        let conn = self.connect()?;
        let current = build_fingerprint(mount_ids);

        let stored: Option<String> = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'mount_fingerprint'",
                [],
                |row| row.get(0),
            )
            .ok();

        Ok(stored.as_deref() == Some(&current))
    }

    /// 現在のマウント ID リストをフィンガープリントとして保存する
    pub(crate) fn save_mount_fingerprint(&self, mount_ids: &[&str]) -> Result<(), IndexerError> {
        let conn = self.connect()?;
        let fingerprint = build_fingerprint(mount_ids);
        conn.execute(
            "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('mount_fingerprint', ?1)",
            params![fingerprint],
        )?;
        Ok(())
    }

    /// ディレクトリをフルスキャンしてインデックスを構築する
    ///
    /// - `parallel_walk` で並列走査し、インデクサブルなファイル/ディレクトリを登録
    /// - `on_walk_entry` コールバックで `DirIndex` 連携 (Phase 6b 用)
    /// - 完了後 `is_ready=true`, `is_stale=false` に設定
    pub(crate) fn scan_directory(
        &self,
        root_dir: &Path,
        path_security: &PathSecurity,
        mount_id: &str,
        workers: usize,
        on_walk_entry: Option<&mut dyn FnMut(WalkCallbackArgs)>,
    ) -> Result<usize, IndexerError> {
        let conn = self.connect()?;

        let mut batch: Vec<IndexEntry> = Vec::with_capacity(BATCH_SIZE);
        let mut total: usize = 0;

        // path_validator: PathSecurity で検証
        let validator = |path: &Path| -> bool { path_security.validate(path).is_ok() };

        // on_walk_entry を外部から受け取る
        let mut on_walk_entry = on_walk_entry;

        parallel_walk::parallel_walk(
            root_dir,
            workers,
            true, // skip_hidden
            Some(&validator),
            &mut |_path, _mtime_ns| true, // dir_filter: フルスキャンでは全ディレクトリを走査
            &mut |entry: WalkEntry| {
                let dir_relative = entry
                    .path
                    .strip_prefix(root_dir)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();

                let prefix = make_relative_prefix(mount_id, &dir_relative);

                // コールバック通知 (DirIndex 連携用)
                if let Some(ref mut cb) = on_walk_entry {
                    cb(WalkCallbackArgs {
                        walk_entry_path: entry.path.to_string_lossy().into_owned(),
                        root_dir: root_dir.to_string_lossy().into_owned(),
                        mount_id: mount_id.to_string(),
                        dir_mtime_ns: entry.mtime_ns,
                        subdirs: entry.subdirs.clone(),
                        files: entry.files.clone(),
                    });
                }

                // サブディレクトリを登録
                for (name, mtime_ns) in &entry.subdirs {
                    if let Some(kind) = classify_for_index(name, true) {
                        let relative_path = format!("{prefix}{name}");
                        batch.push(IndexEntry {
                            relative_path,
                            name: name.clone(),
                            kind: kind.to_string(),
                            size_bytes: None,
                            mtime_ns: *mtime_ns,
                        });
                    }
                }

                // ファイルを登録
                for (name, size_bytes, mtime_ns) in &entry.files {
                    if let Some(kind) = classify_for_index(name, false) {
                        let relative_path = format!("{prefix}{name}");
                        batch.push(IndexEntry {
                            relative_path,
                            name: name.clone(),
                            kind: kind.to_string(),
                            size_bytes: Some(*size_bytes),
                            mtime_ns: *mtime_ns,
                        });
                    }
                }

                // バッチサイズに達したらフラッシュ
                if batch.len() >= BATCH_SIZE {
                    total += batch.len();
                    // エラーはログ出力のみ (走査を止めない)
                    if let Err(e) = batch_insert(&conn, &batch) {
                        tracing::error!("バッチ INSERT 失敗: {e}");
                    }
                    batch.clear();
                }
            },
        );

        // 残りをフラッシュ
        if !batch.is_empty() {
            total += batch.len();
            batch_insert(&conn, &batch)?;
        }

        self.is_ready.store(true, Ordering::Relaxed);
        self.is_stale.store(false, Ordering::Relaxed);

        Ok(total)
    }

    /// 差分スキャンで変更のあったエントリのみ更新する
    ///
    /// - 既存エントリの `mtime_ns` を `HashMap` に読み込み
    /// - `parallel_walk` で `dir_filter` コールバックを使い mtime 変更のないディレクトリを枝刈り
    /// - 追加/更新/削除の件数を返す
    pub(crate) fn incremental_scan(
        &self,
        root_dir: &Path,
        path_security: &PathSecurity,
        mount_id: &str,
        workers: usize,
        on_walk_entry: Option<&mut dyn FnMut(WalkCallbackArgs)>,
    ) -> Result<(usize, usize, usize), IndexerError> {
        let conn = self.connect()?;
        let existing = load_existing_entries(&conn)?;
        let dir_mtimes = load_dir_mtimes(&conn)?;

        // dir_filter と entry_callback の両方から借用するため RefCell
        let seen = RefCell::new(HashSet::new());
        let mut added: usize = 0;
        let mut updated: usize = 0;

        let validator = |path: &Path| -> bool { path_security.validate(path).is_ok() };
        let mut on_walk_entry = on_walk_entry;

        // dir_filter: mtime 未変更のディレクトリを枝刈り
        let mut dir_filter = |path: &Path, mtime_ns: i64| -> bool {
            prune_unchanged_dir(
                path,
                mtime_ns,
                root_dir,
                mount_id,
                &dir_mtimes,
                &existing,
                &seen,
            )
        };

        parallel_walk::parallel_walk(
            root_dir,
            workers,
            true,
            Some(&validator),
            &mut dir_filter,
            &mut |entry: WalkEntry| {
                let (a, u) = process_walk_entry_incremental(
                    &entry,
                    root_dir,
                    mount_id,
                    &conn,
                    &existing,
                    &seen,
                    &mut on_walk_entry,
                );
                added += a;
                updated += u;
            },
        );

        let seen = seen.into_inner();
        let deleted = delete_unseen(&conn, &seen)?;

        self.is_ready.store(true, Ordering::Relaxed);
        self.is_stale.store(false, Ordering::Relaxed);

        Ok((added, updated, deleted))
    }

    /// インデックスを全削除して再構築する
    pub(crate) fn rebuild(
        &self,
        root_dir: &Path,
        path_security: &PathSecurity,
        mount_id: &str,
    ) -> Result<usize, IndexerError> {
        self.is_rebuilding.store(true, Ordering::Relaxed);

        // 全エントリを削除
        let conn = self.connect()?;
        conn.execute("DELETE FROM entries", [])?;
        drop(conn);

        let result = self.scan_directory(root_dir, path_security, mount_id, 8, None);

        self.is_rebuilding.store(false, Ordering::Relaxed);

        result
    }
}

/// mtime 未変更のディレクトリを枝刈りし、配下エントリを seen に追加する
///
/// `true` を返すと走査続行、`false` を返すと枝刈り。
#[allow(
    clippy::too_many_arguments,
    reason = "incremental_scan のコンテキストを受け取る内部ヘルパー"
)]
fn prune_unchanged_dir(
    path: &Path,
    mtime_ns: i64,
    root_dir: &Path,
    mount_id: &str,
    dir_mtimes: &HashMap<String, i64>,
    existing: &HashMap<String, i64>,
    seen: &RefCell<HashSet<String>>,
) -> bool {
    let dir_relative = path
        .strip_prefix(root_dir)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let relative_path = make_relative_prefix(mount_id, &dir_relative);
    let dir_key = relative_path.strip_suffix('/').unwrap_or(&relative_path);

    if let Some(&stored_mtime) = dir_mtimes.get(dir_key) {
        if stored_mtime == mtime_ns {
            // mtime 未変更 → 配下の既存エントリを全て seen に追加して枝刈り
            let prefix_with_slash = if dir_key.is_empty() {
                String::new()
            } else {
                format!("{dir_key}/")
            };
            let mut seen_mut = seen.borrow_mut();
            for key in existing.keys() {
                if key.starts_with(&prefix_with_slash) || key == dir_key {
                    seen_mut.insert(key.clone());
                }
            }
            seen_mut.insert(dir_key.to_string());
            return false;
        }
    }
    true
}

/// `incremental_scan` 内の `WalkEntry` を処理し、(added, updated) を返す
#[allow(
    clippy::too_many_arguments,
    reason = "incremental_scan のコンテキストを受け取る内部ヘルパー"
)]
fn process_walk_entry_incremental(
    entry: &WalkEntry,
    root_dir: &Path,
    mount_id: &str,
    conn: &Connection,
    existing: &HashMap<String, i64>,
    seen: &RefCell<HashSet<String>>,
    on_walk_entry: &mut Option<&mut dyn FnMut(WalkCallbackArgs)>,
) -> (usize, usize) {
    let dir_relative = entry
        .path
        .strip_prefix(root_dir)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let prefix = make_relative_prefix(mount_id, &dir_relative);

    // コールバック通知
    if let Some(cb) = on_walk_entry {
        cb(WalkCallbackArgs {
            walk_entry_path: entry.path.to_string_lossy().into_owned(),
            root_dir: root_dir.to_string_lossy().into_owned(),
            mount_id: mount_id.to_string(),
            dir_mtime_ns: entry.mtime_ns,
            subdirs: entry.subdirs.clone(),
            files: entry.files.clone(),
        });
    }

    let mut added: usize = 0;
    let mut updated: usize = 0;

    // サブディレクトリを処理
    for (name, mtime_ns) in &entry.subdirs {
        if let Some(kind) = classify_for_index(name, true) {
            let relative_path = format!("{prefix}{name}");
            let ie = IndexEntry {
                relative_path: relative_path.clone(),
                name: name.clone(),
                kind: kind.to_string(),
                size_bytes: None,
                mtime_ns: *mtime_ns,
            };
            match upsert_entry(conn, &ie, existing) {
                Ok(UpsertResult::Added) => added += 1,
                Ok(UpsertResult::Updated) => updated += 1,
                Ok(UpsertResult::Unchanged) => {}
                Err(e) => tracing::error!("UPSERT 失敗: {e}"),
            }
            seen.borrow_mut().insert(relative_path);
        }
    }

    // ファイルを処理
    for (name, size_bytes, mtime_ns) in &entry.files {
        if let Some(kind) = classify_for_index(name, false) {
            let relative_path = format!("{prefix}{name}");
            let ie = IndexEntry {
                relative_path: relative_path.clone(),
                name: name.clone(),
                kind: kind.to_string(),
                size_bytes: Some(*size_bytes),
                mtime_ns: *mtime_ns,
            };
            match upsert_entry(conn, &ie, existing) {
                Ok(UpsertResult::Added) => added += 1,
                Ok(UpsertResult::Updated) => updated += 1,
                Ok(UpsertResult::Unchanged) => {}
                Err(e) => tracing::error!("UPSERT 失敗: {e}"),
            }
            seen.borrow_mut().insert(relative_path);
        }
    }

    (added, updated)
}

/// entries テーブルにバッチ INSERT する
fn batch_insert(conn: &Connection, entries: &[IndexEntry]) -> Result<(), IndexerError> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT OR REPLACE INTO entries (relative_path, name, kind, size_bytes, mtime_ns) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for entry in entries {
            stmt.execute(params![
                entry.relative_path,
                entry.name,
                entry.kind,
                entry.size_bytes,
                entry.mtime_ns,
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// `relative_path` プレフィックスを構築する
///
/// `mount_id/dir_relative/` の形式。ルート直下の場合は `mount_id/`。
fn make_relative_prefix(mount_id: &str, dir_relative: &str) -> String {
    if dir_relative.is_empty() {
        format!("{mount_id}/")
    } else {
        format!("{mount_id}/{dir_relative}/")
    }
}

/// UPSERT の結果
enum UpsertResult {
    Added,
    Updated,
    Unchanged,
}

/// 既存エントリとの差分を判定して UPSERT する
fn upsert_entry(
    conn: &Connection,
    entry: &IndexEntry,
    existing: &HashMap<String, i64>,
) -> Result<UpsertResult, IndexerError> {
    if let Some(&stored_mtime) = existing.get(&entry.relative_path) {
        if stored_mtime == entry.mtime_ns {
            return Ok(UpsertResult::Unchanged);
        }
        // mtime が変わった → UPDATE
        conn.execute(
            "UPDATE entries SET name=?1, kind=?2, size_bytes=?3, mtime_ns=?4 \
             WHERE relative_path=?5",
            params![
                entry.name,
                entry.kind,
                entry.size_bytes,
                entry.mtime_ns,
                entry.relative_path,
            ],
        )?;
        Ok(UpsertResult::Updated)
    } else {
        // 新規 → INSERT
        conn.execute(
            "INSERT OR REPLACE INTO entries (relative_path, name, kind, size_bytes, mtime_ns) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                entry.relative_path,
                entry.name,
                entry.kind,
                entry.size_bytes,
                entry.mtime_ns,
            ],
        )?;
        Ok(UpsertResult::Added)
    }
}

/// 既存エントリの (`relative_path`, `mtime_ns`) を `HashMap` に読み込む
fn load_existing_entries(conn: &Connection) -> Result<HashMap<String, i64>, IndexerError> {
    let mut stmt = conn.prepare("SELECT relative_path, mtime_ns FROM entries")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (path, mtime) = row?;
        map.insert(path, mtime);
    }
    Ok(map)
}

/// ディレクトリエントリの (`relative_path`, `mtime_ns`) を `HashMap` に読み込む
fn load_dir_mtimes(conn: &Connection) -> Result<HashMap<String, i64>, IndexerError> {
    let mut stmt =
        conn.prepare("SELECT relative_path, mtime_ns FROM entries WHERE kind = 'directory'")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (path, mtime) = row?;
        map.insert(path, mtime);
    }
    Ok(map)
}

/// `seen` に含まれないエントリを削除し、削除件数を返す
#[allow(clippy::cast_sign_loss, reason = "削除件数は非負")]
fn delete_unseen(conn: &Connection, seen: &HashSet<String>) -> Result<usize, IndexerError> {
    // 全エントリの relative_path を取得して seen にないものを削除
    let mut stmt = conn.prepare("SELECT relative_path FROM entries")?;
    let all_paths: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut deleted: usize = 0;
    for path in &all_paths {
        if !seen.contains(path) {
            conn.execute(
                "DELETE FROM entries WHERE relative_path = ?1",
                params![path],
            )?;
            deleted += 1;
        }
    }
    Ok(deleted)
}

/// マウント ID をソートしてカンマ結合したフィンガープリントを生成する
fn build_fingerprint(mount_ids: &[&str]) -> String {
    let mut sorted: Vec<&str> = mount_ids.to_vec();
    sorted.sort_unstable();
    sorted.join(",")
}

/// FTS5 クエリ文字列を組み立てる
///
/// スペース区切りで分割し、3 文字以上のトークンをダブルクォートで囲む。
/// 内部のダブルクォートは `""` にエスケープする。
/// トークン間はスペース (暗黙 AND) で結合する。
fn build_fts_query(query: &str) -> String {
    let tokens: Vec<String> = query
        .split_whitespace()
        .filter(|w| w.chars().count() >= TRIGRAM_MIN_CHARS)
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect();
    tokens.join(" ")
}

/// `rusqlite::Row` から `SearchHit` にマッピングする
fn map_search_hit(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchHit> {
    Ok(SearchHit {
        relative_path: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        size_bytes: row.get(3)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用の一時 DB パスでインデクサーを生成する
    fn setup_indexer() -> (Indexer, tempfile::NamedTempFile) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let indexer = Indexer::new(tmp.path().to_str().unwrap());
        indexer.init_db().unwrap();
        (indexer, tmp)
    }

    /// テスト用エントリを生成する
    fn make_entry(relative_path: &str, name: &str, kind: &str) -> IndexEntry {
        IndexEntry {
            relative_path: relative_path.to_owned(),
            name: name.to_owned(),
            kind: kind.to_owned(),
            size_bytes: Some(1024),
            mtime_ns: 1_000_000_000,
        }
    }

    #[test]
    fn init_dbでスキーマが作成される() {
        let (indexer, _tmp) = setup_indexer();
        let count = indexer.entry_count().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn エントリの追加と検索ができる() {
        let (indexer, _tmp) = setup_indexer();

        let entry = make_entry("photos/sunset.jpg", "sunset.jpg", "image");
        indexer.add_entry(&entry).unwrap();

        let (hits, has_more) = indexer.search("sunset", None, 10, 0).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(!has_more);
        assert_eq!(hits[0].name, "sunset.jpg");
        assert_eq!(hits[0].relative_path, "photos/sunset.jpg");
        assert_eq!(hits[0].kind, "image");
    }

    #[test]
    fn kind指定で検索をフィルタできる() {
        let (indexer, _tmp) = setup_indexer();

        indexer
            .add_entry(&make_entry("videos/clip.mp4", "clip.mp4", "video"))
            .unwrap();
        indexer
            .add_entry(&make_entry("docs/manual.pdf", "manual.pdf", "pdf"))
            .unwrap();

        // kind="video" で検索 — "clip" は 4 文字なので FTS5 パス
        let (hits, _) = indexer.search("clip", Some("video"), 10, 0).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "video");

        // kind="pdf" で同じクエリ — ヒットしない
        let (hits, _) = indexer.search("clip", Some("pdf"), 10, 0).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn エントリの削除で検索から消える() {
        let (indexer, _tmp) = setup_indexer();

        let entry = make_entry("photos/beach.jpg", "beach.jpg", "image");
        indexer.add_entry(&entry).unwrap();

        // 削除前: 検索にヒットする
        let (hits, _) = indexer.search("beach", None, 10, 0).unwrap();
        assert_eq!(hits.len(), 1);

        // 削除
        indexer.remove_entry("photos/beach.jpg").unwrap();

        // 削除後: 検索にヒットしない
        let (hits, _) = indexer.search("beach", None, 10, 0).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn 二文字クエリでlikeフォールバック() {
        let (indexer, _tmp) = setup_indexer();

        let entry = make_entry("tests/ab_test.mp4", "ab_test.mp4", "video");
        indexer.add_entry(&entry).unwrap();

        // "ab" は 2 文字 → LIKE フォールバック
        let (hits, _) = indexer.search("ab", None, 10, 0).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "ab_test.mp4");
    }

    #[test]
    fn 日本語ファイル名の部分一致検索() {
        let (indexer, _tmp) = setup_indexer();

        let entry = make_entry("動画/テスト動画.mp4", "テスト動画.mp4", "video");
        indexer.add_entry(&entry).unwrap();

        // "テスト" は 3 文字 → FTS5 パス
        let (hits, _) = indexer.search("テスト", None, 10, 0).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "テスト動画.mp4");
    }

    #[test]
    fn 特殊文字入力でエラーにならない() {
        let (indexer, _tmp) = setup_indexer();

        // ダブルクォートやアスタリスクを含むクエリでエラーにならない
        let result = indexer.search("\"test*", None, 10, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn mount_fingerprintの保存と検証() {
        let (indexer, _tmp) = setup_indexer();

        let ids = vec!["mount_a", "mount_b"];
        indexer.save_mount_fingerprint(&ids).unwrap();

        // 同じ ID リストで検証 → true
        assert!(indexer.check_mount_fingerprint(&ids).unwrap());

        // 異なる ID リストで検証 → false
        let different = vec!["mount_c"];
        assert!(!indexer.check_mount_fingerprint(&different).unwrap());

        // 順序を入れ替えても一致する (ソート済みフィンガープリント)
        let reversed = vec!["mount_b", "mount_a"];
        assert!(indexer.check_mount_fingerprint(&reversed).unwrap());
    }

    #[test]
    fn mark_warm_startでis_readyとis_staleが設定される() {
        let (indexer, _tmp) = setup_indexer();

        // 初期状態: 両方 false
        assert!(!indexer.is_ready());
        assert!(!indexer.is_stale());

        indexer.mark_warm_start();

        assert!(indexer.is_ready());
        assert!(indexer.is_stale());
    }

    #[test]
    fn has_moreがlimit超過時にtrueになる() {
        let (indexer, _tmp) = setup_indexer();

        // 3 件のエントリを追加
        for i in 0..3 {
            indexer
                .add_entry(&make_entry(
                    &format!("photos/image_{i}.jpg"),
                    &format!("image_{i}.jpg"),
                    "image",
                ))
                .unwrap();
        }

        // limit=2 で検索 → has_more=true
        let (hits, has_more) = indexer.search("image", None, 2, 0).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(has_more);

        // limit=10 で検索 → has_more=false
        let (hits, has_more) = indexer.search("image", None, 10, 0).unwrap();
        assert_eq!(hits.len(), 3);
        assert!(!has_more);
    }

    // --- scan_directory / incremental_scan / rebuild テスト ---

    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// テスト用ディレクトリ構造とインデクサーを生成する
    ///
    /// root/
    ///   sub1/
    ///     movie.mp4  (5 bytes)
    ///   doc.pdf      (3 bytes)
    ///   image.jpg    (3 bytes)  -- インデックス対象外 (画像)
    struct ScanTestEnv {
        #[allow(dead_code, reason = "TempDir のドロップでディレクトリを保持")]
        _dir: TempDir,
        root: PathBuf,
        indexer: Indexer,
        #[allow(dead_code, reason = "NamedTempFile のドロップで DB を保持")]
        _db_file: tempfile::NamedTempFile,
    }

    impl ScanTestEnv {
        fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let root = fs::canonicalize(dir.path()).unwrap();
            fs::create_dir_all(root.join("sub1")).unwrap();
            fs::write(root.join("sub1/movie.mp4"), b"video").unwrap();
            fs::write(root.join("doc.pdf"), b"pdf").unwrap();
            fs::write(root.join("image.jpg"), b"img").unwrap();

            let db_file = tempfile::NamedTempFile::new().unwrap();
            let indexer = Indexer::new(db_file.path().to_str().unwrap());
            indexer.init_db().unwrap();

            Self {
                _dir: dir,
                root,
                indexer,
                _db_file: db_file,
            }
        }

        fn path_security(&self) -> PathSecurity {
            PathSecurity::new(vec![self.root.clone()], false).unwrap()
        }
    }

    #[test]
    fn scan_directoryでディレクトリを走査してインデックスに登録する() {
        let env = ScanTestEnv::new();
        let ps = env.path_security();

        let count = env
            .indexer
            .scan_directory(&env.root, &ps, "test_mount", 2, None)
            .unwrap();

        // sub1 (directory) + movie.mp4 (video) + doc.pdf (pdf) = 3
        // image.jpg は画像なのでインデックス対象外
        assert_eq!(count, 3);
        assert_eq!(env.indexer.entry_count().unwrap(), 3);

        // is_ready が true に設定される
        assert!(env.indexer.is_ready());
        assert!(!env.indexer.is_stale());
    }

    #[test]
    fn incremental_scanで変更ファイルのみ更新する() {
        let env = ScanTestEnv::new();
        let ps = env.path_security();

        // 初回フルスキャン
        env.indexer
            .scan_directory(&env.root, &ps, "test_mount", 2, None)
            .unwrap();
        assert_eq!(env.indexer.entry_count().unwrap(), 3);

        // ファイルを追加して sub1 の mtime を変える
        fs::write(env.root.join("sub1/extra.mp4"), b"extra").unwrap();

        let (added, updated, deleted) = env
            .indexer
            .incremental_scan(&env.root, &ps, "test_mount", 2, None)
            .unwrap();

        // extra.mp4 が追加される
        assert!(added >= 1, "added={added}, 少なくとも1件追加されるべき");
        // 削除はない
        assert_eq!(deleted, 0);
        // 合計 4 件 (sub1 + movie.mp4 + doc.pdf + extra.mp4)
        assert_eq!(env.indexer.entry_count().unwrap(), 4);

        // is_ready が true に設定される
        assert!(env.indexer.is_ready());
        assert!(!env.indexer.is_stale());

        // updated は sub1 の mtime 変更で 0 以上 (ディレクトリ mtime が更新されていれば updated)
        let _ = updated; // コンパイラ警告抑制
    }

    #[test]
    fn rebuildでインデックスを再構築する() {
        let env = ScanTestEnv::new();
        let ps = env.path_security();

        // 初回スキャン
        env.indexer
            .scan_directory(&env.root, &ps, "test_mount", 2, None)
            .unwrap();
        assert_eq!(env.indexer.entry_count().unwrap(), 3);

        // rebuild
        let count = env.indexer.rebuild(&env.root, &ps, "test_mount").unwrap();

        // 同じ件数で再構築される
        assert_eq!(count, 3);
        assert_eq!(env.indexer.entry_count().unwrap(), 3);

        // is_rebuilding は完了後 false
        assert!(!env.indexer.is_rebuilding());
        assert!(env.indexer.is_ready());
    }
}
