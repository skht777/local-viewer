//! ブラウズ一覧のソートとカーソル位置からのスライス
//!
//! - `name` ソート: ディレクトリ優先 + 自然順
//! - `date` ソート: `modified_at` 順、None は末尾、同一日時は名前昇順タイブレーカー
//! - `apply_cursor`: カーソル位置直後からの要素を返す (先頭フォールバックあり)

use std::cmp::Reverse;

use crate::services::extensions::EntryKind;
use crate::services::models::EntryMeta;
use crate::services::natural_sort::natural_sort_key;

use super::codec::{CursorData, SortOrder};

/// エントリをソート順に並び替える
///
/// name ソートはディレクトリ優先を維持する。
/// date ソートはディレクトリ優先なし (null は末尾)。
pub(crate) fn sort_entries(mut entries: Vec<EntryMeta>, sort: SortOrder) -> Vec<EntryMeta> {
    match sort {
        SortOrder::NameAsc => {
            entries.sort_by_key(|e| (e.kind != EntryKind::Directory, natural_sort_key(&e.name)));
        }
        SortOrder::NameDesc => {
            entries.sort_by_key(|e| {
                (
                    e.kind != EntryKind::Directory,
                    Reverse(natural_sort_key(&e.name)),
                )
            });
        }
        SortOrder::DateDesc => {
            // 同一日時は名前昇順タイブレーカー (Windows Explorer 準拠)
            entries.sort_by(|a, b| {
                let a_none = u8::from(a.modified_at.is_none());
                let b_none = u8::from(b.modified_at.is_none());
                a_none
                    .cmp(&b_none)
                    .then_with(|| {
                        let a_val = a.modified_at.unwrap_or(0.0);
                        let b_val = b.modified_at.unwrap_or(0.0);
                        b_val.total_cmp(&a_val)
                    })
                    .then_with(|| natural_sort_key(&a.name).cmp(&natural_sort_key(&b.name)))
            });
        }
        SortOrder::DateAsc => {
            // 同一日時は名前昇順タイブレーカー (Windows Explorer 準拠)
            entries.sort_by(|a, b| {
                let a_none = u8::from(a.modified_at.is_none());
                let b_none = u8::from(b.modified_at.is_none());
                a_none
                    .cmp(&b_none)
                    .then_with(|| {
                        let a_val = a.modified_at.unwrap_or(0.0);
                        let b_val = b.modified_at.unwrap_or(0.0);
                        a_val.total_cmp(&b_val)
                    })
                    .then_with(|| natural_sort_key(&a.name).cmp(&natural_sort_key(&b.name)))
            });
        }
    }
    entries
}

