//! ソート + カーソル + `limit` を適用したページネーション

use crate::errors::AppError;
use crate::services::models::EntryMeta;

use super::codec::{SortOrder, decode_cursor, encode_cursor};
use super::sorting::{apply_cursor, sort_entries};

/// ソート・ページネーションを適用する
///
/// Returns: `(page_entries, next_cursor, total_count)`
#[allow(clippy::type_complexity, reason = "paginate の戻り値はタプルが自然")]
pub(crate) fn paginate(
    entries: Vec<EntryMeta>,
    sort: SortOrder,
    limit: Option<usize>,
    cursor: Option<&str>,
    etag: &str,
) -> Result<(Vec<EntryMeta>, Option<String>, usize), AppError> {
    let total_count = entries.len();

    let mut sorted_entries = sort_entries(entries, sort);

    if let Some(cursor_str) = cursor {
        let cursor_data = decode_cursor(cursor_str, sort)?;
        sorted_entries = apply_cursor(sorted_entries, &cursor_data);
    }

    // limit 省略時は全件返却 (後方互換)
    let Some(limit) = limit else {
        return Ok((sorted_entries, None, total_count));
    };

    let has_next = sorted_entries.len() > limit;
    let page: Vec<EntryMeta> = sorted_entries.into_iter().take(limit).collect();

    let next_cursor = if has_next {
        page.last().map(|last| encode_cursor(sort, last, etag))
    } else {
        None
    };

    Ok((page, next_cursor, total_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::extensions::EntryKind;

    fn entry_with(
        name: &str,
        kind: EntryKind,
        modified_at: Option<f64>,
        size_bytes: Option<u64>,
    ) -> EntryMeta {
        EntryMeta {
            node_id: format!("id-{name}"),
            name: name.to_string(),
            kind,
            size_bytes,
            mime_type: None,
            child_count: None,
            modified_at,
            mtime_ns: None,
            preview_node_ids: None,
        }
    }

    fn paginate_entries() -> Vec<EntryMeta> {
        (0..5)
            .map(|i| {
                entry_with(
                    &format!("file{i}.jpg"),
                    EntryKind::Image,
                    Some(f64::from(i)),
                    None,
                )
            })
            .collect()
    }

    #[test]
    fn limitなしで全件返却しnext_cursorがnone() {
        let (page, next_cursor, total) =
            paginate(paginate_entries(), SortOrder::NameAsc, None, None, "").unwrap();
        assert_eq!(page.len(), 5);
        assert!(next_cursor.is_none());
        assert_eq!(total, 5);
    }

    #[test]
    fn limitで件数が制限される() {
        let (page, next_cursor, total) =
            paginate(paginate_entries(), SortOrder::NameAsc, Some(2), None, "").unwrap();
        assert_eq!(page.len(), 2);
        assert!(next_cursor.is_some());
        assert_eq!(total, 5);
    }

    #[test]
    fn next_cursorで次ページを取得できる() {
        let (page1, cursor1, _) =
            paginate(paginate_entries(), SortOrder::NameAsc, Some(2), None, "").unwrap();
        let (page2, _cursor2, _) = paginate(
            paginate_entries(),
            SortOrder::NameAsc,
            Some(2),
            cursor1.as_deref(),
            "",
        )
        .unwrap();
        // 重複なく連続している
        let all_names: Vec<&str> = page1
            .iter()
            .chain(page2.iter())
            .map(|e| e.name.as_str())
            .collect();
        let unique: std::collections::HashSet<&str> = all_names.iter().copied().collect();
        assert_eq!(all_names.len(), unique.len());
    }

    #[test]
    fn 最終ページでnext_cursorがnone() {
        let (_, cursor1, _) =
            paginate(paginate_entries(), SortOrder::NameAsc, Some(3), None, "").unwrap();
        let (page2, cursor2, _) = paginate(
            paginate_entries(),
            SortOrder::NameAsc,
            Some(3),
            cursor1.as_deref(),
            "",
        )
        .unwrap();
        assert!(cursor2.is_none());
        assert_eq!(page2.len(), 2);
    }

    #[test]
    fn 全ページ走査で全エントリを網羅する() {
        let mut all: Vec<EntryMeta> = Vec::new();
        let mut cursor = None;
        loop {
            let (page, next_cursor, _) = paginate(
                paginate_entries(),
                SortOrder::NameAsc,
                Some(2),
                cursor.as_deref(),
                "",
            )
            .unwrap();
            all.extend(page);
            cursor = next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn 不正なカーソルでエラーが送出される() {
        let result = paginate(
            paginate_entries(),
            SortOrder::NameAsc,
            Some(2),
            Some("invalid"),
            "",
        );
        assert!(result.is_err());
    }
}
