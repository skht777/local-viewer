//! ディレクトリリスティングインデックス
//!
//! browse API の高速化のため、ディレクトリの子エントリを `SQLite` に事前保存する。
//! `sort_key` による自然順ソートと、カーソルベースのシーク型ページネーションを提供。

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{Connection, params};

use crate::services::extensions::{EntryKind, extract_extension};
use crate::services::indexer::WalkCallbackArgs;
use crate::services::natural_sort::encode_sort_key;

/// スキーマバージョン
const SCHEMA_VERSION: &str = "1";

/// `BulkInserter` のバッチサイズ
const BATCH_SIZE: usize = 1000;

/// `DirIndex` のエラー型
#[derive(Debug, thiserror::Error)]
pub(crate) enum DirIndexError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("{0}")]
    Other(String),
}

/// ディレクトリエントリ
#[derive(Debug)]
pub(crate) struct DirEntry {
    pub parent_path: String,
    pub name: String,
    pub kind: String,
    pub sort_key: String,
    pub size_bytes: Option<i64>,
    pub mtime_ns: i64,
}

/// ディレクトリリスティング専用 `SQLite` インデックス
///
/// - `parent_path` ベースで全エントリ (画像含む) を格納
/// - 自然順ソート (`sort_key`) + カーソルベースページネーション
/// - Warm Start パターン (`is_ready` / `is_stale`)
pub(crate) struct DirIndex {
    db_path: String,
    is_ready: AtomicBool,
    is_stale: AtomicBool,
}

impl DirIndex {
    /// 新しい `DirIndex` を生成する (DB 未初期化状態)
    pub(crate) fn new(db_path: &str) -> Self {
        Self {
            db_path: db_path.to_owned(),
            is_ready: AtomicBool::new(false),
            is_stale: AtomicBool::new(false),
        }
    }

    /// WAL モード + パフォーマンス PRAGMA を設定した接続を開く
    fn connect(&self) -> Result<Connection, DirIndexError> {
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

    /// テーブルとインデックスを作成する (冪等)
    pub(crate) fn init_db(&self) -> Result<(), DirIndexError> {
        let conn = self.connect()?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_meta (\
                 key TEXT PRIMARY KEY, \
                 value TEXT NOT NULL\
             );\
             \
             CREATE TABLE IF NOT EXISTS dir_entries (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT, \
                 parent_path TEXT NOT NULL, \
                 name TEXT NOT NULL, \
                 kind TEXT NOT NULL, \
                 sort_key TEXT NOT NULL, \
                 size_bytes INTEGER, \
                 mtime_ns INTEGER NOT NULL, \
                 UNIQUE(parent_path, name)\
             );\
             \
             CREATE INDEX IF NOT EXISTS idx_dir_parent \
                 ON dir_entries(parent_path);\
             CREATE INDEX IF NOT EXISTS idx_dir_parent_sort \
                 ON dir_entries(parent_path, sort_key);\
             CREATE INDEX IF NOT EXISTS idx_dir_parent_mtime \
                 ON dir_entries(parent_path, mtime_ns);\
             \
             CREATE TABLE IF NOT EXISTS dir_meta (\
                 path TEXT PRIMARY KEY, \
                 mtime_ns INTEGER NOT NULL\
             );",
        )?;

        // スキーマバージョンを記録
        conn.execute(
            "INSERT OR REPLACE INTO schema_meta (key, value) VALUES (?1, ?2)",
            params![&"schema_version", SCHEMA_VERSION],
        )?;

        Ok(())
    }

    /// インデックスが使用可能かどうか
    pub(crate) fn is_ready(&self) -> bool {
        self.is_ready.load(Ordering::Relaxed)
    }

    /// インデックスが古い (再構築が必要) かどうか
    pub(crate) fn is_stale(&self) -> bool {
        self.is_stale.load(Ordering::Relaxed)
    }

    /// インデックスを使用可能にする (`is_ready=true`, `is_stale=false`)
    pub(crate) fn mark_ready(&self) {
        self.is_ready.store(true, Ordering::Relaxed);
        self.is_stale.store(false, Ordering::Relaxed);
    }

    /// ウォームスタートを示す状態にする (`is_ready=true`, `is_stale=true`)
    ///
    /// 既存データで即座にクエリを提供しつつ、バックグラウンドで再構築する場合に使用。
    pub(crate) fn mark_warm_start(&self) {
        self.is_ready.store(true, Ordering::Relaxed);
        self.is_stale.store(true, Ordering::Relaxed);
    }

    // ---------------------------------------------------------------
    // クエリ
    // ---------------------------------------------------------------

