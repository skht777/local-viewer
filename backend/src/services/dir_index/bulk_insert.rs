//! 高速バッチ挿入（per-parent cascade canonicalize 経由）
//!
//! `ingest_walk_entry` で受け取る `WalkCallbackArgs` は parent 単位の完全な子集合を
//! 含む。`BulkInserter` はこれを `pending_by_parent` に格納し、`flush()` で 1 tx 内に
//! 各 parent を `canonicalize_parent_in_tx` で正本化する。
//!
//! 設計ポリシー:
//! - **parent 境界 flush のみ**: 中途半端な flush で部分集合を canonicalize しないため、
//!   `BATCH_SIZE` 到達判定は新 parent 追加直前に行う
//! - **累積保持しない**: flush 後 `pending_by_parent.clear()` (前回 parent の DELETE
//!   重複を回避)
//! - **巨大 parent 即 flush**: 単一 parent が `BATCH_SIZE` を超える場合は追加直後に
//!   flush。1 parent atomic は譲らない (canonicalize は parent 単位でしか分割できない)
//! - **`is_complete=false` skip**: `WalkEntry` が部分結果なら `cascade` を skip して既存行
//!   を保持
//! - **`CorruptPersistentName` Err 伝播のみ**: リカバリは call site で実行
//!   (`BulkInserter` 自身は `Indexer` を知らない)

use std::collections::BTreeMap;

use rusqlite::Connection;

use crate::services::indexer::WalkCallbackArgs;

use super::sort_queries::build_parent_path;
use super::writes::canonicalize_parent_in_tx;
use super::{BATCH_SIZE, DirIndexError};

/// 1 parent 分の蓄積データ
#[derive(Debug)]
pub(super) struct BatchedParent {
    pub(super) mount_id: String,
    pub(super) dir_mtime_ns: i64,
    pub(super) subdirs: Vec<(String, i64)>,
    pub(super) files: Vec<(String, i64, i64)>,
}

impl BatchedParent {
    fn entry_count(&self) -> usize {
        self.subdirs.len() + self.files.len()
    }
}

pub(crate) struct BulkInserter {
    pub(super) conn: Connection,
    /// `parent_path` → 蓄積データ (parent 単位完結)
    pending_by_parent: BTreeMap<String, BatchedParent>,
    /// 累積エントリ数 (`BATCH_SIZE` 判定用、`subdirs + files` 合計)
    pending_entry_count: usize,
}

impl BulkInserter {
    /// `Connection` から `BulkInserter` を生成 (内部利用、`DirIndex::begin_bulk` 経由)
    pub(super) fn new(conn: Connection) -> Self {
        Self {
            conn,
            pending_by_parent: BTreeMap::new(),
            pending_entry_count: 0,
        }
    }

    /// `WalkCallbackArgs` を受け取りバッチに蓄積する (full snapshot API)
    ///
    /// 同 parent への二重 ingest は最新値で上書きする。Walker は通常同 parent を 1
    /// 度しか callback しないが防御的に。
    ///
    /// メモリ抑制ポリシー:
    /// 1. `args.is_complete == false` なら早期 return + WARN (cascade skip)
    /// 2. 既存 pending と新 parent を同時保持しないため、
    ///    `pending_count + new_count > BATCH_SIZE` なら**先に flush**
    /// 3. 単一 parent が `BATCH_SIZE` 超なら追加直後に**即 flush** (1 parent = 1 tx)
    pub(crate) fn ingest_walk_entry(
        &mut self,
        args: &WalkCallbackArgs,
    ) -> Result<(), DirIndexError> {
        if !args.is_complete {
            tracing::warn!(
                mount_id = %args.mount_id,
                walk_entry_path = %args.walk_entry_path,
                "is_complete=false の WalkEntry を skip (DirIndex 既存行を保持)"
            );
            return Ok(());
        }

        let parent_path = build_parent_path(args);
        let new_count = args.subdirs.len() + args.files.len();

        // メモリ抑制: 既存 pending + 新 parent が BATCH_SIZE 超なら先に flush
        if self.pending_entry_count > 0
            && self.pending_entry_count.saturating_add(new_count) > BATCH_SIZE
        {
            self.flush()?;
        }

        // 同 parent への二重 ingest は上書き
        let prev = self.pending_by_parent.insert(
            parent_path.clone(),
            BatchedParent {
                mount_id: args.mount_id.clone(),
                dir_mtime_ns: args.dir_mtime_ns,
                subdirs: args.subdirs.clone(),
                files: args.files.clone(),
            },
        );
        if let Some(prev) = prev {
            self.pending_entry_count = self
                .pending_entry_count
                .saturating_sub(prev.entry_count())
                .saturating_add(new_count);
        } else {
            self.pending_entry_count = self.pending_entry_count.saturating_add(new_count);
        }

        // 単一巨大 parent: 追加直後に即 flush (1 parent = 1 tx を維持)
        if new_count >= BATCH_SIZE {
            self.flush()?;
        }

        Ok(())
    }

    /// 蓄積中の全 parent を 1 tx 内で canonicalize する
    ///
    /// `CorruptPersistentName` を捕捉した場合は tx を rollback (drop) して **そのまま
    /// `Err` を伝播**する。リカバリは呼び出し側 (`run_mount_scan` / `browse fallback`)
    /// で実行する。
    pub(crate) fn flush(&mut self) -> Result<(), DirIndexError> {
        if self.pending_by_parent.is_empty() {
            return Ok(());
        }

        let tx = self.conn.unchecked_transaction()?;
        for (parent_path, batched) in &self.pending_by_parent {
            canonicalize_parent_in_tx(
                &tx,
                &batched.mount_id,
                parent_path,
                batched.dir_mtime_ns,
                &batched.subdirs,
                &batched.files,
            )?;
        }
        tx.commit()?;

        self.pending_by_parent.clear();
        self.pending_entry_count = 0;

        Ok(())
    }
}

impl Drop for BulkInserter {
    fn drop(&mut self) {
        // 残りのエントリをフラッシュ (エラーはログのみ)
        if !self.pending_by_parent.is_empty()
            && let Err(e) = self.flush()
        {
            tracing::error!("BulkInserter drop 時のフラッシュ失敗: {e}");
        }
    }
}
