//! ウォームスタート判定 + バックグラウンドインデックススキャン + `FileWatcher` 起動
//!
//! - ウォームスタート条件: `Indexer` にエントリあり + `DirIndex` フルスキャン完了 + マウントフィンガープリント一致
//! - マウントごとに `spawn_blocking` タスクを逐次実行 (`SQLite` 並行書き込み競合を回避)
//! - マウント構成が変わった (fingerprint 不一致) 場合、旧 `mount_id` 配下の
//!   stale 行を非同期 `spawn_blocking` 内で削除してから per-mount scan を実行
//! - 全マウント完了後、`DirIndex` のフラグ設定 + `FileWatcher` 起動
//! - `scan_handle` panic 時は `FileWatcher` 起動を中止し `scan_complete=false`
//!   のまま維持（partial init 防止）

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::services;
use crate::services::dir_index::DirIndex;
use crate::services::indexer::{Indexer, WalkCallbackArgs};
use crate::services::mount_cleanup::{enumerate_stale_mount_ids, perform_full_stale_cleanup};
use crate::services::path_security::PathSecurity;
use crate::services::scan_diagnostics::{
    FingerprintAction, MountDiagnostic, ScanDiagnostics, WalkMetrics, decide_fingerprint_action,
};

use super::BackgroundContext;

/// 1 マウント分のスキャン結果
///
/// - `scan_ok`: `incremental_scan` / `scan_directory` が `Ok` を返したか
/// - `dir_index_ok`: `DirIndex` への書き込み経路 (`begin_bulk` + `ingest` + `flush`) が
///   すべて成功したか
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct MountScanOutcome {
    pub(super) scan_ok: bool,
    pub(super) dir_index_ok: bool,
}

impl MountScanOutcome {
    pub(super) fn is_ok(self) -> bool {
        self.scan_ok && self.dir_index_ok
    }
}