    /// ソート + カーソルベースページネーション付きでエントリを返す
    ///
    /// - `sort`: `"name-asc"`, `"name-desc"`, `"date-asc"`, `"date-desc"`
    /// - `cursor_sort_key`: 前ページ末尾のソートキー (name 系) または `mtime_ns` 文字列 (date 系)
    ///   name 系カーソルは `"{kind_flag}\x00{sort_key}"` 形式 (`kind_flag`: "0"=directory, "1"=other)
    pub(crate) fn query_page(
        &self,
        parent_path: &str,
        sort: &str,
        limit: usize,
        cursor_sort_key: Option<&str>,
    ) -> Result<Vec<DirEntry>, DirIndexError> {
        let conn = self.connect()?;
        match sort {
            "name-desc" => query_name_desc(&conn, parent_path, limit, cursor_sort_key),
            "date-desc" => query_date_desc(&conn, parent_path, limit, cursor_sort_key),
            "date-asc" => query_date_asc(&conn, parent_path, limit, cursor_sort_key),
            // "name-asc" およびその他のソート指定
            _ => query_name_asc(&conn, parent_path, limit, cursor_sort_key),
        }
    }

    /// 指定ディレクトリの子エントリ数を返す
    #[allow(clippy::cast_sign_loss, reason = "COUNT(*) は非負")]
    pub(crate) fn child_count(&self, parent_path: &str) -> Result<usize, DirIndexError> {
        let conn = self.connect()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM dir_entries WHERE parent_path = ?1",
            params![parent_path],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// サムネイル対象エントリを返す (画像/動画/PDF/アーカイブ)
    #[allow(clippy::cast_possible_wrap, reason = "limit は i64 範囲内")]
    pub(crate) fn preview_entries(
        &self,
        parent_path: &str,
        limit: usize,
    ) -> Result<Vec<DirEntry>, DirIndexError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 AND kind IN ('image', 'archive', 'pdf', 'video') \
             ORDER BY sort_key ASC \
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![parent_path, limit as i64], map_dir_entry)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(DirIndexError::from)
    }

    /// 指定 kind の最初のエントリを返す (`first-viewable` 高速パス用)
    pub(crate) fn first_entry_by_kind(
        &self,
        parent_path: &str,
        kind: &str,
    ) -> Result<Option<DirEntry>, DirIndexError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 AND kind = ?2 \
             ORDER BY sort_key ASC \
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![parent_path, kind], map_dir_entry)?;
        match rows.next() {
            Some(Ok(entry)) => Ok(Some(entry)),
            Some(Err(e)) => Err(DirIndexError::from(e)),
            None => Ok(None),
        }
    }

    /// 次/前の兄弟エントリを返す (`sibling` 高速パス用)
    ///
    /// `direction` は `"next"` or `"prev"`。
    /// `kinds` で対象 kind をフィルタ。
    pub(crate) fn find_sibling_entry(
        &self,
        parent_path: &str,
        current_sort_key: &str,
        direction: &str,
        kinds: &[&str],
    ) -> Result<Option<DirEntry>, DirIndexError> {
        let conn = self.connect()?;
        // kind の IN 句を動的に構築
        let placeholders: Vec<String> = (0..kinds.len()).map(|i| format!("?{}", i + 3)).collect();
        let in_clause = placeholders.join(", ");

        let (op, order) = if direction == "next" {
            (">", "ASC")
        } else {
            ("<", "DESC")
        };

        let sql = format!(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 AND kind IN ({in_clause}) AND sort_key {op} ?2 \
             ORDER BY sort_key {order} \
             LIMIT 1"
        );

        let mut stmt = conn.prepare(&sql)?;
        // パラメータ: ?1=parent_path, ?2=current_sort_key, ?3..=kinds
        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        params_vec.push(Box::new(parent_path.to_string()));
        params_vec.push(Box::new(current_sort_key.to_string()));
        for kind in kinds {
            params_vec.push(Box::new(kind.to_string()));
        }
        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(std::convert::AsRef::as_ref).collect();

        let mut rows = stmt.query_map(params_ref.as_slice(), map_dir_entry)?;
        match rows.next() {
            Some(Ok(entry)) => Ok(Some(entry)),
            Some(Err(e)) => Err(DirIndexError::from(e)),
            None => Ok(None),
        }
    }

    /// DB 内の全エントリ数を返す
    #[allow(clippy::cast_sign_loss, reason = "COUNT(*) は非負")]
    pub(crate) fn entry_count(&self) -> Result<usize, DirIndexError> {
        let conn = self.connect()?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM dir_entries", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    // ---------------------------------------------------------------
    // ディレクトリメタデータ
    // ---------------------------------------------------------------

    /// ディレクトリの mtime を記録する
    pub(crate) fn set_dir_mtime(&self, path: &str, mtime_ns: i64) -> Result<(), DirIndexError> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT OR REPLACE INTO dir_meta (path, mtime_ns) VALUES (?1, ?2)",
            params![path, mtime_ns],
        )?;
        Ok(())
    }

    /// ディレクトリの記録済み mtime を返す
    pub(crate) fn get_dir_mtime(&self, path: &str) -> Result<Option<i64>, DirIndexError> {
        let conn = self.connect()?;
        let result = conn
            .query_row(
                "SELECT mtime_ns FROM dir_meta WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .ok();
        Ok(result)
    }

    // ---------------------------------------------------------------
    // スキャン連携
    // ---------------------------------------------------------------

    /// `parallel_walk` の `WalkCallbackArgs` を受け取り `DirIndex` に格納する
    ///
    /// - `walk_entry_path` を `root_dir` からの相対パスに変換し `mount_id` をプレフィックス
    /// - サブディレクトリ: `kind="directory"`, `size_bytes=None`
    /// - ファイル: `kind` を拡張子から判定 (全種別を格納)
    pub(crate) fn ingest_walk_entry(&self, args: &WalkCallbackArgs) -> Result<(), DirIndexError> {
        let conn = self.connect()?;
        let parent_path = build_parent_path(args);

        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO dir_entries \
                     (parent_path, name, kind, sort_key, size_bytes, mtime_ns) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;

            // サブディレクトリを登録
            for (name, mtime_ns) in &args.subdirs {
                let sort_key = encode_sort_key(name);
                stmt.execute(params![
                    parent_path,
                    name,
                    "directory",
                    sort_key,
                    Option::<i64>::None,
                    mtime_ns,
                ])?;
            }

            // ファイルを登録 (全種別)
            for (name, size_bytes, mtime_ns) in &args.files {
                let kind = classify_kind(name);
                let sort_key = encode_sort_key(name);
                stmt.execute(params![
                    parent_path,
                    name,
                    kind,
                    sort_key,
                    size_bytes,
                    mtime_ns
                ])?;
            }
        }
        tx.commit()?;

        // ディレクトリ mtime を記録
        conn.execute(
            "INSERT OR REPLACE INTO dir_meta (path, mtime_ns) VALUES (?1, ?2)",
            params![parent_path, args.dir_mtime_ns],
        )?;

        Ok(())
    }

    /// フルスキャンが完了しているかを返す
    pub(crate) fn is_full_scan_done(&self) -> Result<bool, DirIndexError> {
        let conn = self.connect()?;
        let result: Option<String> = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'full_scan_done'",
                [],
                |row| row.get(0),
            )
            .ok();
        Ok(result.as_deref() == Some("1"))
    }

    /// フルスキャン完了フラグを設定する
    pub(crate) fn mark_full_scan_done(&self) -> Result<(), DirIndexError> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('full_scan_done', '1')",
            [],
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------
    // バルクモード
    // ---------------------------------------------------------------

    /// バルク挿入用の `BulkInserter` を生成する
    ///
    /// 単一接続 + `synchronous=OFF` + バッチ INSERT で高速に格納する。
    /// `DirIndex` はキャッシュ DB のため、中断時のデータ損失は許容。
    pub(crate) fn begin_bulk(&self) -> Result<BulkInserter, DirIndexError> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;\
             PRAGMA busy_timeout=5000;\
             PRAGMA synchronous=OFF;\
             PRAGMA cache_size=-16384;\
             PRAGMA temp_store=MEMORY;",
        )?;
        Ok(BulkInserter {
            conn,
            pending_entries: Vec::with_capacity(BATCH_SIZE),
            pending_meta: Vec::with_capacity(64),
        })
    }
}

