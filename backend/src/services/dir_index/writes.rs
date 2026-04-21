//! `DirIndex` の書き込み操作
//!
//! - `ingest_walk_entry`: `parallel_walk` の 1 エントリを DB に格納
//! - `set_dir_mtime`: ディレクトリの mtime を `dir_meta` に記録
//! - フルスキャン完了フラグ (`is_full_scan_done` / `mark_full_scan_done`)
//! - `begin_bulk`: `BulkInserter` を開く (`synchronous=OFF` で高速格納)

use rusqlite::{Connection, params};

use crate::services::indexer::WalkCallbackArgs;
use crate::services::natural_sort::encode_sort_key;
use crate::services::path_keys::mount_scope_range;

use super::sort_queries::{build_parent_path, classify_kind};
use super::{BATCH_SIZE, BulkInserter, DirIndex, DirIndexError};

impl DirIndex {
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

    /// ディレクトリの mtime を記録する
    pub(crate) fn set_dir_mtime(&self, path: &str, mtime_ns: i64) -> Result<(), DirIndexError> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT OR REPLACE INTO dir_meta (path, mtime_ns) VALUES (?1, ?2)",
            params![path, mtime_ns],
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

    /// 指定 `mount_id` 配下の `dir_entries` + `dir_meta` 行を一括削除する
    ///
    /// - 16 桁 lowercase hex invariant は `mount_scope_range` で検証
    ///   （違反は `DirIndexError::Other` で reject、traversal 防御を継承）
    /// - range は `[mount_id, mount_id + "0")` に拡張する：
    ///   - `Indexer::entries.relative_path` は常に `{mount_id}/{path}` 形式だが、
    ///     `DirIndex` は **ルート行 `parent_path = mount_id`** も格納するため
    ///     `[{mount_id}/, ...)` では取りこぼす
    ///   - `"0"` (0x30) は `"/"` (0x2F) の直後なので、`{mount_id}` と
    ///     `{mount_id}/...` の両方を含み、16 桁固定長なら他マウント衝突なし
    /// - BINARY range scan: `dir_entries` は `idx_dir_parent`、`dir_meta` は
    ///   TEXT PRIMARY KEY の auto-index (`sqlite_autoindex_dir_meta_1`) で SEARCH
    /// - 2 テーブルを 1 tx で削除（片方失敗 → 両方ロールバック）
    /// - 返値: 削除した `dir_entries` の行数
    pub(crate) fn delete_mount_entries(&self, mount_id: &str) -> Result<usize, DirIndexError> {
        // invariant 検証のためだけに mount_scope_range を呼ぶ（hi も再利用）
        let (_, hi) = mount_scope_range(mount_id)?;
        let lo = mount_id.to_owned();
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        let removed_entries = tx.execute(
            "DELETE FROM dir_entries WHERE parent_path >= ?1 AND parent_path < ?2",
            params![lo, hi],
        )?;
        tx.execute(
            "DELETE FROM dir_meta WHERE path >= ?1 AND path < ?2",
            params![lo, hi],
        )?;
        tx.commit()?;
        Ok(removed_entries)
    }

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