/// ウォームスタート判定 + バックグラウンドスキャンを起動する
#[allow(
    clippy::too_many_lines,
    clippy::needless_pass_by_value,
    reason = "スキャン起動の分岐ロジックは一箇所にまとめる、Arc フィールドを spawn に移動するため所有権が必要"
)]
pub(crate) fn spawn_background_tasks(bg: BackgroundContext) -> tokio::task::JoinHandle<()> {
    let mount_ids: Vec<String> = bg.mount_id_map.keys().cloned().collect();
    let mount_id_refs: Vec<&str> = mount_ids.iter().map(String::as_str).collect();

    let is_warm_start = bg.indexer.entry_count().unwrap_or(0) > 0
        && bg.dir_index.is_full_scan_done().unwrap_or(false)
        && bg
            .indexer
            .check_mount_fingerprint(&mount_id_refs)
            .unwrap_or(false);

    if is_warm_start {
        tracing::info!("Warm Start: 既存インデックスを使用");
        bg.indexer.mark_warm_start();
        bg.dir_index.mark_warm_start();
    } else {
        tracing::info!("フルスキャンを開始");
    }

    // fingerprint 不一致 = マウント構成が変わった可能性。旧 fingerprint から
    // 旧 mount_id セットを復元し、現 mount_id に含まれない id (= stale) を列挙。
    // 実際の DELETE は非同期 spawn_blocking 内で実行する (runtime thread を塞がない)。
    let stale_mount_ids: Vec<String> = if is_warm_start {
        Vec::new()
    } else {
        enumerate_stale_mount_ids(&bg.indexer, &mount_id_refs)
    };
    drop(mount_id_refs);
    if !stale_mount_ids.is_empty() {
        tracing::info!(
            stale_count = stale_mount_ids.len(),
            "stale mount 候補を検出（非同期フェーズで削除）"
        );
    }

    // マウントごとの逐次バックグラウンドスキャン (SQLite 並行書き込み競合を回避)
    let scan_mounts: Vec<(String, PathBuf)> = bg
        .mount_id_map
        .iter()
        .map(|(id, p)| (id.clone(), p.clone()))
        .collect();
    let mount_count = scan_mounts.len().max(1);
    let workers_per_mount = (bg.scan_workers / mount_count).max(2);

    let scan_dir_index = Arc::clone(&bg.dir_index);
    let fingerprint_indexer = Arc::clone(&bg.indexer);
    let fingerprint_mount_ids = mount_ids.clone();

    // FileWatcher 用に先に clone (bg は scan_handle の async move で消費されるため)
    let watcher_indexer = Arc::clone(&bg.indexer);
    let watcher_path_security = Arc::clone(&bg.path_security);
    let watcher_dir_index = Arc::clone(&bg.dir_index);
    let watcher_rebuild_guard = Arc::clone(&bg.rebuild_guard);
    let watcher_slot = Arc::clone(&bg.file_watcher);
    // shutdown_token を scan_handle 内と install task 内でそれぞれ使うため clone
    let shutdown_token_for_scan = bg.shutdown_token.clone();
    let shutdown_token_for_install = bg.shutdown_token.clone();
    let watcher_mounts: Vec<(String, PathBuf)> = bg
        .mount_id_map
        .iter()
        .map(|(id, p)| (id.clone(), p.clone()))
        .collect();

    // マウントごとのスキャンを逐次実行
    // Indexer と DirIndex は同一 SQLite DB を共有するため、並行書き込みで
    // SQLITE_BUSY によるトランザクション失敗を回避する
    //
    // scan_handle は (is_warm_start, all_ok) を返し、外側の tokio::spawn で
    // FileWatcher 起動判定に使う（cold partial は FileWatcher 起動中止）
    let scan_handle: tokio::task::JoinHandle<(bool, bool)> = tokio::spawn(async move {
        // Step 1: stale mount rows 削除（あれば、spawn_blocking 内で同期実行）
        //         Indexer + DirIndex 両方に対して cleanup を実行
        let cleanup_ok = if stale_mount_ids.is_empty() {
            true
        } else {
            let cleanup_indexer = Arc::clone(&bg.indexer);
            let cleanup_dir_index = Arc::clone(&bg.dir_index);
            let stale_for_task = stale_mount_ids.clone();
            let cleanup_token = shutdown_token_for_scan.clone();
            let cleanup_result = tokio::task::spawn_blocking(move || {
                let cancelled_fn = || cleanup_token.is_cancelled();
                perform_full_stale_cleanup(
                    &stale_for_task,
                    &cleanup_indexer,
                    &cleanup_dir_index,
                    &cancelled_fn,
                )
            })
            .await;
            match cleanup_result {
                Ok(ok) => ok,
                Err(e) => {
                    tracing::error!("stale cleanup タスクがパニック: {e}");
                    false
                }
            }
        };

        // Step 2: per-mount scan ループ
        //   (mount_id, Option<MountScanOutcome>, Option<WalkMetrics>) を収集。
        //   panic 時は outcome=None、warm start 時は walk=None（incremental_scan は
        //   WalkReport を返さないため API 契約上常に null）
        let mut mount_results: Vec<(String, Option<MountScanOutcome>, Option<WalkMetrics>)> =
            Vec::with_capacity(scan_mounts.len());
        for (mount_id, root) in scan_mounts {
            let indexer = Arc::clone(&bg.indexer);
            let dir_index = Arc::clone(&bg.dir_index);
            let path_security = Arc::clone(&bg.path_security);
            let mount_id_owned = mount_id.clone();

            let result = tokio::task::spawn_blocking(move || {
                run_mount_scan(
                    is_warm_start,
                    &mount_id_owned,
                    &root,
                    &indexer,
                    &dir_index,
                    &path_security,
                    workers_per_mount,
                )
            })
            .await;

            let (outcome_opt, walk) = match result {
                Ok((outcome, walk)) => (Some(outcome), walk),
                Err(e) => {
                    tracing::error!("バックグラウンドスキャンタスクがパニック ({mount_id}): {e}");
                    (None, None)
                }
            };
            mount_results.push((mount_id, outcome_opt, walk));
        }

        // Step 3: readiness フラグの gate
        //   partial 時は mark_full_scan_done / mark_ready / scan_complete を
        //   すべてスキップし、/api/ready を 503 (cold) / 既存 warm 応答に留める
        let mount_outcomes: Vec<Option<MountScanOutcome>> =
            mount_results.iter().map(|(_, o, _)| *o).collect();
        let (cleanup_ok, scans_ok, all_ok) = aggregate_scan_readiness(cleanup_ok, &mount_outcomes);

        if !all_ok {
            tracing::warn!(
                cleanup_ok,
                scans_ok,
                is_warm_start,
                "cleanup / per-mount scan の部分失敗のため、ready 到達を保留 \
                 (/api/ready: warm=200 既存データで応答 / cold=503 維持)"
            );
        }

        // Step 4: fingerprint 更新 or クリア + 最終アクションを捕捉
        //   - 全成功: 現構成で fingerprint を保存 → Saved or SaveFailed
        //   - warm partial: fingerprint をクリアして次回 cold start を強制 → Cleared or ClearFailed
        //   - cold partial: fingerprint 更新を保留 → NotNeeded
        let (save_ok, clear_ok) = if all_ok {
            let refs: Vec<&str> = fingerprint_mount_ids.iter().map(String::as_str).collect();
            let ok = match fingerprint_indexer.save_mount_fingerprint(&refs) {
                Ok(()) => true,
                Err(e) => {
                    tracing::error!("マウントフィンガープリント保存失敗: {e}");
                    false
                }
            };
            (ok, true)
        } else if is_warm_start {
            // warm partial: 次回起動を cold start に落として fresh full scan で復旧させる
            let ok = match fingerprint_indexer.clear_mount_fingerprint() {
                Ok(()) => {
                    tracing::warn!(
                        cleanup_ok,
                        scans_ok,
                        "warm partial failure: fingerprint クリア、次回起動を cold start に落として復旧"
                    );
                    true
                }
                Err(e) => {
                    tracing::error!("warm partial 時の fingerprint クリア失敗: {e}");
                    false
                }
            };
            (true, ok)
        } else {
            tracing::warn!(
                cleanup_ok,
                scans_ok,
                "cold partial failure: fingerprint 更新を保留、再起動で再試行"
            );
            (true, true)
        };

        // Step 5: ScanDiagnostics を組み立て、readiness promote + last_scan_report 書き込みを
        //   共通ヘルパ `finalize_scan_success` に委譲（rebuild 経路と同じ手順）
        let fingerprint_action =
            decide_fingerprint_action(all_ok, is_warm_start, save_ok, clear_ok);
        let diagnostics = build_scan_diagnostics(
            is_warm_start,
            cleanup_ok,
            scans_ok,
            all_ok,
            fingerprint_action,
            mount_results,
        );
        services::scan_diagnostics::finalize_scan_success(
            &scan_dir_index,
            &bg.scan_complete,
            &bg.last_scan_report,
            diagnostics,
        );

        (is_warm_start, all_ok)
    });

    // スキャン完了後に FileWatcher を起動する外側 coordinator task
    //   - 全成功 or warm partial: 起動（warm partial は fingerprint クリア済で
    //     次回 cold start で復旧するが、稼働中は既存 entries で応答 + 増分追従）
    //   - cold partial: 起動中止（再起動での復旧に一本化、DB 状態を静的に保つ）
    //   - panic: 起動中止（partial init 防止）
    //
    // shutdown_token が cancel されているときは install を skip（late-install 抑止、
    // graceful shutdown 中に FileWatcher が起動しないよう保証）
    //
    // **戻り値**: 本関数は外側の coordinator task の JoinHandle<()> を返す。
    // drain_long_tasks はこれを `abort + timeout 付き await` して scan + install
    // の全ライフサイクル終了を保証する
    tokio::spawn(async move {
        match scan_handle.await {
            Ok((is_warm_start, all_ok)) => {
                let should_start_watcher = all_ok || is_warm_start;
                if !should_start_watcher {
                    tracing::warn!(
                        all_ok,
                        is_warm_start,
                        "cold start partial failure: FileWatcher 起動を中止、再起動で復旧"
                    );
                    return;
                }
                if shutdown_token_for_install.is_cancelled() {
                    tracing::info!("shutdown 中のため FileWatcher を install しない");
                    return;
                }
                tracing::info!(all_ok, is_warm_start, "FileWatcher を起動");

                let file_watcher = services::file_watcher::FileWatcher::new(
                    watcher_indexer,
                    watcher_path_security,
                    watcher_dir_index,
                    watcher_mounts,
                    watcher_rebuild_guard,
                );
                if let Err(e) = file_watcher.start() {
                    tracing::error!("FileWatcher 起動失敗: {e}");
                }
                // FileWatcher を AppState の slot に保存（旧実装の std::mem::forget
                // 相当。hot reload からは take() → stop() → replace() で差し替え
                // 可能。通常動作では drop されずアプリ終了 / shutdown まで存続する）
                //
                // slot poison 時は watcher を保存せずに drop に任せる。
                let mut slot = watcher_slot
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                *slot = Some(file_watcher);
            }
            Err(e) => {
                // partial init 防止: FileWatcher を起動せず、scan_complete も false のまま
                // /api/ready 応答: warm=200 (mark_warm_start 済) / cold=503
                tracing::error!(
                    panicked = true,
                    error = %e,
                    "scan_handle panic、FileWatcher 起動を中止 \
                     (warm なら既存 entries 継続利用 / cold なら /api/ready=503)"
                );
            }
        }
    })
}

