//! `SQLite` FTS5 trigram 検索インデクサー
//!
//! ファイルシステムのエントリを `SQLite` に格納し、FTS5 trigram トークナイザで
//! ファイル名・相対パスの部分一致検索を提供する。
//!
//! - 3 文字以上のクエリ: FTS5 MATCH で高速検索
//! - 2 文字のクエリ: LIKE フォールバック
//! - 接続パターン: connection-per-call (WAL モード)
//! - 状態フラグ: `AtomicBool` でロックフリーの状態チェック

use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{Connection, params};

/// FTS5 trigram トークナイザが要求する最小文字数
const TRIGRAM_MIN_CHARS: usize = 3;

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
}
