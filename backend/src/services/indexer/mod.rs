//! `SQLite` FTS5 trigram 検索インデクサー
//!
//! ファイルシステムのエントリを `SQLite` に格納し、FTS5 trigram トークナイザで
//! ファイル名・相対パスの部分一致検索を提供する。
//!
//! - 3 文字以上のクエリ: FTS5 MATCH で高速検索
//! - 2 文字のクエリ: LIKE フォールバック
//! - 接続パターン: connection-per-call (WAL モード)
//! - 状態フラグ: `AtomicBool` でロックフリーの状態チェック

mod helpers;
mod scan;

pub(crate) use helpers::SearchHit;
// DirIndex など他 service から `{mount_id}/` の range scan キーを組み立てるための
// 再エクスポート。invariant (16 桁 lowercase hex) は `path_keys` 側で強制。
pub(crate) use crate::services::path_keys::mount_scope_range;

/// 検索結果のソート順
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SearchOrder {
    /// FTS5 bm25 ランキング（FTS 経路）/ name ASC（LIKE 経路）。既定値
    #[default]
    Relevance,
    /// 名前昇順（`lower(name)` で大小無視）
    NameAsc,
    /// 名前降順
    NameDesc,
    /// 更新日時昇順（`mtime_ns`）
    DateAsc,
    /// 更新日時降順
    DateDesc,
}

/// 検索パラメータ
pub(crate) struct SearchParams<'a> {
    pub query: &'a str,
    pub kind: Option<&'a str>,
    pub limit: usize,
    pub offset: usize,
    /// ディレクトリスコープ: `{mount_id}/{relative}` 形式のプレフィックス
    pub scope_prefix: Option<&'a str>,
    /// 結果のソート順（既定: `Relevance`）
    pub order: SearchOrder,
}

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{Connection, params};

use helpers::{build_fingerprint, search_combined};

/// バッチ INSERT のサイズ
const BATCH_SIZE: usize = 1000;

/// インデクサーエラー
#[derive(Debug, thiserror::Error)]
pub(crate) enum IndexerError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("{0}")]
    Other(String),
    /// `shutdown_token` による協調キャンセル
    ///
    /// - `parallel_walk` / `batch_insert` / mount ループで cancel 検知時に返る
    /// - 呼び出し側は readiness マークや fingerprint 更新を skip すべき
    #[error("scan cancelled due to shutdown")]
    Cancelled,
}

impl From<crate::services::path_keys::PathKeyError> for IndexerError {
    fn from(err: crate::services::path_keys::PathKeyError) -> Self {
        Self::Other(err.to_string())
    }
}

/// 検索インデックスに登録するエントリ
pub(crate) struct IndexEntry {
    pub relative_path: String,
    pub name: String,
    pub kind: String,
    pub size_bytes: Option<i64>,
    pub mtime_ns: i64,
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
}