/// 1 マウント分のスキャンを実行する。**本関数は `spawn_blocking` 内で呼ぶ**。
///
/// - `DirIndex` の `begin_bulk` → `ingest_walk_entry` (walk callback) → `flush` の
///   各失敗を個別に補足し、`dir_index_ok` フラグへ反映する
///   （従来の `let _ = bulk.ingest_walk_entry(..)` の握りつぶしを廃止、
///   失敗時は warn ログを追加で出力する）
/// - `scan_ok` は `incremental_scan` / `scan_directory` の `Result` から
/// - callback は内側スコープに閉じ込めて `bulk_opt` の可変借用を解放してから
///   `flush` で move out する（borrow-check 対応）
/// - 返値: `(MountScanOutcome { scan_ok, dir_index_ok }, Option<WalkMetrics>)`
///   - `WalkMetrics` は cold start (`scan_directory`) の `WalkReport` から変換して `Some`
///   - warm start (`incremental_scan`) は `WalkReport` を返さないため常に `None`
#[allow(
    clippy::too_many_arguments,
    reason = "scan 組み立てに必要な Arc/参照をまとめるため、現状の閾値超過を許容"
)]
fn run_mount_scan(
    is_warm_start: bool,
    mount_id: &str,
    root: &Path,
    indexer: &Indexer,
    dir_index: &DirIndex,
    path_security: &PathSecurity,
    workers_per_mount: usize,
) -> (MountScanOutcome, Option<WalkMetrics>) {
    let (mut bulk_opt, begin_bulk_ok) = match dir_index.begin_bulk() {
        Ok(b) => (Some(b), true),
        Err(e) => {
            tracing::warn!("DirIndex begin_bulk 失敗 ({mount_id}): {e}");
            (None, false)
        }
    };

    let mut ingest_ok = true;
    let mut walk_metrics: Option<WalkMetrics> = None;
    // Ok 値はスキャン種別で異なる:
    //   incremental_scan: Result<(usize, usize, usize), IndexerError>  (add/upd/del)
    //   scan_directory:   Result<(usize, WalkReport), IndexerError>
    // cold start のみ WalkReport を walk_metrics に保存。warm start は None 維持。
    let scan_result: Result<(), crate::services::indexer::IndexerError> = {
        let mut callback = |args: WalkCallbackArgs| {
            if let Some(bulk) = bulk_opt.as_mut() {
                if let Err(e) = bulk.ingest_walk_entry(&args) {
                    tracing::warn!("DirIndex ingest 失敗 ({mount_id}): {e}");
                    ingest_ok = false;
                }
            }
        };
        if is_warm_start {
            indexer
                .incremental_scan(
                    root,
                    path_security,
                    mount_id,
                    workers_per_mount,
                    Some(&mut callback),
                    &|| false,
                )
                .map(|_| ())
        } else {
            indexer
                .scan_directory(
                    root,
                    path_security,
                    mount_id,
                    workers_per_mount,
                    Some(&mut callback),
                    &|| false,
                )
                .map(|(_, report)| {
                    walk_metrics = Some(WalkMetrics::from(&report));
                })
        }
    }; // ← ここで callback が drop し、bulk_opt と ingest_ok の借用解放
    let scan_ok = scan_result.is_ok();
    if let Err(e) = &scan_result {
        let label = if is_warm_start {
            "Incremental scan"
        } else {
            "Full scan"
        };
        tracing::error!("{label} 失敗 ({mount_id}): {e}");
    }

    let flush_ok = if let Some(mut bulk) = bulk_opt {
        match bulk.flush() {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("DirIndex flush 失敗 ({mount_id}): {e}");
                false
            }
        }
    } else {
        // begin_bulk 失敗時は flush が実行されないので false 扱い
        false
    };

    tracing::info!("スキャン完了: {mount_id}");
    (
        MountScanOutcome {
            scan_ok,
            dir_index_ok: begin_bulk_ok && ingest_ok && flush_ok,
        },
        walk_metrics,
    )
}

