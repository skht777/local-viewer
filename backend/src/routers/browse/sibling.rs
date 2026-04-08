//! 兄弟セット探索エンドポイント

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};

use crate::errors::AppError;
use crate::services::browse_cursor::{self, SortOrder};
use crate::services::dir_index::DirIndex;
use crate::services::extensions::EntryKind;
use crate::services::models::EntryMeta;
use crate::services::node_registry::{NodeRegistry, scan_entries, scan_entry_metas, stat_entries};
use crate::state::AppState;

use super::{SiblingQuery, SiblingResponse, dir_entry_to_entry_meta};

/// `GET /api/browse/{parent_node_id}/sibling`
///
/// 次または前の兄弟セット (directory/archive/pdf) を返す。
#[allow(
    clippy::too_many_lines,
    reason = "Two-Phase Lock Splitting で DirIndex パスとフォールバックパスが分離"
)]
pub(crate) async fn find_sibling(
    State(state): State<Arc<AppState>>,
    Path(parent_node_id): Path<String>,
    Query(query): Query<SiblingQuery>,
) -> Result<Json<SiblingResponse>, AppError> {
    let registry = Arc::clone(&state.node_registry);
    let dir_index = Arc::clone(&state.dir_index);
    let sort = query.sort;
    let current = query.current;
    let direction = query.direction;

    let result = tokio::task::spawn_blocking(move || {
        // Phase 0: 短時間ロックでパス解決 + PathSecurity 取得
        #[allow(
            clippy::expect_used,
            reason = "Mutex poison は致命的エラー、パニックが適切"
        )]
        let (parent_path, path_security) = {
            let reg = registry.lock().expect("NodeRegistry Mutex poisoned");
            let path = reg.resolve(&parent_node_id)?.to_path_buf();
            let ps = reg.path_security_arc();
            (path, ps)
        };
        if !parent_path.is_dir() {
            return Err(AppError::NotADirectory {
                path: parent_path.display().to_string(),
            });
        }

        // DirIndex 高速パス (既存ロックパターン維持)
        if dir_index.is_ready() {
            #[allow(
                clippy::expect_used,
                reason = "Mutex poison は致命的エラー、パニックが適切"
            )]
            let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");
            if let Some(resp) = try_sibling_from_index(
                &dir_index,
                &mut reg,
                &parent_path,
                &current,
                &direction,
                sort,
            ) {
                return Ok(resp);
            }
        }

        // Two-Phase フォールバック: scan 外、register 内
        let raw = scan_entries(&path_security, &parent_path)?;
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

        // 閲覧可能なエントリ (directory, archive, pdf) のみフィルタ
        let candidates: Vec<&EntryMeta> = sorted
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    EntryKind::Directory | EntryKind::Archive | EntryKind::Pdf
                )
            })
            .collect();

        // 現在のエントリを検索
        let current_idx = candidates.iter().position(|e| e.node_id == current);
        let Some(idx) = current_idx else {
            return Ok(SiblingResponse { entry: None });
        };

        // 方向に応じて隣接エントリを返す
        let sibling = match direction.as_str() {
            "next" => {
                if idx + 1 < candidates.len() {
                    Some(candidates[idx + 1].clone())
                } else {
                    None
                }
            }
            "prev" => {
                if idx > 0 {
                    Some(candidates[idx - 1].clone())
                } else {
                    None
                }
            }
            _ => None,
        };

        Ok(SiblingResponse { entry: sibling })
    })
    .await
    .map_err(|e| AppError::path_security(format!("タスク実行エラー: {e}")))?;

    Ok(Json(result?))
}

/// `DirIndex` から隣接エントリを sort 対応のクエリで直接取得する
///
/// `current` `node_id` からファイル名と `is_dir` を取得し、
/// `query_sibling` で sort に応じた SQL 検索を実行。
fn try_sibling_from_index(
    dir_index: &DirIndex,
    reg: &mut NodeRegistry,
    parent_path: &std::path::Path,
    current_node_id: &str,
    direction: &str,
    sort: SortOrder,
) -> Option<SiblingResponse> {
    let parent_key = reg.compute_parent_path_key(parent_path)?;
    let root = reg
        .path_security()
        .find_root_for(parent_path)
        .map(std::path::Path::to_path_buf)?;

    // current node_id からファイル名と is_dir を取得
    let current_path = reg.resolve(current_node_id).ok()?;
    let current_name = current_path.file_name()?.to_string_lossy().into_owned();
    let current_is_dir = current_path.is_dir();

    let sort_str = match sort {
        SortOrder::NameAsc => "name-asc",
        SortOrder::NameDesc => "name-desc",
        SortOrder::DateAsc => "date-asc",
        SortOrder::DateDesc => "date-desc",
    };

    let kinds = &["directory", "archive", "pdf"];
    let de = dir_index
        .query_sibling(
            &parent_key,
            &current_name,
            current_is_dir,
            direction,
            sort_str,
            kinds,
        )
        .ok()??;

    let meta = dir_entry_to_entry_meta(&de, &root, &parent_key, reg)?;
    Some(SiblingResponse { entry: Some(meta) })
}