// ===================================================================
// BulkInserter
// ===================================================================

/// バッチ挿入用のエントリ行
/// `(parent_path, name, kind, sort_key, size_bytes, mtime_ns)`
type PendingEntry = (String, String, String, String, Option<i64>, i64);

/// バッチ挿入用のハンドル
///
/// エントリを蓄積し、`BATCH_SIZE` 到達時またはドロップ時にフラッシュする。
pub(crate) struct BulkInserter {
    conn: Connection,
    pending_entries: Vec<PendingEntry>,
    pending_meta: Vec<(String, i64)>,
}

impl BulkInserter {
    /// `WalkCallbackArgs` を受け取りバッチに蓄積する
    ///
    /// `BATCH_SIZE` に達したら自動フラッシュする。
    pub(crate) fn ingest_walk_entry(
        &mut self,
        args: &WalkCallbackArgs,
    ) -> Result<(), DirIndexError> {
        let parent_path = build_parent_path(args);

        // サブディレクトリ
        for (name, mtime_ns) in &args.subdirs {
            let sort_key = encode_sort_key(name);
            self.pending_entries.push((
                parent_path.clone(),
                name.clone(),
                "directory".to_owned(),
                sort_key,
                None,
                *mtime_ns,
            ));
        }

        // ファイル (全種別)
        for (name, size_bytes, mtime_ns) in &args.files {
            let kind = classify_kind(name).to_owned();
            let sort_key = encode_sort_key(name);
            self.pending_entries.push((
                parent_path.clone(),
                name.clone(),
                kind,
                sort_key,
                Some(*size_bytes),
                *mtime_ns,
            ));
        }

        self.pending_meta.push((parent_path, args.dir_mtime_ns));

        if self.pending_entries.len() >= BATCH_SIZE {
            self.flush()?;
        }

        Ok(())
    }