/// カーソル位置以降のエントリを返す
pub(crate) fn apply_cursor(entries: Vec<EntryMeta>, cursor_data: &CursorData) -> Vec<EntryMeta> {
    for (i, entry) in entries.iter().enumerate() {
        if entry.node_id == cursor_data.node_id {
            return entries[i + 1..].to_vec();
        }
    }
    // カーソルのエントリが見つからない場合は先頭から (フォールバック)
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str) -> EntryMeta {
        EntryMeta {
            node_id: format!("id-{name}"),
            name: name.to_string(),
            kind: EntryKind::Image,
            size_bytes: None,
            mime_type: None,
            child_count: None,
            modified_at: None,
            mtime_ns: None,
            preview_node_ids: None,
        }
    }

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

    fn test_entries() -> Vec<EntryMeta> {
        vec![
            entry_with("file10.jpg", EntryKind::Image, Some(300.0), None),
            entry_with("subdir", EntryKind::Directory, None, None),
            entry_with("file2.jpg", EntryKind::Image, Some(100.0), None),
            entry_with("archive.zip", EntryKind::Archive, Some(200.0), None),
        ]
    }

    #[test]
    fn name_ascでディレクトリが先頭に来る() {
        let result = sort_entries(test_entries(), SortOrder::NameAsc);
        assert_eq!(result[0].kind, EntryKind::Directory);
    }

    #[test]
    fn name_ascで自然順ソートされる() {
        let result = sort_entries(test_entries(), SortOrder::NameAsc);
        let non_dirs: Vec<&str> = result
            .iter()
            .filter(|e| e.kind != EntryKind::Directory)
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(non_dirs, ["archive.zip", "file2.jpg", "file10.jpg"]);
    }

    #[test]
    fn name_descでディレクトリが先頭かつ名前が降順() {
        let result = sort_entries(test_entries(), SortOrder::NameDesc);
        assert_eq!(result[0].kind, EntryKind::Directory);
        let non_dirs: Vec<&str> = result
            .iter()
            .filter(|e| e.kind != EntryKind::Directory)
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(non_dirs, ["file10.jpg", "file2.jpg", "archive.zip"]);
    }

    #[test]
    fn date_descで新しい順に並ぶ() {
        let result = sort_entries(test_entries(), SortOrder::DateDesc);
        let dates: Vec<f64> = result.iter().filter_map(|e| e.modified_at).collect();
        let mut expected = dates.clone();
        expected.sort_by(|a, b| b.total_cmp(a));
        assert_eq!(dates, expected);
    }

    #[test]
    fn date_ascで古い順に並ぶ() {
        let result = sort_entries(test_entries(), SortOrder::DateAsc);
        let dates: Vec<f64> = result.iter().filter_map(|e| e.modified_at).collect();
        let mut expected = dates.clone();
        expected.sort_by(f64::total_cmp);
        assert_eq!(dates, expected);
    }

    #[test]
    fn dateソートでmodified_atがnoneのエントリが末尾() {
        let entries = vec![
            entry_with("a.jpg", EntryKind::Image, None, None),
            entry_with("b.jpg", EntryKind::Image, Some(100.0), None),
        ];
        let result = sort_entries(entries, SortOrder::DateDesc);
        assert!(result.last().unwrap().modified_at.is_none());
    }

    #[test]
    fn dateソートでディレクトリ優先なし() {
        let entries = vec![
            entry_with("dir", EntryKind::Directory, None, None),
            entry_with("a.jpg", EntryKind::Image, Some(100.0), None),
        ];
        let result = sort_entries(entries, SortOrder::DateDesc);
        assert_eq!(result[0].name, "a.jpg");
    }

    #[test]
    fn 空リストで空リストを返す() {
        assert!(sort_entries(vec![], SortOrder::NameAsc).is_empty());
    }

    #[test]
    fn date_descで同一日時は名前昇順のタイブレーカー() {
        let entries = vec![
            entry_with("beta.jpg", EntryKind::Image, Some(100.0), None),
            entry_with("alpha.jpg", EntryKind::Image, Some(100.0), None),
        ];
        let result = sort_entries(entries, SortOrder::DateDesc);
        assert_eq!(result[0].name, "alpha.jpg");
        assert_eq!(result[1].name, "beta.jpg");
    }

    #[test]
    fn date_ascで同一日時は名前昇順のタイブレーカー() {
        let entries = vec![
            entry_with("beta.jpg", EntryKind::Image, Some(100.0), None),
            entry_with("alpha.jpg", EntryKind::Image, Some(100.0), None),
        ];
        let result = sort_entries(entries, SortOrder::DateAsc);
        assert_eq!(result[0].name, "alpha.jpg");
        assert_eq!(result[1].name, "beta.jpg");
    }

    #[test]
    fn カーソル位置の次から返す() {
        let entries = vec![entry("a"), entry("b"), entry("c")];
        let cursor_data = CursorData {
            node_id: "id-b".to_string(),
        };
        let result = apply_cursor(entries, &cursor_data);
        let names: Vec<&str> = result.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["c"]);
    }

    #[test]
    fn カーソルが先頭の場合は残り全件を返す() {
        let entries = vec![entry("a"), entry("b"), entry("c")];
        let cursor_data = CursorData {
            node_id: "id-a".to_string(),
        };
        let result = apply_cursor(entries, &cursor_data);
        let names: Vec<&str> = result.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["b", "c"]);
    }

    #[test]
    fn カーソルが末尾の場合は空リストを返す() {
        let entries = vec![entry("a"), entry("b")];
        let cursor_data = CursorData {
            node_id: "id-b".to_string(),
        };
        let result = apply_cursor(entries, &cursor_data);
        assert!(result.is_empty());
    }

    #[test]
    fn 存在しないカーソルidでは先頭から全件を返す() {
        let entries = vec![entry("a"), entry("b")];
        let cursor_data = CursorData {
            node_id: "nonexistent".to_string(),
        };
        let result = apply_cursor(entries, &cursor_data);
        let names: Vec<&str> = result.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
    }
}
