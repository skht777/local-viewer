//! ページネーション処理
//!
//! ディレクトリエントリの Two-Phase ソート/ページネーションを担当する。

use crate::errors::AppError;
use crate::services::browse_cursor::{self, SortOrder};
use crate::services::extensions::EntryKind;
use crate::services::models::EntryMeta;
use crate::services::node_registry::{NodeRegistry, scan_entries, scan_entry_metas, stat_entries};

use super::compute_etag;

/// ディレクトリエントリを取得し、ソート/ページネーションを適用する (Two-Phase)
///
/// Phase 1: ロック外で scan + stat + `scan_entry_metas`
/// Phase 2: 短時間ロックで `register_scanned_entries`
#[allow(
    clippy::type_complexity,
    reason = "ページネーション結果のタプルが自然な構造"
)]
pub(super) fn fetch_page(
    registry: &std::sync::Mutex<NodeRegistry>,
    path_security: &crate::services::path_security::PathSecurity,
    path: &std::path::Path,
    sort: SortOrder,
    limit: Option<usize>,
    cursor: Option<&str>,
) -> Result<(Vec<EntryMeta>, Option<String>, usize, String), AppError> {
    let is_name_sort = matches!(sort, SortOrder::NameAsc | SortOrder::NameDesc);

    if is_name_sort && limit.is_some() {
        fetch_page_name_sort(
            registry,
            path_security,
            path,
            sort,
            limit.unwrap_or(0),
            cursor,
        )
    } else {
        fetch_page_full(registry, path_security, path, sort, limit, cursor)
    }
}

/// name ソート + limit 指定時: ページ分だけ stat する最適化パス (Two-Phase)
#[allow(
    clippy::type_complexity,
    reason = "ページネーション結果のタプルが自然な構造"
)]
fn fetch_page_name_sort(
    registry: &std::sync::Mutex<NodeRegistry>,
    path_security: &crate::services::path_security::PathSecurity,
    path: &std::path::Path,
    sort: SortOrder,
    limit_val: usize,
    cursor: Option<&str>,
) -> Result<(Vec<EntryMeta>, Option<String>, usize, String), AppError> {
    // カーソルから node_id を抽出
    let cursor_node_id = cursor
        .map(|c| browse_cursor::decode_cursor(c, sort).map(|d| d.node_id))
        .transpose()?;

    // Phase 1 (ロック外): scan + sort + page slice + stat + build ScannedEntry
    let mut raw = scan_entries(path_security, path)?;
    let total_count = raw.len();
    let reverse = sort == SortOrder::NameDesc;

    // ディレクトリ優先 + 自然順ソート
    use crate::services::natural_sort::natural_sort_key;
    raw.sort_by(|(a_path, a_kind, _), (b_path, b_kind, _)| {
        let a_is_dir = *a_kind == EntryKind::Directory;
        let b_is_dir = *b_kind == EntryKind::Directory;
        b_is_dir.cmp(&a_is_dir).then_with(|| {
            let a_name = a_path.file_name().unwrap_or_default().to_string_lossy();
            let b_name = b_path.file_name().unwrap_or_default().to_string_lossy();
            natural_sort_key(&a_name).cmp(&natural_sort_key(&b_name))
        })
    });

    if reverse {
        let dir_count = raw
            .iter()
            .filter(|(_, k, _)| *k == EntryKind::Directory)
            .count();
        raw[..dir_count].reverse();
        raw[dir_count..].reverse();
    }

    // カーソル位置を検索 (短時間ロック: path_to_id 参照)
    let start_idx = if let Some(ref cursor_id) = cursor_node_id {
        #[allow(
            clippy::expect_used,
            reason = "Mutex poison は致命的エラー、パニックが適切"
        )]
        let reg = registry.lock().expect("NodeRegistry Mutex poisoned");
        raw.iter()
            .position(|(p, _, _)| {
                let key = p.to_string_lossy();
                reg.path_to_id_get(key.as_ref())
                    .is_some_and(|id| id == *cursor_id)
            })
            .map_or(0, |pos| pos + 1)
    } else {
        0
    };

    let fetch_limit = limit_val + 1; // +1 で次ページ有無を判定
    let end_idx = (start_idx + fetch_limit).min(raw.len());
    let page_raw = &raw[start_idx..end_idx];

    // ページ分だけ stat + scan_entry_metas
    let stated: Vec<_> = page_raw
        .iter()
        .map(|(p, k, _)| (p.clone(), *k, std::fs::metadata(p).ok()))
        .collect();
    let scanned = scan_entry_metas(path_security, stated, 3);

    // Phase 2 (短時間ロック): register
    #[allow(
        clippy::expect_used,
        reason = "Mutex poison は致命的エラー、パニックが適切"
    )]
    let all_entries = {
        let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");
        reg.register_scanned_entries(scanned)?
    };

    let has_next = all_entries.len() > limit_val;
    let page: Vec<EntryMeta> = all_entries.into_iter().take(limit_val).collect();

    let etag = compute_etag(&page);
    let next = if has_next {
        page.last()
            .map(|last| browse_cursor::encode_cursor(sort, last, &etag))
    } else {
        None
    };

    Ok((page, next, total_count, etag))
}

/// date ソート or limit なし: 全件取得してからページネーション (Two-Phase)
#[allow(
    clippy::type_complexity,
    reason = "ページネーション結果のタプルが自然な構造"
)]
fn fetch_page_full(
    registry: &std::sync::Mutex<NodeRegistry>,
    path_security: &crate::services::path_security::PathSecurity,
    path: &std::path::Path,
    sort: SortOrder,
    limit: Option<usize>,
    cursor: Option<&str>,
) -> Result<(Vec<EntryMeta>, Option<String>, usize, String), AppError> {
    // Phase 1 (ロック外): scan + stat + build ScannedEntry
    let raw = scan_entries(path_security, path)?;
    let stated = stat_entries(&raw);
    let scanned = scan_entry_metas(path_security, stated, 3);

    // Phase 2 (短時間ロック): register
    #[allow(
        clippy::expect_used,
        reason = "Mutex poison は致命的エラー、パニックが適切"
    )]
    let entries = {
        let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");
        reg.register_scanned_entries(scanned)?
    };

    let total = entries.len();

    if let Some(limit_val) = limit {
        let (page, next, _) = browse_cursor::paginate(entries, sort, Some(limit_val), cursor, "")?;
        let etag = compute_etag(&page);
        // etag 更新後にカーソルを再生成
        let next = if next.is_some() {
            page.last()
                .map(|last| browse_cursor::encode_cursor(sort, last, &etag))
        } else {
            None
        };
        Ok((page, next, total, etag))
    } else {
        // limit なし: ソートのみ
        let sorted = browse_cursor::sort_entries(entries, sort);
        let etag = compute_etag(&sorted);
        Ok((sorted, None, total, etag))
    }
}