    /// 蓄積中のエントリを `SQLite` にフラッシュする
    pub(crate) fn flush(&mut self) -> Result<(), DirIndexError> {
        if self.pending_entries.is_empty() && self.pending_meta.is_empty() {
            return Ok(());
        }

        let tx = self.conn.unchecked_transaction()?;
        {
            // エントリ挿入
            if !self.pending_entries.is_empty() {
                let mut stmt = tx.prepare_cached(
                    "INSERT OR REPLACE INTO dir_entries \
                         (parent_path, name, kind, sort_key, size_bytes, mtime_ns) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )?;
                for (parent_path, name, kind, sort_key, size_bytes, mtime_ns) in
                    &self.pending_entries
                {
                    stmt.execute(params![
                        parent_path,
                        name,
                        kind,
                        sort_key,
                        size_bytes,
                        mtime_ns
                    ])?;
                }
            }

            // メタデータ挿入
            if !self.pending_meta.is_empty() {
                let mut stmt = tx.prepare_cached(
                    "INSERT OR REPLACE INTO dir_meta (path, mtime_ns) VALUES (?1, ?2)",
                )?;
                for (path, mtime_ns) in &self.pending_meta {
                    stmt.execute(params![path, mtime_ns])?;
                }
            }
        }
        tx.commit()?;

        self.pending_entries.clear();
        self.pending_meta.clear();

        Ok(())
    }
}

impl Drop for BulkInserter {
    fn drop(&mut self) {
        // 残りのエントリをフラッシュ (エラーはログのみ)
        if !self.pending_entries.is_empty() || !self.pending_meta.is_empty() {
            if let Err(e) = self.flush() {
                tracing::error!("BulkInserter drop 時のフラッシュ失敗: {e}");
            }
        }
    }
}

// ===================================================================
// ヘルパー関数
// ===================================================================

/// `WalkCallbackArgs` から `parent_path` を構築する
///
/// `"{mount_id}/relative/path"` 形式。ルート直下の場合は `mount_id` のみ。
fn build_parent_path(args: &WalkCallbackArgs) -> String {
    let walk_path = Path::new(&args.walk_entry_path);
    let root_path = Path::new(&args.root_dir);

    let rel = walk_path
        .strip_prefix(root_path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();

    if rel.is_empty() || rel == "." {
        args.mount_id.clone()
    } else {
        format!("{}/{rel}", args.mount_id)
    }
}

/// ファイル名から種別文字列を返す (全種別を分類)
///
/// `DirIndex` は Indexer と異なり画像・other も含めて全エントリを格納する。
fn classify_kind(name: &str) -> &'static str {
    let ext = extract_extension(name).to_lowercase();
    match EntryKind::from_extension(&ext) {
        EntryKind::Directory => "directory",
        EntryKind::Image => "image",
        EntryKind::Video => "video",
        EntryKind::Pdf => "pdf",
        EntryKind::Archive => "archive",
        EntryKind::Other => "other",
    }
}

/// `rusqlite::Row` から `DirEntry` にマッピングする
fn map_dir_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<DirEntry> {
    Ok(DirEntry {
        parent_path: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        sort_key: row.get(3)?,
        size_bytes: row.get(4)?,
        mtime_ns: row.get(5)?,
    })
}

/// name-asc カーソルから `(kind_flag, sort_key)` を分離する
///
/// カーソル形式: `"{kind_flag}\x00{sort_key}"` (`kind_flag`: "0"=directory, "1"=other)
fn parse_name_cursor(cursor: &str) -> (i64, &str) {
    if let Some(pos) = cursor.find('\x00') {
        let flag_str = &cursor[..pos];
        let sort_key = &cursor[pos + 1..];
        let flag: i64 = flag_str.parse().unwrap_or(1);
        (flag, sort_key)
    } else {
        // フォールバック: カーソル全体を sort_key として扱う
        (1, cursor)
    }
}

// ---------------------------------------------------------------
// ソートクエリ実装
// ---------------------------------------------------------------

/// ディレクトリ優先 + 名前昇順
#[allow(clippy::cast_possible_wrap, reason = "limit は i64 範囲内")]
fn query_name_asc(
    conn: &Connection,
    parent_path: &str,
    limit: usize,
    cursor: Option<&str>,
) -> Result<Vec<DirEntry>, DirIndexError> {
    let rows = if let Some(c) = cursor {
        let (kind_flag, sort_key) = parse_name_cursor(c);
        let mut stmt = conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 \
               AND (CASE WHEN kind = 'directory' THEN 0 ELSE 1 END, sort_key) > (?2, ?3) \
             ORDER BY CASE WHEN kind = 'directory' THEN 0 ELSE 1 END, sort_key ASC \
             LIMIT ?4",
        )?;
        stmt.query_map(
            params![parent_path, kind_flag, sort_key, limit as i64],
            map_dir_entry,
        )?
        .collect::<Result<Vec<_>, _>>()?
    } else {
        let mut stmt = conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 \
             ORDER BY CASE WHEN kind = 'directory' THEN 0 ELSE 1 END, sort_key ASC \
             LIMIT ?2",
        )?;
        stmt.query_map(params![parent_path, limit as i64], map_dir_entry)?
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(rows)
}

