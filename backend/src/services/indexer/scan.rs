//! `Indexer` のスキャン関連ロジック
//!
//! - `scan_directory`: フルスキャンで全エントリを登録
//! - `incremental_scan`: `mtime` 差分で追加/更新/削除を反映
//! - `rebuild`: 自マウント配下をスキャンし直してから stale 行を削除 (scan → delete)

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::Ordering;

use crate::services::extensions::classify_for_index;
use crate::services::parallel_walk::{self, WalkEntry, WalkReport};
use crate::services::path_security::PathSecurity;

use super::helpers::{
    IncrementalScanContext, batch_insert, delete_unseen, load_dir_mtimes, load_existing_entries,
    make_relative_prefix, process_walk_entry_incremental, prune_unchanged_dir, should_skip_delete,
};
use super::{BATCH_SIZE, IndexEntry, Indexer, IndexerError, WalkCallbackArgs};

impl Indexer {
    /// ディレクトリをフルスキャンしてインデックスを構築する
    ///
    /// - `parallel_walk` で並列走査し、インデクサブルなファイル/ディレクトリを登録
    /// - `on_walk_entry` コールバックで `DirIndex` 連携 (Phase 6b 用)
    /// - 完了後 `is_ready=true`, `is_stale=false` に設定
    /// - 戻り値: `(登録件数, WalkReport)`。呼び出し側 (`rebuild` 等) は
    ///   `WalkReport.error_count` を参照して「走査エラー時は削除を抑止」の
    ///   判断に使える
    #[allow(
        clippy::too_many_arguments,
        reason = "scan に必要な依存 + cancelled を個別に受ける設計、閾値超過を許容"
    )]
    pub(crate) fn scan_directory(
        &self,
        root_dir: &Path,
        path_security: &PathSecurity,
        mount_id: &str,
        workers: usize,
        on_walk_entry: Option<&mut dyn FnMut(WalkCallbackArgs)>,
        cancelled: &(dyn Fn() -> bool + Sync),
    ) -> Result<(usize, WalkReport), IndexerError> {
        let conn = self.connect()?;

        let mut batch: Vec<IndexEntry> = Vec::with_capacity(BATCH_SIZE);
        let mut total: usize = 0;

        // path_validator: PathSecurity で検証
        let validator = |path: &Path| -> bool { path_security.validate(path).is_ok() };

        // on_walk_entry を外部から受け取る
        let mut on_walk_entry = on_walk_entry;

        let walk_report: WalkReport = parallel_walk::parallel_walk(
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
                        is_complete: entry.is_complete,
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
            cancelled,
        );

        // 協調キャンセル検知時は readiness を更新せず Cancelled を返す。
        // parallel_walk が partial WalkReport を返していても、readiness/stale の
        // 状態は現状維持（次回起動で再試行）
        if walk_report.cancelled {
            return Err(IndexerError::Cancelled);
        }

        // 残りをフラッシュ（cancel されていなければ通常フロー）
        if !batch.is_empty() {
            total += batch.len();
            batch_insert(&conn, &batch)?;
        }

        if walk_report.error_count() > 0 {
            tracing::warn!(
                "scan_directory: 走査中に {} 件のエラーを検出 (mount_id={mount_id}, entries={})",
                walk_report.error_count(),
                walk_report.entry_count
            );
        }

        self.is_ready.store(true, Ordering::Relaxed);
        self.is_stale.store(false, Ordering::Relaxed);

        Ok((total, walk_report))
    }

    /// 差分スキャンで変更のあったエントリのみ更新する
    ///
    /// - 既存エントリの `mtime_ns` を `HashMap` に読み込み
    /// - `parallel_walk` で `dir_filter` コールバックを使い mtime 変更のないディレクトリを枝刈り
    /// - 追加/更新/削除の件数を返す
    #[allow(
        clippy::too_many_arguments,
        reason = "scan に必要な依存 + cancelled を個別に受ける設計、閾値超過を許容"
    )]
    pub(crate) fn incremental_scan(
        &self,
        root_dir: &Path,
        path_security: &PathSecurity,
        mount_id: &str,
        workers: usize,
        on_walk_entry: Option<&mut dyn FnMut(WalkCallbackArgs)>,
        cancelled: &(dyn Fn() -> bool + Sync),
    ) -> Result<(usize, usize, usize), IndexerError> {
        let conn = self.connect()?;
        let existing = load_existing_entries(&conn, mount_id)?;
        let dir_mtimes = load_dir_mtimes(&conn, mount_id)?;

        // 子ディレクトリを持つディレクトリを事前計算 (リーフのみ枝刈りするため)
        let has_subdirs: HashSet<String> = dir_mtimes
            .keys()
            .filter_map(|k| k.rfind('/').map(|i| k[..i].to_string()))
            .collect();

        // dir_filter と entry_callback の両方から借用するため RefCell
        let seen = RefCell::new(HashSet::new());
        let upsert_errors = RefCell::new(0_usize);
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
            upsert_errors: &upsert_errors,
        };

        let validator = |path: &Path| -> bool { path_security.validate(path).is_ok() };
        let mut on_walk_entry = on_walk_entry;

        // dir_filter: mtime 未変更のリーフディレクトリのみ枝刈り
        let mut dir_filter =
            |path: &Path, mtime_ns: i64| -> bool { prune_unchanged_dir(path, mtime_ns, &ctx) };

        let walk_report: WalkReport = parallel_walk::parallel_walk(
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
            cancelled,
        );

        // 協調キャンセル検知時は readiness / delete_unseen を skip
        if walk_report.cancelled {
            return Err(IndexerError::Cancelled);
        }

        let seen = seen.into_inner();
        let upsert_err_count = *upsert_errors.borrow();
        let walk_err_count = walk_report.error_count();

        // ハイブリッド閾値 (絶対 100 件 or 率 1%) に基づき削除スキップを判定。
        // 小規模 (observed < 100) は従来どおり 1 件でもスキップして既存挙動と互換。
        // 大規模では単発の EACCES 等で stale 行が溜まるのを防ぐため削除を許容。
        let deleted = if should_skip_delete(
            walk_err_count,
            upsert_err_count,
            walk_report.observed_entries,
        ) {
            tracing::warn!(
                walk_errors = walk_err_count,
                upsert_errors = upsert_err_count,
                observed = walk_report.observed_entries,
                entries = walk_report.entry_count,
                kind_counts = ?walk_report.error_kind_counts,
                "incremental_scan: エラー閾値超過、delete_unseen をスキップ (mount_id={mount_id})"
            );
            0
        } else {
            delete_unseen(&conn, &seen, mount_id)?
        };

        self.is_ready.store(true, Ordering::Relaxed);
        self.is_stale.store(false, Ordering::Relaxed);

        Ok((added, updated, deleted))
    }

    /// 指定マウント配下のエントリを再構築する
    ///
    /// 方針（走査エラー時のデータロスを避けるため `scan → delete` の順）:
    /// 1. `scan_directory` (INSERT OR REPLACE) で新しい行を登録しつつ、コールバック
    ///    で走査で見えた相対パス集合 `seen` を収集
    /// 2. 走査が成功 (`WalkReport.error_count == 0`) した場合のみ、seen に無い
    ///    自マウント配下の行 (= stale 行) を `delete_unseen` で一括削除
    /// 3. 走査エラー時は削除をスキップし、既存行を保護（次回 rebuild で再試行）
    ///
    /// `DELETE` は **当該 `mount_id` のプレフィックス配下に限定**されるため、
    /// 複数マウントがある場合に他マウントの行を巻き込まない。
    pub(crate) fn rebuild(
        &self,
        root_dir: &Path,
        path_security: &PathSecurity,
        mount_id: &str,
        cancelled: &(dyn Fn() -> bool + Sync),
    ) -> Result<usize, IndexerError> {
        if mount_id.is_empty() {
            return Err(IndexerError::Other(
                "空の mount_id はサポートされていません".to_string(),
            ));
        }

        // scan_directory のコールバックで seen（走査で見えた相対パス）を収集
        let seen: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
        let mut on_walk_entry = |args: WalkCallbackArgs| {
            // walk_entry_path を root_dir からの相対パスへ変換
            let dir_relative = Path::new(&args.walk_entry_path)
                .strip_prefix(Path::new(&args.root_dir))
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let prefix = make_relative_prefix(&args.mount_id, &dir_relative);
            let mut s = seen.borrow_mut();
            for (name, _) in &args.subdirs {
                if classify_for_index(name, true).is_some() {
                    s.insert(format!("{prefix}{name}"));
                }
            }
            for (name, _, _) in &args.files {
                if classify_for_index(name, false).is_some() {
                    s.insert(format!("{prefix}{name}"));
                }
            }
        };

        let scan_result = self.scan_directory(
            root_dir,
            path_security,
            mount_id,
            8,
            Some(&mut on_walk_entry),
            cancelled,
        );

        let (count, walk_report) = scan_result?;

        // ハイブリッド閾値判定で stale 行 (seen に含まれない自マウント行) の削除を判断
        // rebuild は UPSERT エラーを個別集計しないため upsert_errors=0 固定
        if should_skip_delete(walk_report.error_count(), 0, walk_report.observed_entries) {
            tracing::warn!(
                walk_errors = walk_report.error_count(),
                observed = walk_report.observed_entries,
                entries = walk_report.entry_count,
                kind_counts = ?walk_report.error_kind_counts,
                "rebuild: エラー閾値超過、stale 行の削除をスキップ (mount_id={mount_id})"
            );
        } else {
            let conn = self.connect()?;
            let seen_set = seen.into_inner();
            let _deleted = delete_unseen(&conn, &seen_set, mount_id)?;
        }

        Ok(count)
    }
}
