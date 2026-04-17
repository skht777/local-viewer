//! ウォームスタート判定 + バックグラウンドインデックススキャン + `FileWatcher` 起動
//!
//! - ウォームスタート条件: `Indexer` にエントリあり + `DirIndex` フルスキャン完了 + マウントフィンガープリント一致
//! - マウントごとに `spawn_blocking` タスクを逐次実行 (`SQLite` 並行書き込み競合を回避)
//! - 全マウント完了後、`DirIndex` のフラグ設定 + `FileWatcher` 起動

use std::path::PathBuf;
use std::sync::Arc;

use crate::services;

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

    // マウントフィンガープリントを保存
    if let Err(e) = bg.indexer.save_mount_fingerprint(&mount_id_refs) {
        tracing::error!("マウントフィンガープリント保存失敗: {e}");
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

    // FileWatcher 用に先に clone (bg は scan_handle の async move で消費されるため)
    let watcher_indexer = Arc::clone(&bg.indexer);
    let watcher_path_security = Arc::clone(&bg.path_security);
    let watcher_dir_index = Arc::clone(&bg.dir_index);
    let watcher_mounts: Vec<(String, PathBuf)> = bg
        .mount_id_map
        .iter()
        .map(|(id, p)| (id.clone(), p.clone()))
        .collect();

    // マウントごとのスキャンを逐次実行
    // Indexer と DirIndex は同一 SQLite DB を共有するため、並行書き込みで
    // SQLITE_BUSY によるトランザクション失敗を回避する
    let scan_handle = tokio::spawn(async move {
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

        // 全マウントスキャン完了後にフラグを設定
        if !is_warm_start {
            let _ = scan_dir_index.mark_full_scan_done();
        }
        scan_dir_index.mark_ready();
        bg.scan_complete
            .store(true, std::sync::atomic::Ordering::Relaxed);
    });

    // スキャン完了後に FileWatcher を起動
    tokio::spawn(async move {
        let _ = scan_handle.await;
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
    });
}
