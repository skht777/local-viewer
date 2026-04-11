//! ディレクトリリスティングインデックス
//!
//! browse API の高速化のため、ディレクトリの子エントリを `SQLite` に事前保存する。
//! `sort_key` による自然順ソートと、カーソルベースのシーク型ページネーションを提供。

mod bulk_insert;
mod sort_queries;
#[cfg(test)]
mod tests;

pub(crate) use bulk_insert::BulkInserter;
use sort_queries::{
    build_parent_path, classify_kind, map_dir_entry, query_date_asc, query_date_desc,
    query_name_asc, query_name_desc, query_sibling_date, query_sibling_name,
};

use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{Connection, params};

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

/// ディレクトリの `child_count` + プレビューエントリ (`batch_dir_info` の戻り値)
#[derive(Debug)]
pub(crate) struct DirChildInfo {
    pub count: usize,
    pub previews: Vec<DirEntry>,
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

/// 読み取りセッション (1リクエスト内で Connection を再利用)
///
/// `DirIndex::reader()` で取得し、複数クエリを同一接続で実行する。
/// browse API のホットパスで `Connection::open` + PRAGMA の繰り返しを回避する。
pub(crate) struct DirIndexReader<'a> {
    _index: &'a DirIndex,
    conn: Connection,
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

    /// 計測ログ向けの状態ラベル
    ///
    /// - `cold`: 未 ready (初回フルスキャン中)
    /// - `warm_indexing`: ready かつ stale (差分スキャン中、既存データで応答可能)
    /// - `warm_ready`: ready かつ stale 解除済み (定常状態)
    pub(crate) fn state_label(&self) -> &'static str {
        match (self.is_ready(), self.is_stale()) {
            (false, _) => "cold",
            (true, true) => "warm_indexing",
            (true, false) => "warm_ready",
        }
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