/// per-mount 結果から `ScanDiagnostics` を組み立てる（純粋関数、unit test seam）
///
/// - `mount_results` は `(mount_id, Option<MountScanOutcome>, Option<WalkMetrics>)`
/// - `None` の `MountScanOutcome` は `panicked=true` として表現
/// - `completed_at_ms` は現在時刻から `SystemTime` 経由で取得、UNIX epoch 前の
///   場合 (clock skew) のみ 0
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "readiness gate の集計値 3 + warm フラグ = 4 軸は診断出力の意味的に分離されたフィールド"
)]
fn build_scan_diagnostics(
    is_warm_start: bool,
    cleanup_ok: bool,
    scans_ok: bool,
    all_ok: bool,
    fingerprint: FingerprintAction,
    mount_results: Vec<(String, Option<MountScanOutcome>, Option<WalkMetrics>)>,
) -> ScanDiagnostics {
    let completed_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    let mounts = mount_results
        .into_iter()
        .map(|(mount_id, outcome, walk)| MountDiagnostic {
            mount_id,
            scan_ok: outcome.is_some_and(|o| o.scan_ok),
            dir_index_ok: outcome.is_some_and(|o| o.dir_index_ok),
            panicked: outcome.is_none(),
            walk,
        })
        .collect();
    ScanDiagnostics {
        completed_at_ms,
        is_warm_start,
        cleanup_ok,
        scans_ok,
        all_ok,
        fingerprint,
        mounts,
    }
}

