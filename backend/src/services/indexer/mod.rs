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

/// 検索パラメータ
pub(crate) struct SearchParams<'a> {
    pub query: &'a str,
    pub kind: Option<&'a str>,
    pub limit: usize,
    pub offset: usize,
    /// ディレクトリスコープ: `{mount_id}/{relative}` 形式のプレフィックス
    pub scope_prefix: Option<&'a str>,
}

use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{Connection, params};

use helpers::{build_fingerprint, build_fts_query, escape_like_pattern, search_fts, search_like};

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

        let fts_query = build_fts_query(trimmed);
        let fetch_limit = params.limit + 1;

        // scope_prefix のワイルドカードエスケープ
        let scope_pattern = params.scope_prefix.map(|prefix| {
            let escaped = escape_like_pattern(prefix);
            format!("{escaped}/%")
        });

        let mut hits = if fts_query.is_empty() {
            search_like(
                &conn,
                trimmed,
                params.kind,
                fetch_limit,
                params.offset,
                scope_pattern.as_deref(),
            )?
        } else {
            search_fts(
                &conn,
                &fts_query,
                params.kind,
                fetch_limit,
                params.offset,
                scope_pattern.as_deref(),
            )?
        };

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
}

#[cfg(test)]
mod tests;
