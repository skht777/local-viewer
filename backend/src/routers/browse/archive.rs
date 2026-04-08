//! アーカイブファイルのブラウズ処理

use std::sync::Arc;

use crate::errors::AppError;
use crate::services::browse_cursor::{self, SortOrder};
use crate::services::extensions::{self, EntryKind};
use crate::services::models::{AncestorEntry, BrowseResponse, EntryMeta};
use crate::state::AppState;

use super::compute_etag;

/// アーカイブファイルをディレクトリとして閲覧する
///
/// - `archive_service.list_entries()` でエントリ一覧取得 (ロック外)
/// - `NodeRegistry` にアーカイブエントリを登録 (短ロック)
/// - `BrowseResponse` を構築して返す
#[allow(
    clippy::too_many_lines,
    reason = "ページネーション追加で一時的に超過、将来分割予定"
)]
pub(super) async fn browse_archive(
    state: &Arc<AppState>,
    archive_path: &std::path::Path,
    archive_node_id: &str,
    sort: SortOrder,
    limit: Option<usize>,
    cursor: Option<&str>,
) -> Result<(BrowseResponse, String), AppError> {
    // Step 1: アーカイブエントリ一覧を取得 (ロック外で I/O)
    let svc = Arc::clone(&state.archive_service);
    let path = archive_path.to_path_buf();
    let arc_entries = tokio::task::spawn_blocking(move || svc.list_entries(&path))
        .await
        .map_err(|e| AppError::path_security(format!("タスク実行エラー: {e}")))?
        .map_err(|e| match e {
            // zip/rar/7z ライブラリのエラーを InvalidArchive に正規化
            AppError::ArchiveSecurity(_) | AppError::ArchivePassword(_) => e,
            _ => AppError::InvalidArchive(e.to_string()),
        })?;

    // Step 2: NodeRegistry にエントリを登録して EntryMeta を構築 (短ロック)
    let registry = Arc::clone(&state.node_registry);
    let a_path = archive_path.to_path_buf();
    let a_nid = archive_node_id.to_string();
    let entries_clone = Arc::clone(&arc_entries);

    let (entry_metas, parent_node_id, ancestors) =
        tokio::task::spawn_blocking(move || -> Result<_, AppError> {
            #[allow(
                clippy::expect_used,
                reason = "Mutex poison は致命的エラー、パニックが適切"
            )]
            let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");

            let mut metas = Vec::with_capacity(entries_clone.len());
            for entry in entries_clone.iter() {
                // アーカイブエントリの node_id を登録
                let entry_node_id = reg.register_archive_entry(&a_path, &entry.name)?;

                // エントリ名からファイル名部分を取得 (パスの最後の要素)
                let display_name = entry
                    .name
                    .rsplit('/')
                    .next()
                    .unwrap_or(&entry.name)
                    .to_string();

                // 拡張子から kind と mime_type を判定
                let ext = extensions::extract_extension(&display_name).to_ascii_lowercase();
                let kind = EntryKind::from_extension(&ext);
                let mime_type = extensions::mime_for_extension(&ext).map(String::from);

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

            // パンくずリスト
            let parent_node_id = reg.get_parent_node_id(&a_path);
            let ancestors = reg
                .get_ancestors(&a_path)
                .into_iter()
                .map(|(nid, name)| AncestorEntry { node_id: nid, name })
                .collect::<Vec<_>>();

            Ok((metas, parent_node_id, ancestors))
        })
        .await
        .map_err(|e| AppError::path_security(format!("タスク実行エラー: {e}")))??;

    // ソート・ページネーション (fetch_page_full と同じパターン)
    let total = entry_metas.len();
    let (page_entries, next_cursor, etag) = if let Some(limit_val) = limit {
        let (page, next, _) =
            browse_cursor::paginate(entry_metas, sort, Some(limit_val), cursor, "")?;
        let etag = compute_etag(&page);
        let next = if next.is_some() {
            page.last()
                .map(|last| browse_cursor::encode_cursor(sort, last, &etag))
        } else {
            None
        };
        (page, next, etag)
    } else {
        let sorted = browse_cursor::sort_entries(entry_metas, sort);
        let etag = compute_etag(&sorted);
        (sorted, None, etag)
    };

    let archive_name = archive_path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());

    let response = BrowseResponse {
        current_node_id: Some(a_nid.clone()),
        current_name: archive_name,
        parent_node_id,
        ancestors,
        entries: page_entries,
        next_cursor,
        total_count: if limit.is_some() { Some(total) } else { None },
    };

    Ok((response, etag))
}