/// 集約ロジックの純粋関数版（unit test 用の seam）
///
/// - `cleanup_ok`: Step 1 の stale cleanup 全成功可否
/// - `outcomes`: 各マウントの `MountScanOutcome`（`spawn_blocking` が panic した場合は `None`）
/// - 返値: `(cleanup_ok, scans_ok, all_ok)` タプル
/// - **空配列セマンティクス**: `outcomes.len() == 0` は「スキャン対象 0 件 = no-op success」
///   として `scans_ok == true` を返す
pub(super) fn aggregate_scan_readiness(
    cleanup_ok: bool,
    outcomes: &[Option<MountScanOutcome>],
) -> (bool, bool, bool) {
    let scans_ok = outcomes
        .iter()
        .all(|o| o.is_some_and(MountScanOutcome::is_ok));
    let all_ok = cleanup_ok && scans_ok;
    (cleanup_ok, scans_ok, all_ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_outcome() -> MountScanOutcome {
        MountScanOutcome {
            scan_ok: true,
            dir_index_ok: true,
        }
    }

    #[test]
    fn aggregate_scan_readinessは全成功でtrueを返す() {
        let outcomes = vec![Some(ok_outcome()), Some(ok_outcome())];
        let (cleanup, scans, all) = aggregate_scan_readiness(true, &outcomes);
        assert!(cleanup && scans && all);
    }

    #[test]
    fn aggregate_scan_readinessはcleanup失敗でall_ok_falseになる() {
        let outcomes = vec![Some(ok_outcome())];
        let (cleanup, scans, all) = aggregate_scan_readiness(false, &outcomes);
        assert!(!cleanup);
        assert!(scans);
        assert!(!all);
    }

    #[test]
    fn aggregate_scan_readinessはpanic含有でscans_ok_falseになる() {
        let outcomes = vec![Some(ok_outcome()), None];
        let (cleanup, scans, all) = aggregate_scan_readiness(true, &outcomes);
        assert!(cleanup);
        assert!(!scans);
        assert!(!all);
    }

    #[test]
    fn aggregate_scan_readinessはdir_index_ok_falseでscans_ok_falseになる() {
        let outcomes = vec![Some(MountScanOutcome {
            scan_ok: true,
            dir_index_ok: false,
        })];
        let (_, scans, all) = aggregate_scan_readiness(true, &outcomes);
        assert!(!scans);
        assert!(!all);
    }

    #[test]
    fn aggregate_scan_readinessはmount0件でno_op_successになる() {
        // 空 outcomes → scans_ok=true (vacuous)
        // mount_id_map が空のケースは既存動作通り warning ログ済みなので
        // ここで false にしても情報価値はない
        let (cleanup, scans, all) = aggregate_scan_readiness(true, &[]);
        assert!(cleanup && scans && all);
    }
}
