//! 1 秒バッチ flush worker + 1 件イベントの index 反映処理

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use tracing::{debug, warn};

use super::WATCHER_PENDING_SOFT_CAP;
use super::path_utils::compute_relative_path;
use crate::services::dir_index::DirIndex;
use crate::services::extensions::classify_for_index;
use crate::services::indexer::{IndexEntry, Indexer};
use crate::services::path_security::PathSecurity;
use crate::services::rebuild_guard::RebuildGuard;

/// 1 秒間隔で pending イベントを取り出し、`Indexer` + `DirIndex` に反映する
///
/// - `rebuild_guard.is_held()` の間は flush を延期し、pending を維持したまま次 tick へ
/// - ただし pending が `WATCHER_PENDING_SOFT_CAP` を超えた場合は
///   `DirIndex::mark_warm_start()` で整合回復を予約し pending を drain する
///   （rebuild 完了後の warm-start 経路で再スキャンされ、DB 上の乖離は解消）
pub(super) async fn flush_worker_loop(
    pending: Arc<std::sync::Mutex<HashMap<String, String>>>,
    indexer: Arc<Indexer>,
    path_security: Arc<PathSecurity>,
    dir_index: Arc<DirIndex>,
    mounts: Vec<(String, PathBuf)>,
    rebuild_guard: Arc<RebuildGuard>,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;

        // rebuild / mount reload 中は flush を延期。ただし pending 蓄積が soft cap を
        // 超えたら DirIndex を stale 化して drain、メモリ footgun を防ぐ
        if rebuild_guard.is_held() {
            let pending_len = pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len();
            if pending_len > WATCHER_PENDING_SOFT_CAP {
                warn!(
                    pending = pending_len,
                    cap = WATCHER_PENDING_SOFT_CAP,
                    "FileWatcher: rebuild 中の pending が soft cap 超過、DirIndex 整合回復を予約して drain"
                );
                dir_index.mark_warm_start();
                let mut guard = pending
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                guard.clear();
            }
            continue;
        }

        // pending を一括取得
        let events = {
            let mut guard = pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            std::mem::take(&mut *guard)
        };

        if events.is_empty() {
            continue;
        }

        debug!("FileWatcher flush: {} 件のイベントを処理", events.len());

        // spawn_blocking 内で同期処理
        let indexer_c = Arc::clone(&indexer);
        let ps_c = Arc::clone(&path_security);
        let di_c = Arc::clone(&dir_index);
        let mounts_c = mounts.clone();

        let _ = tokio::task::spawn_blocking(move || {
            for (path_str, action) in &events {
                process_event(&indexer_c, &ps_c, &di_c, &mounts_c, path_str, action);
            }
        })
        .await;
    }
}

/// 1 件のイベントを処理してインデックスに反映し、DirIndex の親ディレクトリを dirty 化する
pub(super) fn process_event(
    indexer: &Indexer,
    path_security: &PathSecurity,
    dir_index: &DirIndex,
    mounts: &[(String, PathBuf)],
    path_str: &str,
    action: &str,
) {
    let path = Path::new(path_str);

    // 親ディレクトリの parent_key を計算して dirty 化
    // remove/rename-from では対象パスが既に存在しないため、存在する親ディレクトリから計算
    if let Some(parent) = path.parent() {
        if let Some(parent_key) = compute_relative_path(parent, mounts) {
            dir_index.mark_dir_dirty(&parent_key);
        }
    }

    if action == "remove" {
        // 削除: 相対パスを計算して remove_entry
        if let Some(rel) = compute_relative_path(path, mounts) {
            if let Err(e) = indexer.remove_entry(&rel) {
                debug!("remove_entry 失敗: {e} (path: {path_str})");
            }
        }
        return;
    }

    // action == "add"
    // パスの存在確認
    if !path.exists() {
        return;
    }

    // PathSecurity 検証
    if path_security.validate(path).is_err() {
        return;
    }

    // メタデータ取得
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };

    let Some(file_name) = path.file_name() else {
        return;
    };
    let name = file_name.to_string_lossy().into_owned();
    let is_dir = meta.is_dir();

    // インデックス対象の種別を判定
    let Some(kind) = classify_for_index(&name, is_dir) else {
        return;
    };

    // 相対パスを計算
    let Some(rel) = compute_relative_path(path, mounts) else {
        return;
    };

    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| {
            // ナノ秒を i64 に変換 (u128 → i64)
            i64::try_from(d.as_nanos()).unwrap_or(i64::MAX)
        });

    let size_bytes = if is_dir {
        None
    } else {
        // ファイルサイズが i64::MAX を超えることは現実的には無いが、
        // `as i64` の wrap を避けるために try_from で clamp する
        Some(i64::try_from(meta.len()).unwrap_or(i64::MAX))
    };

    let entry = IndexEntry {
        relative_path: rel,
        name,
        kind: kind.to_string(),
        size_bytes,
        mtime_ns,
    };

    if let Err(e) = indexer.add_entry(&entry) {
        debug!("add_entry 失敗: {e} (path: {path_str})");
    }
}
