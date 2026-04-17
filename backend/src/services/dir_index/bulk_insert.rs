//! 高速バッチ挿入

use rusqlite::{Connection, params};

use crate::services::indexer::WalkCallbackArgs;
use crate::services::natural_sort::encode_sort_key;

use super::sort_queries::{build_parent_path, classify_kind};
use super::{BATCH_SIZE, DirIndexError, PendingEntry};

pub(crate) struct BulkInserter {
    pub(super) conn: Connection,
    pub(super) pending_entries: Vec<PendingEntry>,
    pub(super) pending_meta: Vec<(String, i64)>,
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
