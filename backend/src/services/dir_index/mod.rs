//! ディレクトリリスティングインデックス
//!
//! browse API の高速化のため、ディレクトリの子エントリを `SQLite` に事前保存する。
//! `sort_key` による自然順ソートと、カーソルベースのシーク型ページネーションを提供。
//!
//! モジュール構成:
//! - `schema`: テーブル/インデックス作成 + スキーママイグレーション (`init_db`)
//! - `writes`: `ingest_walk_entry` / `set_dir_mtime` / `begin_bulk` 等の書き込み系
//! - `queries`: `DirIndexReader` の読み取りクエリ本体
//! - `sort_queries`: name/date 各ソートの SQL プリミティブ
//! - `bulk_insert`: `BulkInserter` の実装
//! - `dirty_state`: 世代付き dirty セット

mod bulk_insert;
pub(crate) mod dirty_state;
mod queries;
mod schema;
mod sort_queries;
#[cfg(test)]
mod tests;
mod writes;

pub(crate) use bulk_insert::BulkInserter;

use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::Connection;

use dirty_state::DirtyState;

/// スキーマバージョン (v3: マイグレーション時に `dir_meta` + `full_scan_done` もクリア)
const SCHEMA_VERSION: &str = "3";

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
    dirty: std::sync::Mutex<DirtyState>,
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
            dirty: std::sync::Mutex::new(DirtyState::new()),
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

    // --- dirty セット委譲メソッド ---

    /// ディレクトリを dirty にマークし、世代番号を返す
    pub(crate) fn mark_dir_dirty(&self, parent_key: &str) -> u64 {
        self.dirty
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .mark_dirty(parent_key)
    }

    /// ディレクトリが dirty かどうか
    pub(crate) fn is_dir_dirty(&self, parent_key: &str) -> bool {
        self.dirty
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_dirty(parent_key)
    }

    /// 世代番号が一致する場合のみ dirty を解除する
    pub(crate) fn clear_dir_dirty_if_match(&self, parent_key: &str, generation: u64) -> bool {
        self.dirty
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear_if_generation_matches(parent_key, generation)
    }

    /// 全ディレクトリを dirty にマーク (inotify overflow 時)
    pub(crate) fn mark_all_dirs_dirty(&self, parent_keys: impl IntoIterator<Item = String>) {
        self.dirty
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .mark_all_dirty(parent_keys);
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

    /// ディレクトリの記録済み mtime を返す
    pub(crate) fn get_dir_mtime(&self, path: &str) -> Result<Option<i64>, DirIndexError> {
        self.reader()?.get_dir_mtime(path)
    }
}

// ===================================================================
// BulkInserter
// ===================================================================

/// バッチ挿入用のエントリ行
/// `(parent_path, name, kind, sort_key, size_bytes, mtime_ns)`
pub(super) type PendingEntry = (String, String, String, String, Option<i64>, i64);