    /// 読み取りセッションを開く (1リクエスト内で Connection を再利用)
    pub(crate) fn reader(&self) -> Result<DirIndexReader<'_>, DirIndexError> {
        Ok(DirIndexReader {
            _index: self,
            conn: self.connect()?,
        })
    }

    // ---------------------------------------------------------------
    // クエリ (各メソッドは DirIndexReader に委譲)
    // ---------------------------------------------------------------

    /// ソート + カーソルベースページネーション付きでエントリを返す
    ///
    /// - `sort`: `"name-asc"`, `"name-desc"`, `"date-asc"`, `"date-desc"`
    /// - `limit`: `Some(n)` で n 件、`None` で全件取得 (`SQLite` `LIMIT -1` 相当)
    /// - `cursor_sort_key`: 前ページ末尾のソートキー (name 系) または `mtime_ns` 文字列 (date 系)
    ///   name 系カーソルは `"{kind_flag}\x00{sort_key}"` 形式 (`kind_flag`: "0"=directory, "1"=other)
    pub(crate) fn query_page(
        &self,
        parent_path: &str,
        sort: &str,
        limit: Option<usize>,
        cursor_sort_key: Option<&str>,
    ) -> Result<Vec<DirEntry>, DirIndexError> {
        self.reader()?
            .query_page(parent_path, sort, limit, cursor_sort_key)
    }

    /// 指定ディレクトリの子エントリ数を返す
    pub(crate) fn child_count(&self, parent_path: &str) -> Result<usize, DirIndexError> {
        self.reader()?.child_count(parent_path)
    }

    /// サムネイル対象エントリを返す (画像/動画/PDF/アーカイブ)
    pub(crate) fn preview_entries(
        &self,
        parent_path: &str,
        limit: usize,
    ) -> Result<Vec<DirEntry>, DirIndexError> {
        self.reader()?.preview_entries(parent_path, limit)
    }

    /// 指定 kind の最初のエントリを返す (`first-viewable` 高速パス用)
    pub(crate) fn first_entry_by_kind(
        &self,
        parent_path: &str,
        kind: &str,
    ) -> Result<Option<DirEntry>, DirIndexError> {
        self.reader()?.first_entry_by_kind(parent_path, kind)
    }

    /// 次/前の兄弟エントリを返す (`sibling` 高速パス用)
    ///
    /// sort に応じて name 系 / date 系のクエリに分岐する。
    /// `direction` は `"next"` or `"prev"`。
    /// `kinds` で対象 kind をフィルタ。
    #[allow(clippy::too_many_arguments, reason = "sort 分岐に必要なパラメータ群")]
    pub(crate) fn query_sibling(
        &self,
        parent_path: &str,
        current_name: &str,
        current_is_dir: bool,
        direction: &str,
        sort: &str,
        kinds: &[&str],
    ) -> Result<Option<DirEntry>, DirIndexError> {
        self.reader()?.query_sibling(
            parent_path,
            current_name,
            current_is_dir,
            direction,
            sort,
            kinds,
        )
    }

    /// DB 内の全エントリ数を返す
    pub(crate) fn entry_count(&self) -> Result<usize, DirIndexError> {
        self.reader()?.entry_count()
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
        self.reader()?.get_dir_mtime(path)
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
pub(super) type PendingEntry = (String, String, String, String, Option<i64>, i64);

// ===================================================================
// DirIndexReader
// ===================================================================

impl DirIndexReader<'_> {
    /// ソート + カーソルベースページネーション付きでエントリを返す
    ///
    /// `limit = None` は全件取得 (`SQLite` `LIMIT -1` 相当)
    pub(crate) fn query_page(
        &self,
        parent_path: &str,
        sort: &str,
        limit: Option<usize>,
        cursor_sort_key: Option<&str>,
    ) -> Result<Vec<DirEntry>, DirIndexError> {
        match sort {
            "name-desc" => query_name_desc(&self.conn, parent_path, limit, cursor_sort_key),
            "date-desc" => query_date_desc(&self.conn, parent_path, limit, cursor_sort_key),
            "date-asc" => query_date_asc(&self.conn, parent_path, limit, cursor_sort_key),
            _ => query_name_asc(&self.conn, parent_path, limit, cursor_sort_key),
        }
    }

    /// 指定ディレクトリの子エントリ数を返す
    #[allow(clippy::cast_sign_loss, reason = "COUNT(*) は非負")]
    pub(crate) fn child_count(&self, parent_path: &str) -> Result<usize, DirIndexError> {
        let count: i64 = self.conn.query_row(
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
        let mut stmt = self.conn.prepare(
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
        let mut stmt = self.conn.prepare(
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
    #[allow(clippy::too_many_arguments, reason = "sort 分岐に必要なパラメータ群")]
    pub(crate) fn query_sibling(
        &self,
        parent_path: &str,
        current_name: &str,
        current_is_dir: bool,
        direction: &str,
        sort: &str,
        kinds: &[&str],
    ) -> Result<Option<DirEntry>, DirIndexError> {
        let current_sort_key = encode_sort_key(current_name);

        match sort {
            "name-asc" | "name-desc" => query_sibling_name(
                &self.conn,
                parent_path,
                &current_sort_key,
                current_is_dir,
                direction,
                sort,
                kinds,
            ),
            "date-asc" | "date-desc" => query_sibling_date(
                &self.conn,
                parent_path,
                current_name,
                direction,
                sort,
                kinds,
            ),
            _ => query_sibling_name(
                &self.conn,
                parent_path,
                &current_sort_key,
                current_is_dir,
                direction,
                "name-asc",
                kinds,
            ),
        }
    }

    /// DB 内の全エントリ数を返す
    #[allow(clippy::cast_sign_loss, reason = "COUNT(*) は非負")]
    pub(crate) fn entry_count(&self) -> Result<usize, DirIndexError> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM dir_entries", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// ディレクトリの記録済み mtime を返す
    #[allow(
        clippy::unnecessary_wraps,
        reason = "DirIndex::get_dir_mtime と同じシグネチャを維持"
    )]
    pub(crate) fn get_dir_mtime(&self, path: &str) -> Result<Option<i64>, DirIndexError> {
        let result = self
            .conn
            .query_row(
                "SELECT mtime_ns FROM dir_meta WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .ok();
        Ok(result)
    }

    /// 複数ディレクトリの `child_count` + `preview_entries` を一括取得する
    ///
    /// `parent_keys` の各ディレクトリについて `DirChildInfo` を返す。
    /// 存在しないキーは結果に含まれない。
    #[allow(clippy::cast_sign_loss, reason = "COUNT(*) は非負")]
    pub(crate) fn batch_dir_info(
        &self,
        parent_keys: &[&str],
        preview_limit: usize,
    ) -> Result<std::collections::HashMap<String, DirChildInfo>, DirIndexError> {
        use std::collections::HashMap;

        if parent_keys.is_empty() {
            return Ok(HashMap::new());
        }

        // 動的 IN 句のプレースホルダ構築
        let placeholders: Vec<String> = (1..=parent_keys.len()).map(|i| format!("?{i}")).collect();
        let in_clause = placeholders.join(", ");

        // child_count をバッチ取得
        let count_sql = format!(
            "SELECT parent_path, COUNT(*) FROM dir_entries \
             WHERE parent_path IN ({in_clause}) GROUP BY parent_path"
        );
        let mut count_stmt = self.conn.prepare(&count_sql)?;
        let params_vec: Vec<&dyn rusqlite::types::ToSql> = parent_keys
            .iter()
            .map(|k| k as &dyn rusqlite::types::ToSql)
            .collect();
        let count_rows = count_stmt.query_map(params_vec.as_slice(), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        let mut result: HashMap<String, DirChildInfo> = HashMap::new();
        for row in count_rows {
            let (path, count) = row?;
            result.insert(
                path,
                DirChildInfo {
                    count: count as usize,
                    previews: Vec::new(),
                },
            );
        }

        // preview_entries をウィンドウ関数でバッチ取得
        #[allow(
            clippy::cast_possible_wrap,
            reason = "preview_limit は小さい値 (通常3)"
        )]
        let limit_i64 = preview_limit as i64;
        let preview_sql = format!(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns FROM (\
                 SELECT *, ROW_NUMBER() OVER (\
                     PARTITION BY parent_path ORDER BY sort_key ASC\
                 ) AS rn \
                 FROM dir_entries \
                 WHERE parent_path IN ({in_clause}) \
                   AND kind IN ('image', 'archive', 'pdf', 'video')\
             ) WHERE rn <= ?{}",
            parent_keys.len() + 1
        );
        let mut preview_stmt = self.conn.prepare(&preview_sql)?;
        let mut preview_params: Vec<&dyn rusqlite::types::ToSql> = parent_keys
            .iter()
            .map(|k| k as &dyn rusqlite::types::ToSql)
            .collect();
        preview_params.push(&limit_i64);
        let preview_rows = preview_stmt.query_map(preview_params.as_slice(), map_dir_entry)?;

        for row in preview_rows {
            let entry = row?;
            let key = entry.parent_path.clone();
            result
                .entry(key)
                .or_insert_with(|| DirChildInfo {
                    count: 0,
                    previews: Vec::new(),
                })
                .previews
                .push(entry);
        }

        Ok(result)
    }
}
