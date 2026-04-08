//! 最初の閲覧対象を再帰的に探索するエンドポイント

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};

use crate::errors::AppError;
use crate::services::browse_cursor::{self};
use crate::services::dir_index::DirIndex;
use crate::services::extensions::{self, EntryKind};
use crate::services::models::EntryMeta;
use crate::services::node_registry::{NodeRegistry, scan_entries, scan_entry_metas, stat_entries};
use crate::state::AppState;

use super::{FirstViewableQuery, FirstViewableResponse, dir_entry_to_entry_meta};

/// `GET /api/browse/{node_id}/first-viewable`
///
/// ディレクトリまたはアーカイブ内の最初の閲覧対象を再帰的に探索する。
/// 優先順位: archive > pdf > image > directory (再帰降下)
/// アーカイブの `node_id` が渡された場合は中身を探索。
/// 最大 10 レベルまで再帰。
#[allow(
    clippy::too_many_lines,
    reason = "アーカイブ対応追加で一時的に超過、将来ヘルパー抽出予定"
)]
pub(crate) async fn first_viewable(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
    Query(query): Query<FirstViewableQuery>,
) -> Result<Json<FirstViewableResponse>, AppError> {
    let registry = Arc::clone(&state.node_registry);
    let dir_index = Arc::clone(&state.dir_index);
    let archive_service = Arc::clone(&state.archive_service);
    let sort = query.sort;

    let result = tokio::task::spawn_blocking(move || {
        // PathSecurity を短時間ロックで取得 (ループ外)
        #[allow(
            clippy::expect_used,
            reason = "Mutex poison は致命的エラー、パニックが適切"
        )]
        let path_security = {
            let reg = registry.lock().expect("NodeRegistry Mutex poisoned");
            reg.path_security_arc()
        };

        let max_depth = 10;
        let mut current_id = node_id;

        for _ in 0..max_depth {
            // 短時間ロック: resolve のみ
            #[allow(
                clippy::expect_used,
                reason = "Mutex poison は致命的エラー、パニックが適切"
            )]
            let path = {
                let reg = registry.lock().expect("NodeRegistry Mutex poisoned");
                reg.resolve(&current_id)?.to_path_buf()
            };

            // アーカイブファイルの場合: 中身から最初の閲覧対象を探す (ロック外で I/O)
            if path.is_file() && extensions::is_archive_extension(&path) {
                match archive_service.list_entries(&path) {
                    Ok(arc_entries) => {
                        // 短時間ロック: アーカイブエントリ登録のみ
                        #[allow(
                            clippy::expect_used,
                            reason = "Mutex poison は致命的エラー、パニックが適切"
                        )]
                        let metas = {
                            let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");
                            let mut metas = Vec::with_capacity(arc_entries.len());
                            for entry in arc_entries.iter() {
                                let entry_node_id =
                                    reg.register_archive_entry(&path, &entry.name)?;
                                let display_name = entry
                                    .name
                                    .rsplit('/')
                                    .next()
                                    .unwrap_or(&entry.name)
                                    .to_string();
                                let ext = extensions::extract_extension(&display_name)
                                    .to_ascii_lowercase();
                                let kind = EntryKind::from_extension(&ext);
                                let mime_type =
                                    extensions::mime_for_extension(&ext).map(String::from);
                                metas.push(EntryMeta {
                                    node_id: entry_node_id,
                                    name: display_name,
                                    kind,
                                    size_bytes: Some(entry.size_uncompressed),
                                    mime_type,
                                    child_count: None,
                                    modified_at: None,
                                    preview_node_ids: None,
                                });
                            }
                            metas
                        };
                        let sorted = browse_cursor::sort_entries(metas, sort);
                        let viewable = select_first_viewable(&sorted);
                        return Ok(FirstViewableResponse {
                            entry: viewable.cloned(),
                            parent_node_id: Some(current_id),
                        });
                    }
                    Err(_) => {
                        return Ok(FirstViewableResponse {
                            entry: None,
                            parent_node_id: None,
                        });
                    }
                }
            }

            if !path.is_dir() {
                break;
            }

            // DirIndex 高速パス (既存ロックパターン維持)
            if dir_index.is_ready() {
                #[allow(
                    clippy::expect_used,
                    reason = "Mutex poison は致命的エラー、パニックが適切"
                )]
                let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");
                if let Some(result) =
                    try_first_viewable_from_index(&dir_index, &mut reg, &path, &current_id)
                {
                    return Ok(result);
                }
            }

            // Two-Phase フォールバック: scan 外、register 内
            let raw = scan_entries(&path_security, &path)?;
            let stated = stat_entries(&raw);
            let scanned = scan_entry_metas(&path_security, stated, 3);

            #[allow(
                clippy::expect_used,
                reason = "Mutex poison は致命的エラー、パニックが適切"
            )]
            let entries = {
                let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");
                reg.register_scanned_entries(scanned)?
            };

            let sorted = browse_cursor::sort_entries(entries, sort);
            let viewable = select_first_viewable(&sorted);

            let Some(entry) = viewable else {
                return Ok(FirstViewableResponse {
                    entry: None,
                    parent_node_id: None,
                });
            };

            // archive, pdf, image は直接返す
            if matches!(
                entry.kind,
                EntryKind::Archive | EntryKind::Pdf | EntryKind::Image
            ) {
                return Ok(FirstViewableResponse {
                    entry: Some(entry.clone()),
                    parent_node_id: Some(current_id),
                });
            }

            // directory → 再帰降下
            current_id.clone_from(&entry.node_id);
        }

        Ok(FirstViewableResponse {
            entry: None,
            parent_node_id: None,
        })
    })
    .await
    .map_err(|e| AppError::path_security(format!("タスク実行エラー: {e}")))?;

    Ok(Json(result?))
}

/// `DirIndex` から最初の閲覧対象を kind 優先で探索する
///
/// archive > pdf > image の順に `first_entry_by_kind` を試行。
/// ヒットすればパス解決 + `NodeRegistry` 登録して返す。
fn try_first_viewable_from_index(
    dir_index: &DirIndex,
    reg: &mut NodeRegistry,
    path: &std::path::Path,
    current_id: &str,
) -> Option<FirstViewableResponse> {
    let parent_key = reg.compute_parent_path_key(path)?;
    let root = reg
        .path_security()
        .find_root_for(path)
        .map(std::path::Path::to_path_buf)?;

    for kind in ["archive", "pdf", "image"] {
        if let Ok(Some(de)) = dir_index.first_entry_by_kind(&parent_key, kind) {
            if let Some(meta) = dir_entry_to_entry_meta(&de, &root, &parent_key, reg) {
                return Some(FirstViewableResponse {
                    entry: Some(meta),
                    parent_node_id: Some(current_id.to_string()),
                });
            }
        }
    }
    // 閲覧対象なし — DirIndex ではディレクトリ再帰降下を行わない (フォールバックに任せる)
    None
}

/// ソート済みエントリから最初の閲覧対象を選ぶ
///
/// 優先順位: archive > pdf > image > directory (再帰降下用)
fn select_first_viewable(entries: &[EntryMeta]) -> Option<&EntryMeta> {
    for kind in [EntryKind::Archive, EntryKind::Pdf, EntryKind::Image] {
        if let Some(entry) = entries.iter().find(|e| e.kind == kind) {
            return Some(entry);
        }
    }
    // 閲覧対象なし → directory を探す (再帰降下用)
    entries.iter().find(|e| e.kind == EntryKind::Directory)
}
