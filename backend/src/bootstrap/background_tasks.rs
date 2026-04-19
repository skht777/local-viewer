//! ウォームスタート判定 + バックグラウンドインデックススキャン + `FileWatcher` 起動
//!
//! - ウォームスタート条件: `Indexer` にエントリあり + `DirIndex` フルスキャン完了 + マウントフィンガープリント一致
//! - マウントごとに `spawn_blocking` タスクを逐次実行 (`SQLite` 並行書き込み競合を回避)
//! - マウント構成が変わった (fingerprint 不一致) 場合、旧 `mount_id` 配下の
//!   stale 行を非同期 `spawn_blocking` 内で削除してから per-mount scan を実行
//! - 全マウント完了後、`DirIndex` のフラグ設定 + `FileWatcher` 起動
//! - `scan_handle` panic 時は `FileWatcher` 起動を中止し `scan_complete=false`
//!   のまま維持（partial init 防止）

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::services;
use crate::services::indexer::{Indexer, IndexerError};

use super::BackgroundContext;

/// ウォームスタート判定 + バックグラウンドスキャンを起動する
#[allow(
    clippy::too_many_lines,
    clippy::needless_pass_by_value,
    reason = "スキャン起動の分岐ロジックは一箇所にまとめる、Arc フィールドを spawn に移動するため所有権が必要"
)]
pub(crate) fn spawn_background_tasks(bg: BackgroundContext) {
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
    let watcher_mounts: Vec<(String, PathBuf)> = bg
        .mount_id_map
        .iter()
        .map(|(id, p)| (id.clone(), p.clone()))
        .collect();

    // stale cleanup の全成功可否を非同期フェーズ間で共有
    let cleanup_all_ok = Arc::new(AtomicBool::new(true));

    // マウントごとのスキャンを逐次実行
    // Indexer と DirIndex は同一 SQLite DB を共有するため、並行書き込みで
    // SQLITE_BUSY によるトランザクション失敗を回避する
    let scan_cleanup_ok = Arc::clone(&cleanup_all_ok);
    let scan_handle = tokio::spawn(async move {
        // Step 1: stale mount rows 削除（あれば、spawn_blocking 内で同期実行）
        if !stale_mount_ids.is_empty() {
            let cleanup_indexer = Arc::clone(&bg.indexer);
            let stale_for_task = stale_mount_ids.clone();
            let cleanup_ok_ref = Arc::clone(&scan_cleanup_ok);
            let cleanup_result = tokio::task::spawn_blocking(move || {
                let delete_one = |id: &str| cleanup_indexer.delete_mount_entries(id);
                let all_ok = perform_stale_cleanup(&stale_for_task, delete_one);
                if !all_ok {
                    cleanup_ok_ref.store(false, Ordering::Relaxed);
                }
            })
            .await;
            if let Err(e) = cleanup_result {
                tracing::error!("stale cleanup タスクがパニック: {e}");
                scan_cleanup_ok.store(false, Ordering::Relaxed);
            }
        }

        // Step 2: 既存の per-mount scan ループ
        for (mount_id, root) in scan_mounts {
            let indexer = Arc::clone(&bg.indexer);
            let dir_index = Arc::clone(&bg.dir_index);
            let path_security = Arc::clone(&bg.path_security);

            let result = tokio::task::spawn_blocking(move || {
                if is_warm_start {
                    // 差分スキャン + DirIndex コールバック
                    let mut bulk = match dir_index.begin_bulk() {
                        Ok(b) => Some(b),
                        Err(e) => {
                            tracing::warn!("DirIndex begin_bulk 失敗 ({mount_id}): {e}");
                            None
                        }
                    };
                    let result = indexer.incremental_scan(
                        &root,
                        &path_security,
                        &mount_id,
                        workers_per_mount,
                        Some(&mut |args| {
                            if let Some(bulk) = bulk.as_mut() {
                                let _ = bulk.ingest_walk_entry(&args);
                            }
                        }),
                    );
                    if let Some(mut bulk) = bulk {
                        if let Err(e) = bulk.flush() {
                            tracing::warn!("DirIndex flush 失敗 ({mount_id}): {e}");
                        }
                    }
                    if let Err(e) = &result {
                        tracing::error!("Incremental scan 失敗 ({mount_id}): {e}");
                    }
                } else {
                    // フルスキャン + DirIndex コールバック
                    let mut bulk = match dir_index.begin_bulk() {
                        Ok(b) => Some(b),
                        Err(e) => {
                            tracing::warn!("DirIndex begin_bulk 失敗 ({mount_id}): {e}");
                            None
                        }
                    };
                    let result = indexer.scan_directory(
                        &root,
                        &path_security,
                        &mount_id,
                        workers_per_mount,
                        Some(&mut |args| {
                            if let Some(bulk) = bulk.as_mut() {
                                let _ = bulk.ingest_walk_entry(&args);
                            }
                        }),
                    );
                    if let Some(mut bulk) = bulk {
                        if let Err(e) = bulk.flush() {
                            tracing::warn!("DirIndex flush 失敗 ({mount_id}): {e}");
                        }
                    }
                    if let Err(e) = &result {
                        tracing::error!("Full scan 失敗 ({mount_id}): {e}");
                    }
                }
                tracing::info!("スキャン完了: {mount_id}");
            })
            .await;

            if let Err(e) = result {
                tracing::error!("バックグラウンドスキャンタスクがパニック: {e}");
            }
        }

        // Step 3: 全マウントスキャン完了後にフラグを設定
        if !is_warm_start {
            let _ = scan_dir_index.mark_full_scan_done();
        }
        scan_dir_index.mark_ready();

        // Step 4: 全 stale 削除成功時のみ fingerprint を新構成で上書き
        //         部分失敗時は旧 fingerprint を残して次回起動で再試行可能にする
        if scan_cleanup_ok.load(Ordering::Relaxed) {
            let refs: Vec<&str> = fingerprint_mount_ids.iter().map(String::as_str).collect();
            if let Err(e) = fingerprint_indexer.save_mount_fingerprint(&refs) {
                tracing::error!("マウントフィンガープリント保存失敗: {e}");
            }
        } else {
            tracing::warn!(
                "stale cleanup が部分失敗のため、fingerprint 更新を保留（次回起動で再試行）"
            );
        }

        bg.scan_complete.store(true, Ordering::Relaxed);
    });

    // スキャン完了後に FileWatcher を起動（panic 時は起動中止）
    tokio::spawn(async move {
        match scan_handle.await {
            Ok(()) => {
                tracing::info!("全マウントのスキャン完了、FileWatcher を起動");

                let file_watcher = services::file_watcher::FileWatcher::new(
                    watcher_indexer,
                    watcher_path_security,
                    watcher_dir_index,
                    watcher_mounts,
                );
                if let Err(e) = file_watcher.start() {
                    tracing::error!("FileWatcher 起動失敗: {e}");
                }
                // FileWatcher を維持 (drop で停止するため)
                std::mem::forget(file_watcher);
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
    });
}

/// 旧 fingerprint と現 `mount_id` セットの差分から stale `mount_id` を列挙する
///
/// - 旧 fingerprint 読み取り失敗時は `tracing::warn!` + 空 `Vec`（cleanup を
///   スキップ、次回起動で再試行）
/// - 同期呼び出し用（`spawn_background_tasks` 冒頭の列挙フェーズ）
fn enumerate_stale_mount_ids(indexer: &Indexer, current_ids: &[&str]) -> Vec<String> {
    match indexer.load_stored_mount_ids() {
        Ok(old_ids) => {
            let current_set: HashSet<&str> = current_ids.iter().copied().collect();
            old_ids
                .into_iter()
                .filter(|id| !current_set.contains(id.as_str()))
                .collect()
        }
        Err(e) => {
            tracing::warn!("旧 fingerprint 読み出し失敗 (stale cleanup skip): {e}");
            Vec::new()
        }
    }
}

/// `stale_ids` に対応する entries 行を順次削除し、全成功可否を返す
///
/// - **呼び出し位置**: 必ず `tokio::task::spawn_blocking` の**内部**で呼ぶ
///   （同期 DB I/O のため）
/// - `delete_one`: `Fn(&str) -> Result<usize, IndexerError>` で fault injection
///   可能化（テストでは 2 件目のみ `Err` を返す closure 等を注入）
/// - 各 mount の削除成功件数を `tracing::info!` で記録、失敗は `tracing::error!`
/// - 返値: 全 mount の削除が成功したかどうか
pub(super) fn perform_stale_cleanup<F>(stale_ids: &[String], delete_one: F) -> bool
where
    F: Fn(&str) -> Result<usize, IndexerError>,
{
    let mut all_ok = true;
    for id in stale_ids {
        match delete_one(id) {
            Ok(n) => tracing::info!("stale mount 行削除: mount_id={id}, rows={n}"),
            Err(e) => {
                tracing::error!("stale mount 行削除失敗: mount_id={id}, err={e}");
                all_ok = false;
            }
        }
    }
    all_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::indexer::Indexer;

    /// テスト用の一時 DB パスでインデクサーを生成する
    fn setup_indexer() -> (Indexer, tempfile::NamedTempFile) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let indexer = Indexer::new(tmp.path().to_str().unwrap());
        indexer.init_db().unwrap();
        (indexer, tmp)
    }

    // 16 桁 lowercase hex の mount_id 定数
    const MOUNT_A: &str = "aaaaaaaaaaaaaaaa";
    const MOUNT_B: &str = "bbbbbbbbbbbbbbbb";
    const MOUNT_C: &str = "cccccccccccccccc";

    // --- perform_stale_cleanup の回帰テスト ---

    #[test]
    fn perform_stale_cleanup全成功時はtrueを返す() {
        let calls = std::sync::Mutex::new(Vec::<String>::new());
        let ids = vec![MOUNT_A.to_string(), MOUNT_B.to_string()];
        let ok = perform_stale_cleanup(&ids, |id| {
            calls.lock().unwrap().push(id.to_string());
            Ok(5)
        });
        assert!(ok);
        let calls = calls.into_inner().unwrap();
        assert_eq!(calls, vec![MOUNT_A.to_string(), MOUNT_B.to_string()]);
    }

    #[test]
    fn perform_stale_cleanup部分失敗時はfalseを返し全件試行する() {
        // codex v2 Warning 反映: fault injection で部分失敗をシミュレート
        // (load_stored_mount_ids の all-or-nothing filter とは独立の経路)
        let calls = std::sync::Mutex::new(Vec::<String>::new());
        let ids = vec![
            MOUNT_A.to_string(),
            MOUNT_B.to_string(),
            MOUNT_C.to_string(),
        ];
        let ok = perform_stale_cleanup(&ids, |id| {
            calls.lock().unwrap().push(id.to_string());
            if id == MOUNT_B {
                Err(IndexerError::Other("forced failure".into()))
            } else {
                Ok(3)
            }
        });
        assert!(!ok, "部分失敗なら false を返すべき");
        // 失敗しても残りの mount も試行される
        let calls = calls.into_inner().unwrap();
        assert_eq!(
            calls,
            vec![
                MOUNT_A.to_string(),
                MOUNT_B.to_string(),
                MOUNT_C.to_string()
            ]
        );
    }

    // --- enumerate_stale_mount_ids の回帰テスト ---

    #[test]
    fn enumerate_stale_mount_idsは旧と新の差分を返す() {
        let (indexer, _tmp) = setup_indexer();
        indexer.save_mount_fingerprint(&[MOUNT_A, MOUNT_B]).unwrap();
        // 新構成: mountA + mountC (mountB が stale)
        let stale = enumerate_stale_mount_ids(&indexer, &[MOUNT_A, MOUNT_C]);
        assert_eq!(stale, vec![MOUNT_B.to_string()]);
    }

    #[test]
    fn enumerate_stale_mount_idsはfingerprint未保存で空vecを返す() {
        let (indexer, _tmp) = setup_indexer();
        let stale = enumerate_stale_mount_ids(&indexer, &[MOUNT_A, MOUNT_B]);
        assert!(stale.is_empty());
    }

    #[test]
    fn enumerate_stale_mount_idsは全マウント変更時に旧全件を返す() {
        let (indexer, _tmp) = setup_indexer();
        indexer.save_mount_fingerprint(&[MOUNT_A, MOUNT_B]).unwrap();
        // 新構成: 全く別のマウント集合
        let new_mount = "0123456789abcdef";
        let stale = enumerate_stale_mount_ids(&indexer, &[new_mount]);
        // MOUNT_A, MOUNT_B の両方が stale
        let mut stale_sorted = stale.clone();
        stale_sorted.sort();
        assert_eq!(stale_sorted, vec![MOUNT_A.to_string(), MOUNT_B.to_string()]);
    }
}
