//! `Indexer` のスキャン関連ロジック
//!
//! - `scan_directory`: フルスキャンで全エントリを登録
//! - `incremental_scan`: `mtime` 差分で追加/更新/削除を反映
//! - `rebuild`: 既存エントリを全削除してからフルスキャン

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::Ordering;

use crate::services::extensions::classify_for_index;
use crate::services::parallel_walk::{self, WalkEntry};
use crate::services::path_security::PathSecurity;

use super::helpers::{
    IncrementalScanContext, batch_insert, delete_unseen, load_dir_mtimes, load_existing_entries,
    make_relative_prefix, process_walk_entry_incremental, prune_unchanged_dir,
};
use super::{BATCH_SIZE, IndexEntry, Indexer, IndexerError, WalkCallbackArgs};

impl Indexer {
    /// ディレクトリをフルスキャンしてインデックスを構築する
    ///
    /// - `parallel_walk` で並列走査し、インデクサブルなファイル/ディレクトリを登録
    /// - `on_walk_entry` コールバックで `DirIndex` 連携 (Phase 6b 用)
    /// - 完了後 `is_ready=true`, `is_stale=false` に設定
    pub(crate) fn scan_directory(
        &self,
        root_dir: &Path,
        path_security: &PathSecurity,
        mount_id: &str,
        workers: usize,
        on_walk_entry: Option<&mut dyn FnMut(WalkCallbackArgs)>,
    ) -> Result<usize, IndexerError> {
        let conn = self.connect()?;

        let mut batch: Vec<IndexEntry> = Vec::with_capacity(BATCH_SIZE);
        let mut total: usize = 0;

        // path_validator: PathSecurity で検証
        let validator = |path: &Path| -> bool { path_security.validate(path).is_ok() };

        // on_walk_entry を外部から受け取る
        let mut on_walk_entry = on_walk_entry;

        parallel_walk::parallel_walk(
            root_dir,
            workers,
            true, // skip_hidden
            Some(&validator),
            &mut |_path, _mtime_ns| true, // dir_filter: フルスキャンでは全ディレクトリを走査
            &mut |entry: WalkEntry| {
                let dir_relative = entry
                    .path
                    .strip_prefix(root_dir)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();

                let prefix = make_relative_prefix(mount_id, &dir_relative);

                // コールバック通知 (DirIndex 連携用)
                if let Some(ref mut cb) = on_walk_entry {
                    cb(WalkCallbackArgs {
                        walk_entry_path: entry.path.to_string_lossy().into_owned(),
                        root_dir: root_dir.to_string_lossy().into_owned(),
                        mount_id: mount_id.to_string(),
                        dir_mtime_ns: entry.mtime_ns,
                        subdirs: entry.subdirs.clone(),
                        files: entry.files.clone(),
                    });
                }

                // サブディレクトリを登録
                for (name, mtime_ns) in &entry.subdirs {
                    if let Some(kind) = classify_for_index(name, true) {
                        let relative_path = format!("{prefix}{name}");
                        batch.push(IndexEntry {
                            relative_path,
                            name: name.clone(),
                            kind: kind.to_string(),
                            size_bytes: None,
                            mtime_ns: *mtime_ns,
                        });
                    }
                }

                // ファイルを登録
                for (name, size_bytes, mtime_ns) in &entry.files {
                    if let Some(kind) = classify_for_index(name, false) {
                        let relative_path = format!("{prefix}{name}");
                        batch.push(IndexEntry {
                            relative_path,
                            name: name.clone(),
                            kind: kind.to_string(),
                            size_bytes: Some(*size_bytes),
                            mtime_ns: *mtime_ns,
                        });
                    }
                }

                // バッチサイズに達したらフラッシュ
                if batch.len() >= BATCH_SIZE {
                    total += batch.len();
                    // エラーはログ出力のみ (走査を止めない)
                    if let Err(e) = batch_insert(&conn, &batch) {
                        tracing::error!("バッチ INSERT 失敗: {e}");
                    }
                    batch.clear();
                }
            },
        );

        // 残りをフラッシュ
        if !batch.is_empty() {
            total += batch.len();
            batch_insert(&conn, &batch)?;
        }

        self.is_ready.store(true, Ordering::Relaxed);
        self.is_stale.store(false, Ordering::Relaxed);

        Ok(total)
    }

    /// 差分スキャンで変更のあったエントリのみ更新する
    ///
    /// - 既存エントリの `mtime_ns` を `HashMap` に読み込み
    /// - `parallel_walk` で `dir_filter` コールバックを使い mtime 変更のないディレクトリを枝刈り
    /// - 追加/更新/削除の件数を返す
    pub(crate) fn incremental_scan(
        &self,
        root_dir: &Path,
        path_security: &PathSecurity,
        mount_id: &str,
        workers: usize,
        on_walk_entry: Option<&mut dyn FnMut(WalkCallbackArgs)>,
    ) -> Result<(usize, usize, usize), IndexerError> {
        let conn = self.connect()?;
        let existing = load_existing_entries(&conn)?;
        let dir_mtimes = load_dir_mtimes(&conn)?;

        // 子ディレクトリを持つディレクトリを事前計算 (リーフのみ枝刈りするため)
        let has_subdirs: HashSet<String> = dir_mtimes
            .keys()
            .filter_map(|k| k.rfind('/').map(|i| k[..i].to_string()))
            .collect();

        // dir_filter と entry_callback の両方から借用するため RefCell
        let seen = RefCell::new(HashSet::new());
        let mut added: usize = 0;
        let mut updated: usize = 0;

        let ctx = IncrementalScanContext {
            root_dir,
            mount_id,
            conn: &conn,
            existing: &existing,
            dir_mtimes: &dir_mtimes,
            seen: &seen,
            has_subdirs: &has_subdirs,
        };

        let validator = |path: &Path| -> bool { path_security.validate(path).is_ok() };
        let mut on_walk_entry = on_walk_entry;

        // dir_filter: mtime 未変更のリーフディレクトリのみ枝刈り
        let mut dir_filter =
            |path: &Path, mtime_ns: i64| -> bool { prune_unchanged_dir(path, mtime_ns, &ctx) };

        parallel_walk::parallel_walk(
            root_dir,
            workers,
            true,
            Some(&validator),
            &mut dir_filter,
            &mut |entry: WalkEntry| {
                let (a, u) = process_walk_entry_incremental(&entry, &ctx, &mut on_walk_entry);
                added += a;
                updated += u;
            },
        );

        let seen = seen.into_inner();
        let deleted = delete_unseen(&conn, &seen)?;

        self.is_ready.store(true, Ordering::Relaxed);
        self.is_stale.store(false, Ordering::Relaxed);

        Ok((added, updated, deleted))
    }

    /// インデックスを全削除して再構築する
    pub(crate) fn rebuild(
        &self,
        root_dir: &Path,
        path_security: &PathSecurity,
        mount_id: &str,
    ) -> Result<usize, IndexerError> {
        self.is_rebuilding.store(true, Ordering::Relaxed);

        // 全エントリを削除
        let conn = self.connect()?;
        conn.execute("DELETE FROM entries", [])?;
        drop(conn);

        let result = self.scan_directory(root_dir, path_security, mount_id, 8, None);

        self.is_rebuilding.store(false, Ordering::Relaxed);

        result
    }
}