impl Indexer {
    /// 新しいインデクサーを生成する (DB 未初期化状態)
    pub(crate) fn new(db_path: &str) -> Self {
        Self {
            db_path: db_path.to_owned(),
            is_ready: AtomicBool::new(false),
            is_stale: AtomicBool::new(false),
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
    /// クエリをスペース区切りで分割し、トークンごとにルーティング:
    /// - 3 文字以上のトークン → FTS5 MATCH
    /// - 2 文字以下のトークン → `name LIKE %t% OR relative_path LIKE %t%`
    ///
    /// 全トークンを SQL 上で AND 結合する (日本語 2 文字名詞 + 複数キーワードの
    /// 実用検索に対応)。
    ///
    /// - `limit + 1` 件取得して `has_more` を判定
    /// - `scope_prefix` 指定時はそのプレフィックス配下のみ検索
    pub(crate) fn search(
        &self,
        params: &SearchParams<'_>,
    ) -> Result<(Vec<SearchHit>, bool), IndexerError> {
        let conn = self.connect()?;
        let trimmed = params.query.trim();
        if trimmed.is_empty() {
            return Ok((Vec::new(), false));
        }

        let fetch_limit = params.limit + 1;

        // scope_prefix を range `(lo, hi)` に変換（BINARY range scan で SEARCH USING INDEX）
        let scope_range = params
            .scope_prefix
            .map(crate::services::path_keys::prefix_scope_range);

        let mut hits = search_combined(
            &conn,
            trimmed,
            params.kind,
            fetch_limit,
            params.offset,
            scope_range
                .as_ref()
                .map(|(lo, hi)| (lo.as_str(), hi.as_str())),
            params.order,
        )?;

        let has_more = hits.len() > params.limit;
        hits.truncate(params.limit);

        Ok((hits, has_more))
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
    pub(crate) fn entry_count(&self) -> Result<usize, IndexerError> {
        let conn = self.connect()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?;
        // COUNT(*) は非負なので try_from は成功する。
        // 万一 usize::MAX を超えても（64bit 環境ではあり得ない）clamp で安全側に倒す
        Ok(usize::try_from(count).unwrap_or(usize::MAX))
    }

    /// 永続化された全エントリの `relative_path` を列挙する
    ///
    /// - 起動時に `NodeRegistry` を rehydrate するための入力を提供
    /// - 戻り値の形式は `{mount_id}/{rest}`（`helpers::make_relative_prefix` と整合）
    /// - ディレクトリ / ファイル / アーカイブ等の区別はしない（kind フィルタなし）
    pub(crate) fn list_entry_paths(&self) -> Result<Vec<String>, IndexerError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT relative_path FROM entries")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut paths = Vec::new();
        for row in rows {
            paths.push(row?);
        }
        Ok(paths)
    }

    /// インデックスが使用可能かどうか
    pub(crate) fn is_ready(&self) -> bool {
        self.is_ready.load(Ordering::Relaxed)
    }

    /// インデックスが古い (再構築が必要) かどうか
    pub(crate) fn is_stale(&self) -> bool {
        self.is_stale.load(Ordering::Relaxed)
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

    /// 保存済みマウントフィンガープリントを削除する（warm partial 復旧用）
    ///
    /// - warm start で per-mount scan が部分失敗した場合、`incremental_scan` の
    ///   mtime 枝刈りにより `DirIndex` 欠損が埋まらないまま fingerprint が現構成と
    ///   一致し続ける。次回起動も warm start と判定され復旧不能になる問題を防ぐ
    /// - fingerprint を削除すると次回起動時に `check_mount_fingerprint` が false を
    ///   返し、cold start (fresh full scan) に落ちて確実に復旧できる
    /// - 未保存状態で呼んでも 0 件 DELETE で no-op（冪等）
    pub(crate) fn clear_mount_fingerprint(&self) -> Result<(), IndexerError> {
        let conn = self.connect()?;
        conn.execute(
            "DELETE FROM schema_meta WHERE key = 'mount_fingerprint'",
            [],
        )?;
        Ok(())
    }

    /// 保存済み fingerprint から旧 `mount_id` リストを復元する（all-or-nothing）
    ///
    /// - 全 token が `len == 16 && [0-9a-f]` を満たす場合のみ採用
    /// - 1 件でも不正 → `tracing::warn!` + 空 `Vec`（破損 fingerprint から意図せず
    ///   cleanup しない）
    /// - 重複は `BTreeSet` で排除、空文字 token は除外
    /// - `QueryReturnedNoRows` は `None` に正規化、それ以外の `Sqlite` エラーは
    ///   `IndexerError::Sqlite` で伝播（silent skip を禁止、
    ///   `check_mount_fingerprint` との観測性不一致を回避）
    pub(crate) fn load_stored_mount_ids(&self) -> Result<Vec<String>, IndexerError> {
        let conn = self.connect()?;
        let raw: Option<String> = match conn.query_row(
            "SELECT value FROM schema_meta WHERE key = 'mount_fingerprint'",
            [],
            |r| r.get(0),
        ) {
            Ok(v) => Some(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(IndexerError::Sqlite(e)),
        };
        let Some(raw) = raw else {
            return Ok(Vec::new());
        };
        let tokens: Vec<&str> = raw.split(',').filter(|t| !t.is_empty()).collect();
        let valid = !tokens.is_empty()
            && tokens.iter().all(|t| {
                t.len() == 16
                    && t.bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            });
        if !valid {
            tracing::warn!(
                raw_len = raw.len(),
                "破損した mount_fingerprint 検出、cleanup をスキップ"
            );
            return Ok(Vec::new());
        }
        let uniq: BTreeSet<String> = tokens.into_iter().map(String::from).collect();
        Ok(uniq.into_iter().collect())
    }

    /// 指定 `mount_id` 配下の `entries` 行を range scan で一括削除する
    ///
    /// - `mount_scope_range` で BINARY range scan → `idx_entries_relative_path`
    ///   を利用（`SCAN entries` にならない）
    /// - FTS trigger (`entries_ad`) が連動するため `entries_fts` 側にも
    ///   削除行数と同数の DELETE が発行される
    /// - 返値は削除行数
    pub(crate) fn delete_mount_entries(&self, mount_id: &str) -> Result<usize, IndexerError> {
        let (lo, hi) = mount_scope_range(mount_id)?;
        let conn = self.connect()?;
        let deleted = conn.execute(
            "DELETE FROM entries WHERE relative_path >= ?1 AND relative_path < ?2",
            params![lo, hi],
        )?;
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod perf_bench;
