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
    ///
    /// 対象は 2 系統:
    /// 1. Image / Archive / Pdf / Video エントリ自体
    /// 2. Directory エントリの `preview_node_ids`（カードプレビュー用画像）
    ///
    /// 2 の経路は `preview_node_ids` を opaque な `node_id` 文字列としてのみ扱い、
    /// パスの組み立てや FS access は行わない。実 FS access は
    /// `warm_one_thumbnail_inner` 内側の `NodeRegistry::resolve` 経由のみ
    /// （`path_security` の経路を変えない）。
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

        // 1. Image / Archive / Pdf / Video エントリ自体
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

            match self.try_schedule(&entry.node_id, state) {
                ScheduleResult::Scheduled => scheduled += 1,
                ScheduleResult::SkippedPending => skipped_pending += 1,
            }
        }

        // 2. Directory エントリの preview_node_ids
        // preview の実 mtime は呼び出し側で持っていない（EntryMeta はディレクトリの mtime）。
        // そのため早期 cache 確認はスキップし、warm_one_thumbnail_inner 内部の
        // 解決経路（NodeRegistry → ThumbnailService.generate_*）に任せる。
        // InflightLocks が cache hit / 重複生成を吸収するため、追加コストは小さい。
        for preview_nid in collect_preview_node_ids(entries) {
            match self.try_schedule(preview_nid, state) {
                ScheduleResult::Scheduled => scheduled += 1,
                ScheduleResult::SkippedPending => skipped_pending += 1,
            }
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

    /// 単一 `node_id` を pending に登録し、warm タスクを spawn する
    ///
    /// pending に既に存在する場合は `SkippedPending` を返す。
    fn try_schedule(&self, node_id: &str, state: &Arc<AppState>) -> ScheduleResult {
        if !self.try_register_pending(node_id) {
            return ScheduleResult::SkippedPending;
        }

        let sem = Arc::clone(&self.semaphore);
        let pending = Arc::clone(&self.pending);
        let state = Arc::clone(state);
        let nid = node_id.to_string();

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

        ScheduleResult::Scheduled
    }

    /// `node_id` を pending セットに登録する（純粋同期、テスト容易性のため独立化）
    ///
    /// 戻り値: 新規登録なら `true`、既に存在していたら `false`
    fn try_register_pending(&self, node_id: &str) -> bool {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if pending.contains(node_id) {
            return false;
        }
        pending.insert(node_id.to_string());
        true
    }

    /// 現在の pending 件数（テスト用）
    #[cfg(test)]
    fn pending_count(&self) -> usize {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ScheduleResult {
    Scheduled,
    SkippedPending,
}

/// directory entry 群から `preview_node_ids` を順序保持で連結する（dedup なし）
///
/// 呼び出し側で pending dedup する前提のため、重複もそのまま返す。
fn collect_preview_node_ids(entries: &[EntryMeta]) -> Vec<&str> {
    let mut result = Vec::new();
    for entry in entries {
        if entry.kind != EntryKind::Directory {
            continue;
        }
        let Some(preview_ids) = entry.preview_node_ids.as_ref() else {
            continue;
        };
        for nid in preview_ids {
            result.push(nid.as_str());
        }
    }
    result
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

#[cfg(test)]
mod tests {
    use super::*;

    fn dir_entry(node_id: &str, preview_ids: Option<Vec<&str>>) -> EntryMeta {
        EntryMeta {
            node_id: node_id.to_string(),
            name: format!("dir-{node_id}"),
            kind: EntryKind::Directory,
            size_bytes: None,
            mime_type: None,
            child_count: Some(3),
            modified_at: None,
            mtime_ns: None,
            preview_node_ids: preview_ids.map(|v| v.into_iter().map(String::from).collect()),
        }
    }

    fn image_entry(node_id: &str) -> EntryMeta {
        EntryMeta {
            node_id: node_id.to_string(),
            name: format!("{node_id}.jpg"),
            kind: EntryKind::Image,
            size_bytes: Some(1000),
            mime_type: Some("image/jpeg".to_string()),
            child_count: None,
            modified_at: None,
            mtime_ns: None,
            preview_node_ids: None,
        }
    }

    #[test]
    fn collect_preview_node_idsがdirectoryのpreview_idsを順序保持で返す() {
        let entries = vec![
            image_entry("img1"),
            dir_entry("dir1", Some(vec!["p1", "p2"])),
            dir_entry("dir2", None),
            dir_entry("dir3", Some(vec!["p3"])),
            image_entry("img2"),
        ];
        let preview_ids = collect_preview_node_ids(&entries);
        assert_eq!(preview_ids, vec!["p1", "p2", "p3"]);
    }

    #[test]
    fn collect_preview_node_idsは重複idも保持する() {
        // 複数 directory が同じ preview を持つケース。dedup は呼び出し側 try_register_pending の責務
        let entries = vec![
            dir_entry("dir1", Some(vec!["shared", "p1"])),
            dir_entry("dir2", Some(vec!["shared", "p2"])),
        ];
        let preview_ids = collect_preview_node_ids(&entries);
        assert_eq!(preview_ids, vec!["shared", "p1", "shared", "p2"]);
    }

    #[test]
    fn try_register_pendingが新規登録でtrueを返す() {
        let warmer = ThumbnailWarmer::new(4);
        assert!(warmer.try_register_pending("n1"));
        assert_eq!(warmer.pending_count(), 1);
    }

    #[test]
    fn try_register_pendingが重複でfalseを返しpending件数が増えない() {
        let warmer = ThumbnailWarmer::new(4);
        assert!(warmer.try_register_pending("n1"));
        assert!(!warmer.try_register_pending("n1"));
        assert_eq!(warmer.pending_count(), 1);
    }

    #[test]
    fn 異なるnode_idは独立にpending登録できる() {
        let warmer = ThumbnailWarmer::new(4);
        assert!(warmer.try_register_pending("n1"));
        assert!(warmer.try_register_pending("n2"));
        assert!(warmer.try_register_pending("n3"));
        assert_eq!(warmer.pending_count(), 3);
    }
}