/// ディレクトリ優先 + 名前降順
#[allow(clippy::cast_possible_wrap, reason = "limit は i64 範囲内")]
fn query_name_desc(
    conn: &Connection,
    parent_path: &str,
    limit: usize,
    cursor: Option<&str>,
) -> Result<Vec<DirEntry>, DirIndexError> {
    let rows = if let Some(c) = cursor {
        let (kind_flag, sort_key) = parse_name_cursor(c);
        let mut stmt = conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 \
               AND (CASE WHEN kind = 'directory' THEN 0 ELSE 1 END, sort_key) < (?2, ?3) \
             ORDER BY CASE WHEN kind = 'directory' THEN 0 ELSE 1 END ASC, sort_key DESC \
             LIMIT ?4",
        )?;
        stmt.query_map(
            params![parent_path, kind_flag, sort_key, limit as i64],
            map_dir_entry,
        )?
        .collect::<Result<Vec<_>, _>>()?
    } else {
        let mut stmt = conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 \
             ORDER BY CASE WHEN kind = 'directory' THEN 0 ELSE 1 END ASC, sort_key DESC \
             LIMIT ?2",
        )?;
        stmt.query_map(params![parent_path, limit as i64], map_dir_entry)?
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(rows)
}

/// 日付降順 (新しい順)
#[allow(clippy::cast_possible_wrap, reason = "limit は i64 範囲内")]
fn query_date_desc(
    conn: &Connection,
    parent_path: &str,
    limit: usize,
    cursor: Option<&str>,
) -> Result<Vec<DirEntry>, DirIndexError> {
    let rows = if let Some(c) = cursor {
        let mtime: i64 = c
            .parse()
            .map_err(|e| DirIndexError::Other(format!("無効な date カーソル: {e}")))?;
        let mut stmt = conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 AND mtime_ns < ?2 \
             ORDER BY mtime_ns DESC \
             LIMIT ?3",
        )?;
        stmt.query_map(params![parent_path, mtime, limit as i64], map_dir_entry)?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let mut stmt = conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 \
             ORDER BY mtime_ns DESC \
             LIMIT ?2",
        )?;
        stmt.query_map(params![parent_path, limit as i64], map_dir_entry)?
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(rows)
}

/// 日付昇順 (古い順)
#[allow(clippy::cast_possible_wrap, reason = "limit は i64 範囲内")]
fn query_date_asc(
    conn: &Connection,
    parent_path: &str,
    limit: usize,
    cursor: Option<&str>,
) -> Result<Vec<DirEntry>, DirIndexError> {
    let rows = if let Some(c) = cursor {
        let mtime: i64 = c
            .parse()
            .map_err(|e| DirIndexError::Other(format!("無効な date カーソル: {e}")))?;
        let mut stmt = conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 AND mtime_ns > ?2 \
             ORDER BY mtime_ns ASC \
             LIMIT ?3",
        )?;
        stmt.query_map(params![parent_path, mtime, limit as i64], map_dir_entry)?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let mut stmt = conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 \
             ORDER BY mtime_ns ASC \
             LIMIT ?2",
        )?;
        stmt.query_map(params![parent_path, limit as i64], map_dir_entry)?
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(rows)
}

