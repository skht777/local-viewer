//! サムネイルプリウォームサービス
//!
//! browse レスポンス後に fire-and-forget でサムネイルを事前生成する。
//! - `Semaphore(4)` で同時実行数を制限
//! - `pending` セットで処理中の重複排除

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

use crate::services::extensions::{EntryKind, IMAGE_EXTENSIONS, PDF_EXTENSIONS, VIDEO_EXTENSIONS};
use crate::services::models::EntryMeta;
use crate::state::AppState;

/// サムネイルプリウォームサービス
pub(crate) struct ThumbnailWarmer {
    semaphore: Arc<tokio::sync::Semaphore>,
    pending: Arc<Mutex<HashSet<String>>>,
}

impl ThumbnailWarmer {
    pub(crate) fn new(concurrency: usize) -> Self {
        Self {
            semaphore: Arc::new(tokio::sync::Semaphore::new(concurrency)),
            pending: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// browse レスポンスのエントリ群に対してプリウォームをスケジュールする
    ///
    /// fire-and-forget で非同期タスクを起動する。
    /// 依存方向: `ThumbnailWarmer` → `ThumbnailService` / `VideoConverter` / `ArchiveService`
    /// (router 関数は呼ばない)
    pub(crate) fn warm(&self, entries: &[EntryMeta], state: &Arc<AppState>) {
        for entry in entries {
            // サムネイル対象の種別のみ
            if !matches!(
                entry.kind,
                EntryKind::Image | EntryKind::Archive | EntryKind::Pdf | EntryKind::Video
            ) {
                continue;
            }

            // 近似 mtime_ns でキャッシュ確認
            if let Some(modified_at) = entry.modified_at {
                let approx_mtime_ns = (modified_at * 1_000_000_000.0) as u128;
                let cache_key = state
                    .thumbnail_service
                    .make_cache_key(&entry.node_id, approx_mtime_ns);
                if state.thumbnail_service.is_cached(&cache_key) {
                    continue;
                }
            }

            // pending チェック + 追加
            {
                let mut pending = self
                    .pending
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if pending.contains(&entry.node_id) {
                    continue;
                }
                pending.insert(entry.node_id.clone());
            }

            let sem = Arc::clone(&self.semaphore);
            let pending = Arc::clone(&self.pending);
            let state = Arc::clone(state);
            let nid = entry.node_id.clone();

            tokio::spawn(async move {
                // Semaphore で同時実行数制限
                let Ok(_permit) = sem.acquire().await else {
                    pending
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .remove(&nid);
                    return;
                };

                // サムネイル生成 (CPU バウンド → spawn_blocking)
                let _ = tokio::task::spawn_blocking(move || {
                    warm_one_thumbnail(&state, &nid);
                    // 完了後に pending から除去
                    pending
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .remove(&nid);
                })
                .await;
            });
        }
    }
}

/// 1 エントリのサムネイルを生成する (sync、`spawn_blocking` 内)
///
/// エラーは無視する (プリウォームのため、失敗してもリクエスト処理に影響しない)
fn warm_one_thumbnail(state: &AppState, node_id: &str) {
    let thumb_svc = &state.thumbnail_service;
    let video_conv = &state.video_converter;

    // アーカイブエントリか
    let entry = {
        let mut reg = state
            .node_registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reg.resolve_archive_entry(node_id)
    };

    if let Some((archive_path, entry_name)) = entry {
        if let Ok(mtime_ns) = mtime_ns_of(&archive_path) {
            let cache_key = thumb_svc.make_cache_key(node_id, mtime_ns);
            if let Ok(data) = state
                .archive_service
                .extract_entry(&archive_path, &entry_name)
            {
                let _ = thumb_svc.generate_from_bytes(&data, &cache_key);
            }
        }
        return;
    }

    // 通常パス解決
    let resolved = {
        let reg = state
            .node_registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reg.resolve(node_id).ok().map(std::path::Path::to_path_buf)
    };

    let Some(resolved) = resolved else { return };
    if resolved.is_dir() {
        return;
    }

    let Ok(mtime_ns) = mtime_ns_of(&resolved) else {
        return;
    };
    let cache_key = thumb_svc.make_cache_key(node_id, mtime_ns);
    let ext = ext_lower(&resolved.to_string_lossy());

    // アーカイブファイル → 先頭画像
    if state.archive_service.is_supported(&resolved) {
        if let Ok(Some(entry)) = state.archive_service.first_image_entry(&resolved) {
            if let Ok(data) = state.archive_service.extract_entry(&resolved, &entry.name) {
                let _ = thumb_svc.generate_from_bytes(&data, &cache_key);
            }
        }
        return;
    }

    // PDF
    if PDF_EXTENSIONS.contains(&ext.as_str()) {
        let _ = thumb_svc.generate_pdf_thumbnail(
            &resolved,
            &cache_key,
            state.settings.video_thumb_timeout,
        );
        return;
    }

    // 動画
    if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        if let Some(frame) = video_conv.extract_frame(&resolved) {
            let _ = thumb_svc.generate_from_bytes(&frame, &cache_key);
        }
        return;
    }

    // 画像
    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        let _ = thumb_svc.generate_from_path(&resolved, &cache_key);
    }
}

fn mtime_ns_of(path: &std::path::Path) -> Result<u128, std::io::Error> {
    let meta = std::fs::metadata(path)?;
    Ok(meta
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos())
}

fn ext_lower(name: &str) -> String {
    std::path::Path::new(name)
        .extension()
        .map_or(String::new(), |e| {
            format!(".{}", e.to_string_lossy().to_lowercase())
        })
}
