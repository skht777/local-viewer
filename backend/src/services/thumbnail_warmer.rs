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
        let mut scheduled = 0_usize;
        let mut skipped_cached = 0_usize;
        let mut skipped_pending = 0_usize;
        let eligible = entries
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    EntryKind::Image | EntryKind::Archive | EntryKind::Pdf | EntryKind::Video
                )
            })
            .count();

        for entry in entries {
            // サムネイル対象の種別のみ
            if !matches!(
                entry.kind,
                EntryKind::Image | EntryKind::Archive | EntryKind::Pdf | EntryKind::Video
            ) {
                continue;
            }

            // 真値 mtime_ns でキャッシュ確認
            // 注意: modified_at (f64 秒) 経由の近似値ではサブミリ秒以下の精度が
            // 欠落し、実生成側 (warm_one_thumbnail_inner) と常に不一致になる。
            // mtime_ns が None のエントリ (アーカイブ等) は check を飛ばして
            // pending チェックへ進む。
            if let Some(mtime_ns) = entry.mtime_ns {
                let cache_key = state
                    .thumbnail_service
                    .make_cache_key(&entry.node_id, mtime_ns);
                if state.thumbnail_service.is_cached(&cache_key) {
                    skipped_cached += 1;
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
                    skipped_pending += 1;
                    continue;
                }
                pending.insert(entry.node_id.clone());
            }
            scheduled += 1;

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

        tracing::info!(
            total = entries.len(),
            eligible,
            scheduled,
            skipped_cached,
            skipped_pending,
            "thumbnail.warmer scheduled"
        );
    }
}

/// 1 エントリのサムネイルを生成する (sync、`spawn_blocking` 内)
///
/// エラーは無視する (プリウォームのため、失敗してもリクエスト処理に影響しない)
fn warm_one_thumbnail(state: &AppState, node_id: &str) {
    let started = std::time::Instant::now();
    warm_one_thumbnail_inner(state, node_id);
    tracing::info!(
        node_id = short_node_id(node_id),
        source = "warmer",
        elapsed_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
        "thumbnail.generated"
    );
}

/// `node_id` 先頭 8 文字 (PII 配慮 + 相関用)
fn short_node_id(node_id: &str) -> &str {
    &node_id[..node_id.len().min(8)]
}

fn warm_one_thumbnail_inner(state: &AppState, node_id: &str) {
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
            // archive-entry: extract_entry も dedup 対象に含める
            let _ = thumb_svc.generate_with_dedup(&cache_key, || {
                let data = state
                    .archive_service
                    .extract_entry(&archive_path, &entry_name)?;
                thumb_svc.generate_from_bytes_inner(&data, &cache_key)
            });
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

    // アーカイブファイル → 先頭画像 (first_image_entry + extract_entry も dedup 対象に含める)
    if state.archive_service.is_supported(&resolved) {
        let _ = thumb_svc.generate_with_dedup(&cache_key, || {
            let entry = state
                .archive_service
                .first_image_entry(&resolved)?
                .ok_or_else(|| {
                    crate::errors::AppError::NoImage(
                        "アーカイブ内に画像が見つかりません".to_string(),
                    )
                })?;
            let data = state
                .archive_service
                .extract_entry(&resolved, &entry.name)?;
            thumb_svc.generate_from_bytes_inner(&data, &cache_key)
        });
        return;
    }

    // PDF (generate_pdf_thumbnail 内で dedup 済み)
    if PDF_EXTENSIONS.contains(&ext.as_str()) {
        let _ = thumb_svc.generate_pdf_thumbnail(
            &resolved,
            &cache_key,
            state.settings.video_thumb_timeout,
        );
        return;
    }

    // 動画: extract_frame も dedup 対象に含める
    if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        let _ = thumb_svc.generate_with_dedup(&cache_key, || {
            let frame = video_conv.extract_frame(&resolved).ok_or_else(|| {
                crate::errors::AppError::FrameExtractFailed(
                    "動画のフレーム抽出に失敗しました".to_string(),
                )
            })?;
            thumb_svc.generate_from_bytes_inner(&frame, &cache_key)
        });
        return;
    }

    // 画像 (generate_from_path 内で dedup 済み)
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