// ===================================================================
// テスト
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用の一時 DB パスで `DirIndex` を生成する
    fn setup() -> (DirIndex, tempfile::NamedTempFile) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let idx = DirIndex::new(tmp.path().to_str().unwrap());
        idx.init_db().unwrap();
        (idx, tmp)
    }

    /// テスト用の `WalkCallbackArgs` を生成する
    fn make_args(
        walk_path: &str,
        root_dir: &str,
        mount_id: &str,
        subdirs: Vec<(&str, i64)>,
        files: Vec<(&str, i64, i64)>,
    ) -> WalkCallbackArgs {
        WalkCallbackArgs {
            walk_entry_path: walk_path.to_owned(),
            root_dir: root_dir.to_owned(),
            mount_id: mount_id.to_owned(),
            dir_mtime_ns: 1_000_000_000,
            subdirs: subdirs
                .into_iter()
                .map(|(n, m)| (n.to_owned(), m))
                .collect(),
            files: files
                .into_iter()
                .map(|(n, s, m)| (n.to_owned(), s, m))
                .collect(),
        }
    }

    #[test]
    fn init_dbでスキーマが作成される() {
        let (idx, _tmp) = setup();
        let count = idx.entry_count().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn ingest_walk_entryでエントリが保存される() {
        let (idx, _tmp) = setup();

        let args = make_args(
            "/data/photos",
            "/data",
            "mount1",
            vec![("subdir", 2_000_000_000)],
            vec![
                ("image1.jpg", 1024, 3_000_000_000),
                ("archive.zip", 2048, 4_000_000_000),
            ],
        );
        idx.ingest_walk_entry(&args).unwrap();

        // 3 エントリ (subdir + image1.jpg + archive.zip) が保存される
        let entries = idx
            .query_page("mount1/photos", "name-asc", 100, None)
            .unwrap();
        assert_eq!(entries.len(), 3);

        // ディレクトリが先頭
        assert_eq!(entries[0].name, "subdir");
        assert_eq!(entries[0].kind, "directory");

        // ファイルが後に続く (自然順)
        assert_eq!(entries[1].name, "archive.zip");
        assert_eq!(entries[1].kind, "archive");

        assert_eq!(entries[2].name, "image1.jpg");
        assert_eq!(entries[2].kind, "image");
    }

    #[test]
    fn query_pageでname_ascソートが自然順で返る() {
        let (idx, _tmp) = setup();

        let args = make_args(
            "/data",
            "/data",
            "m",
            vec![],
            vec![
                ("file10.jpg", 100, 1_000_000),
                ("file1.jpg", 100, 2_000_000),
                ("file2.jpg", 100, 3_000_000),
            ],
        );
        idx.ingest_walk_entry(&args).unwrap();

        let entries = idx.query_page("m", "name-asc", 100, None).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["file1.jpg", "file2.jpg", "file10.jpg"]);
    }

    #[test]
    fn query_pageでカーソルページネーションが動作する() {
        let (idx, _tmp) = setup();

        let args = make_args(
            "/data",
            "/data",
            "m",
            vec![],
            vec![
                ("a.jpg", 100, 1_000_000),
                ("b.jpg", 100, 2_000_000),
                ("c.jpg", 100, 3_000_000),
            ],
        );
        idx.ingest_walk_entry(&args).unwrap();

        // 1 件目を取得
        let page1 = idx.query_page("m", "name-asc", 1, None).unwrap();
        assert_eq!(page1.len(), 1);
        assert_eq!(page1[0].name, "a.jpg");

        // カーソルを使って 2 件目を取得
        // kind_flag=1 (non-directory) + sort_key
        let cursor = format!("1\x00{}", page1[0].sort_key);
        let page2 = idx.query_page("m", "name-asc", 1, Some(&cursor)).unwrap();
        assert_eq!(page2.len(), 1);
        assert_eq!(page2[0].name, "b.jpg");

        // 3 件目
        let cursor2 = format!("1\x00{}", page2[0].sort_key);
        let page3 = idx.query_page("m", "name-asc", 1, Some(&cursor2)).unwrap();
        assert_eq!(page3.len(), 1);
        assert_eq!(page3[0].name, "c.jpg");

        // 4 件目は空
        let cursor3 = format!("1\x00{}", page3[0].sort_key);
        let page4 = idx.query_page("m", "name-asc", 1, Some(&cursor3)).unwrap();
        assert!(page4.is_empty());
    }

    #[test]
    fn child_countが正しく返る() {
        let (idx, _tmp) = setup();

        let args = make_args(
            "/data",
            "/data",
            "m",
            vec![("sub1", 1_000_000)],
            vec![("a.jpg", 100, 2_000_000), ("b.png", 200, 3_000_000)],
        );
        idx.ingest_walk_entry(&args).unwrap();

        assert_eq!(idx.child_count("m").unwrap(), 3);
        assert_eq!(idx.child_count("nonexistent").unwrap(), 0);
    }

    #[test]
    fn preview_entriesが画像とアーカイブを返す() {
        let (idx, _tmp) = setup();

        let args = make_args(
            "/data",
            "/data",
            "m",
            vec![("subdir", 1_000_000)],
            vec![
                ("photo.jpg", 100, 2_000_000),
                ("readme.txt", 50, 3_000_000),
                ("comic.zip", 500, 4_000_000),
                ("movie.mp4", 1000, 5_000_000),
            ],
        );
        idx.ingest_walk_entry(&args).unwrap();

        let previews = idx.preview_entries("m", 10).unwrap();
        let kinds: Vec<&str> = previews.iter().map(|e| e.kind.as_str()).collect();
        // directory と other (txt) は含まれない
        assert!(!kinds.contains(&"directory"));
        assert!(!kinds.contains(&"other"));
        assert_eq!(previews.len(), 3); // photo.jpg, comic.zip, movie.mp4
    }

    #[test]
    fn dir_mtimeの保存と取得() {
        let (idx, _tmp) = setup();

        // 未登録の場合 None
        assert_eq!(idx.get_dir_mtime("some/path").unwrap(), None);

        // 保存後は値が返る
        idx.set_dir_mtime("some/path", 12345).unwrap();
        assert_eq!(idx.get_dir_mtime("some/path").unwrap(), Some(12345));

        // 上書き
        idx.set_dir_mtime("some/path", 99999).unwrap();
        assert_eq!(idx.get_dir_mtime("some/path").unwrap(), Some(99999));
    }

    #[test]
    fn bulk_inserterでバッチ保存される() {
        let (idx, _tmp) = setup();
        let mut bulk = idx.begin_bulk().unwrap();

        let args1 = make_args(
            "/data/dir1",
            "/data",
            "m",
            vec![],
            vec![("a.jpg", 100, 1_000_000), ("b.png", 200, 2_000_000)],
        );
        let args2 = make_args(
            "/data/dir2",
            "/data",
            "m",
            vec![],
            vec![("c.jpg", 300, 3_000_000)],
        );

        bulk.ingest_walk_entry(&args1).unwrap();
        bulk.ingest_walk_entry(&args2).unwrap();
        bulk.flush().unwrap();

        // DirIndex 経由で確認
        assert_eq!(idx.entry_count().unwrap(), 3);
        assert_eq!(idx.child_count("m/dir1").unwrap(), 2);
        assert_eq!(idx.child_count("m/dir2").unwrap(), 1);
    }

    #[test]
    fn is_full_scan_doneのフラグ管理() {
        let (idx, _tmp) = setup();

        assert!(!idx.is_full_scan_done().unwrap());
        idx.mark_full_scan_done().unwrap();
        assert!(idx.is_full_scan_done().unwrap());
    }

    #[test]
    fn mark_readyとmark_warm_startの状態遷移() {
        let idx = DirIndex::new(":memory:");

        // 初期状態
        assert!(!idx.is_ready());
        assert!(!idx.is_stale());

        // ウォームスタート
        idx.mark_warm_start();
        assert!(idx.is_ready());
        assert!(idx.is_stale());

        // 準備完了
        idx.mark_ready();
        assert!(idx.is_ready());
        assert!(!idx.is_stale());
    }

    #[test]
    fn date_descソートとカーソル() {
        let (idx, _tmp) = setup();

        let args = make_args(
            "/data",
            "/data",
            "m",
            vec![],
            vec![
                ("old.jpg", 100, 1_000_000),
                ("mid.jpg", 100, 2_000_000),
                ("new.jpg", 100, 3_000_000),
            ],
        );
        idx.ingest_walk_entry(&args).unwrap();

        // 新しい順
        let page1 = idx.query_page("m", "date-desc", 2, None).unwrap();
        assert_eq!(page1.len(), 2);
        assert_eq!(page1[0].name, "new.jpg");
        assert_eq!(page1[1].name, "mid.jpg");

        // カーソルで次ページ
        let cursor = page1[1].mtime_ns.to_string();
        let page2 = idx.query_page("m", "date-desc", 2, Some(&cursor)).unwrap();
        assert_eq!(page2.len(), 1);
        assert_eq!(page2[0].name, "old.jpg");
    }

    #[test]
    fn ルートディレクトリのparent_pathがmount_idになる() {
        let (idx, _tmp) = setup();

        let args = make_args(
            "/data",
            "/data",
            "myMount",
            vec![("sub", 1_000_000)],
            vec![("file.jpg", 100, 2_000_000)],
        );
        idx.ingest_walk_entry(&args).unwrap();

        // ルート直下は mount_id がそのまま parent_path
        let entries = idx.query_page("myMount", "name-asc", 100, None).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].parent_path, "myMount");
    }

    // --- first_entry_by_kind ---

    #[test]
    fn first_entry_by_kindがarchiveを優先して返す() {
        let (idx, _tmp) = setup();

        let args = make_args(
            "/data/photos",
            "/data",
            "m1",
            vec![],
            vec![
                ("image1.jpg", 100, 1_000_000_000),
                ("archive.zip", 200, 2_000_000_000),
                ("doc.pdf", 300, 3_000_000_000),
            ],
        );
        idx.ingest_walk_entry(&args).unwrap();

        // archive が最初に見つかる
        let entry = idx.first_entry_by_kind("m1/photos", "archive").unwrap();
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().name, "archive.zip");
    }

    #[test]
    fn first_entry_by_kindで該当なしはnoneを返す() {
        let (idx, _tmp) = setup();

        let args = make_args(
            "/data/photos",
            "/data",
            "m1",
            vec![],
            vec![("image1.jpg", 100, 1_000_000_000)],
        );
        idx.ingest_walk_entry(&args).unwrap();

        let entry = idx.first_entry_by_kind("m1/photos", "archive").unwrap();
        assert!(entry.is_none());
    }

    // --- find_sibling_entry ---

    #[test]
    fn 次の兄弟をkindフィルタ付きで取得できる() {
        let (idx, _tmp) = setup();

        let args = make_args(
            "/data",
            "/data",
            "m1",
            vec![("a_dir", 1_000_000), ("c_dir", 3_000_000)],
            vec![("b_file.jpg", 100, 2_000_000_000)],
        );
        idx.ingest_walk_entry(&args).unwrap();

        // a_dir の次の directory は c_dir (b_file.jpg はスキップ)
        let a_sort_key = {
            let entries = idx.query_page("m1", "name-asc", 10, None).unwrap();
            entries
                .iter()
                .find(|e| e.name == "a_dir")
                .unwrap()
                .sort_key
                .clone()
        };

        let next = idx
            .find_sibling_entry("m1", &a_sort_key, "next", &["directory"])
            .unwrap();
        assert!(next.is_some());
        assert_eq!(next.unwrap().name, "c_dir");
    }

    #[test]
    fn 前の兄弟をkindフィルタ付きで取得できる() {
        let (idx, _tmp) = setup();

        let args = make_args(
            "/data",
            "/data",
            "m1",
            vec![("a_dir", 1_000_000), ("c_dir", 3_000_000)],
            vec![("b_file.jpg", 100, 2_000_000_000)],
        );
        idx.ingest_walk_entry(&args).unwrap();

        let c_sort_key = {
            let entries = idx.query_page("m1", "name-asc", 10, None).unwrap();
            entries
                .iter()
                .find(|e| e.name == "c_dir")
                .unwrap()
                .sort_key
                .clone()
        };

        let prev = idx
            .find_sibling_entry("m1", &c_sort_key, "prev", &["directory"])
            .unwrap();
        assert!(prev.is_some());
        assert_eq!(prev.unwrap().name, "a_dir");
    }

    #[test]
    fn 該当なしでnoneを返す() {
        let (idx, _tmp) = setup();

        let args = make_args(
            "/data",
            "/data",
            "m1",
            vec![("only_dir", 1_000_000)],
            vec![],
        );
        idx.ingest_walk_entry(&args).unwrap();

        let sort_key = {
            let entries = idx.query_page("m1", "name-asc", 10, None).unwrap();
            entries[0].sort_key.clone()
        };

        let next = idx
            .find_sibling_entry("m1", &sort_key, "next", &["directory"])
            .unwrap();
        assert!(next.is_none());
    }
}
