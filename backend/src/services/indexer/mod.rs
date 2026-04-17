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
    #[allow(clippy::cast_possible_wrap, reason = "エントリ数は i64 範囲内")]
    pub(crate) fn entry_count(&self) -> Result<usize, IndexerError> {
        let conn = self.connect()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?;
        #[allow(clippy::cast_sign_loss, reason = "COUNT(*) は非負")]
        Ok(count as usize)
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
mod tests {
    use super::*;
    use crate::services::path_security::PathSecurity;

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

        let (hits, has_more) = indexer
            .search(&SearchParams {
                query: "sunset",
                kind: None,
                limit: 10,
                offset: 0,
                scope_prefix: None,
            })
            .unwrap();
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
        let (hits, _) = indexer
            .search(&SearchParams {
                query: "clip",
                kind: Some("video"),
                limit: 10,
                offset: 0,
                scope_prefix: None,
            })
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "video");

        // kind="pdf" で同じクエリ — ヒットしない
        let (hits, _) = indexer
            .search(&SearchParams {
                query: "clip",
                kind: Some("pdf"),
                limit: 10,
                offset: 0,
                scope_prefix: None,
            })
            .unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn エントリの削除で検索から消える() {
        let (indexer, _tmp) = setup_indexer();

        let entry = make_entry("photos/beach.jpg", "beach.jpg", "image");
        indexer.add_entry(&entry).unwrap();

        // 削除前: 検索にヒットする
        let (hits, _) = indexer
            .search(&SearchParams {
                query: "beach",
                kind: None,
                limit: 10,
                offset: 0,
                scope_prefix: None,
            })
            .unwrap();
        assert_eq!(hits.len(), 1);

        // 削除
        indexer.remove_entry("photos/beach.jpg").unwrap();

        // 削除後: 検索にヒットしない
        let (hits, _) = indexer
            .search(&SearchParams {
                query: "beach",
                kind: None,
                limit: 10,
                offset: 0,
                scope_prefix: None,
            })
            .unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn 二文字クエリでlikeフォールバック() {
        let (indexer, _tmp) = setup_indexer();

        let entry = make_entry("tests/ab_test.mp4", "ab_test.mp4", "video");
        indexer.add_entry(&entry).unwrap();

        // "ab" は 2 文字 → LIKE フォールバック
        let (hits, _) = indexer
            .search(&SearchParams {
                query: "ab",
                kind: None,
                limit: 10,
                offset: 0,
                scope_prefix: None,
            })
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "ab_test.mp4");
    }

    #[test]
    fn 日本語ファイル名の部分一致検索() {
        let (indexer, _tmp) = setup_indexer();

        let entry = make_entry("動画/テスト動画.mp4", "テスト動画.mp4", "video");
        indexer.add_entry(&entry).unwrap();

        // "テスト" は 3 文字 → FTS5 パス
        let (hits, _) = indexer
            .search(&SearchParams {
                query: "テスト",
                kind: None,
                limit: 10,
                offset: 0,
                scope_prefix: None,
            })
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "テスト動画.mp4");
    }

    #[test]
    fn 特殊文字入力でエラーにならない() {
        let (indexer, _tmp) = setup_indexer();

        // ダブルクォートやアスタリスクを含むクエリでエラーにならない
        let result = indexer.search(&SearchParams {
            query: "\"test*",
            kind: None,
            limit: 10,
            offset: 0,
            scope_prefix: None,
        });
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
        let (hits, has_more) = indexer
            .search(&SearchParams {
                query: "image",
                kind: None,
                limit: 2,
                offset: 0,
                scope_prefix: None,
            })
            .unwrap();
        assert_eq!(hits.len(), 2);
        assert!(has_more);

        // limit=10 で検索 → has_more=false
        let (hits, has_more) = indexer
            .search(&SearchParams {
                query: "image",
                kind: None,
                limit: 10,
                offset: 0,
                scope_prefix: None,
            })
            .unwrap();
        assert_eq!(hits.len(), 3);
        assert!(!has_more);
    }

    // --- scope_prefix テスト ---

    #[test]
    fn scope_prefix付きでfts検索がプレフィックス一致のみ返す() {
        let (indexer, _tmp) = setup_indexer();

        indexer
            .add_entry(&make_entry(
                "mount1/photos/sunset.jpg",
                "sunset.jpg",
                "image",
            ))
            .unwrap();
        indexer
            .add_entry(&make_entry(
                "mount1/videos/sunset.mp4",
                "sunset.mp4",
                "video",
            ))
            .unwrap();
        indexer
            .add_entry(&make_entry(
                "mount2/photos/sunset.png",
                "sunset.png",
                "image",
            ))
            .unwrap();

        // scope_prefix="mount1/photos" → mount1/photos 配下のみ
        let (hits, _) = indexer
            .search(&SearchParams {
                query: "sunset",
                kind: None,
                limit: 10,
                offset: 0,
                scope_prefix: Some("mount1/photos"),
            })
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].relative_path, "mount1/photos/sunset.jpg");
    }

    #[test]
    fn scope_prefix付きでlike検索がプレフィックス一致のみ返す() {
        let (indexer, _tmp) = setup_indexer();

        indexer
            .add_entry(&make_entry("mount1/ab_test.jpg", "ab_test.jpg", "image"))
            .unwrap();
        indexer
            .add_entry(&make_entry("mount2/ab_other.jpg", "ab_other.jpg", "image"))
            .unwrap();

        // "ab" は 2 文字 → LIKE フォールバック + scope_prefix
        let (hits, _) = indexer
            .search(&SearchParams {
                query: "ab",
                kind: None,
                limit: 10,
                offset: 0,
                scope_prefix: Some("mount1"),
            })
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].relative_path, "mount1/ab_test.jpg");
    }

    #[test]
    fn scope_prefix内のlikeワイルドカードがエスケープされる() {
        let (indexer, _tmp) = setup_indexer();

        // ディレクトリ名に % と _ を含む
        indexer
            .add_entry(&make_entry("mount/dir_100%/test.jpg", "test.jpg", "image"))
            .unwrap();
        indexer
            .add_entry(&make_entry("mount/dir_200/test.jpg", "test.jpg", "image"))
            .unwrap();

        // scope_prefix に % を含む → エスケープされて literal match
        let (hits, _) = indexer
            .search(&SearchParams {
                query: "test",
                kind: None,
                limit: 10,
                offset: 0,
                scope_prefix: Some("mount/dir_100%"),
            })
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].relative_path, "mount/dir_100%/test.jpg");
    }

    #[test]
    fn scope_prefixがnoneの場合はフィルタなしで全件返す() {
        let (indexer, _tmp) = setup_indexer();

        indexer
            .add_entry(&make_entry("mount1/photo.jpg", "photo.jpg", "image"))
            .unwrap();
        indexer
            .add_entry(&make_entry("mount2/photo.jpg", "photo.jpg", "image"))
            .unwrap();

        let (hits, _) = indexer
            .search(&SearchParams {
                query: "photo",
                kind: None,
                limit: 10,
                offset: 0,
                scope_prefix: None,
            })
            .unwrap();
        assert_eq!(hits.len(), 2);
    }

    // --- escape_like_pattern テスト ---

    #[test]
    fn likeパターンのワイルドカードがエスケープされる() {
        use super::helpers::escape_like_pattern;
        assert_eq!(escape_like_pattern("normal/path"), "normal/path");
        assert_eq!(escape_like_pattern("dir_100%"), "dir\\_100\\%");
        assert_eq!(escape_like_pattern("back\\slash"), "back\\\\slash");
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
    fn incremental_scanでネストされたディレクトリ内の新規ファイルを検出する() {
        // root/dir1/dir2/movie.mp4 を作成
        let dir = TempDir::new().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::create_dir_all(root.join("dir1/dir2")).unwrap();
        fs::write(root.join("dir1/dir2/movie.mp4"), b"video").unwrap();

        let db_file = tempfile::NamedTempFile::new().unwrap();
        let indexer = Indexer::new(db_file.path().to_str().unwrap());
        indexer.init_db().unwrap();
        let ps = PathSecurity::new(vec![root.clone()], false).unwrap();

        // 初回フルスキャン: dir1 + dir2 + movie.mp4 = 3
        indexer.scan_directory(&root, &ps, "m", 2, None).unwrap();
        assert_eq!(indexer.entry_count().unwrap(), 3);

        // dir1 の mtime を記録
        let dir1_mtime = fs::metadata(root.join("dir1")).unwrap().modified().unwrap();

        // dir2 にファイル追加 (dir2 の mtime は変わるが dir1 の mtime は変わらない)
        fs::write(root.join("dir1/dir2/new.mp4"), b"new video").unwrap();

        // dir1 の mtime を元に戻す (テスト環境の安全策)
        let times = std::fs::FileTimes::new().set_modified(dir1_mtime);
        std::fs::File::open(root.join("dir1"))
            .unwrap()
            .set_times(times)
            .unwrap();

        // dir1 の mtime が変わっていないことを確認
        let dir1_mtime_after = fs::metadata(root.join("dir1")).unwrap().modified().unwrap();
        assert_eq!(dir1_mtime, dir1_mtime_after);

        // incremental_scan で new.mp4 が検出されるべき
        let (added, _updated, deleted) =
            indexer.incremental_scan(&root, &ps, "m", 2, None).unwrap();

        assert!(added >= 1, "new.mp4 が追加されるべき (added={added})");
        assert_eq!(deleted, 0, "削除はないはず");
        // dir1 + dir2 + movie.mp4 + new.mp4 = 4
        assert_eq!(indexer.entry_count().unwrap(), 4);
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

    #[test]
    fn list_entry_pathsは空テーブルで空vecを返す() {
        let (indexer, _tmp) = setup_indexer();
        let paths = indexer.list_entry_paths().unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn list_entry_pathsは登録済みrelative_pathを返す() {
        let (indexer, _tmp) = setup_indexer();
        indexer
            .add_entry(&make_entry(
                "mount_a/photos/sunset.jpg",
                "sunset.jpg",
                "image",
            ))
            .unwrap();
        indexer
            .add_entry(&make_entry("mount_a/videos/clip.mp4", "clip.mp4", "video"))
            .unwrap();
        indexer
            .add_entry(&make_entry("mount_b/docs/manual.pdf", "manual.pdf", "pdf"))
            .unwrap();

        let mut paths = indexer.list_entry_paths().unwrap();
        paths.sort();
        assert_eq!(
            paths,
            vec![
                "mount_a/photos/sunset.jpg".to_string(),
                "mount_a/videos/clip.mp4".to_string(),
                "mount_b/docs/manual.pdf".to_string(),
            ]
        );
    }
}
