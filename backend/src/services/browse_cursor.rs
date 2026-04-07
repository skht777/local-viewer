//! browse API のカーソルベースページネーション
//!
//! - カーソル: base64 エンコード JSON + HMAC-SHA256 署名
//! - ソート順: `name-asc`, `name-desc`, `date-asc`, `date-desc`
//! - 位置復元: `node_id` + ソートキー (名前/日付/サイズ) で一意に特定
//! - 改ざん耐性: `NODE_SECRET` を使った HMAC 署名で検証

use std::cmp::Reverse;
use std::collections::BTreeMap;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::errors::AppError;
use crate::services::extensions::EntryKind;
use crate::services::models::EntryMeta;
use crate::services::natural_sort::natural_sort_key;

type HmacSha256 = Hmac<Sha256>;

/// ソート順序
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SortOrder {
    #[serde(rename = "name-asc")]
    NameAsc,
    #[serde(rename = "name-desc")]
    NameDesc,
    #[serde(rename = "date-asc")]
    DateAsc,
    #[serde(rename = "date-desc")]
    DateDesc,
}

/// ページネーションのデフォルト値
pub(crate) const DEFAULT_LIMIT: usize = 100;
pub(crate) const MAX_LIMIT: usize = 500;

/// デコード済みカーソルデータ
#[derive(Debug)]
pub(crate) struct CursorData {
    pub node_id: String,
}

/// カーソルを base64 + HMAC 署名でエンコードする
pub(crate) fn encode_cursor(sort: SortOrder, last_entry: &EntryMeta, etag: &str) -> String {
    let secret = get_secret();
    let mut payload = BTreeMap::new();
    payload.insert(
        "d".to_string(),
        serde_json::Value::Bool(last_entry.kind == EntryKind::Directory),
    );
    payload.insert(
        "et".to_string(),
        serde_json::Value::String(etag.to_string()),
    );
    payload.insert(
        "id".to_string(),
        serde_json::Value::String(last_entry.node_id.clone()),
    );
    payload.insert(
        "m".to_string(),
        match last_entry.modified_at {
            Some(v) => serde_json::Value::from(v),
            None => serde_json::Value::Null,
        },
    );
    payload.insert(
        "n".to_string(),
        serde_json::Value::String(last_entry.name.clone()),
    );
    // SortOrder のシリアライズ (BTreeMap のキーに "s" を使用)
    let sort_str = match sort {
        SortOrder::NameAsc => "name-asc",
        SortOrder::NameDesc => "name-desc",
        SortOrder::DateAsc => "date-asc",
        SortOrder::DateDesc => "date-desc",
    };
    payload.insert(
        "s".to_string(),
        serde_json::Value::String(sort_str.to_string()),
    );
    payload.insert(
        "sz".to_string(),
        match last_entry.size_bytes {
            Some(v) => serde_json::Value::from(v),
            None => serde_json::Value::Null,
        },
    );

    // 署名生成 (ペイロード JSON に対して HMAC)
    let payload_json = serde_json::to_string(&payload).unwrap_or_default();
    let sig = hmac_hex_16(&secret, &payload_json);

    // sig を追加して再シリアライズ
    payload.insert("sig".to_string(), serde_json::Value::String(sig));
    let signed_json = serde_json::to_string(&payload).unwrap_or_default();

    URL_SAFE_NO_PAD.encode(signed_json.as_bytes())
}

/// カーソルをデコードし、署名と整合性を検証する
pub(crate) fn decode_cursor(
    cursor_str: &str,
    expected_sort: SortOrder,
) -> Result<CursorData, AppError> {
    let secret = get_secret();

    // base64 デコード
    let raw = URL_SAFE_NO_PAD
        .decode(cursor_str)
        .map_err(|_| cursor_error("不正なカーソルフォーマットです"))?;
    let json_str =
        String::from_utf8(raw).map_err(|_| cursor_error("不正なカーソルフォーマットです"))?;
    let mut data: BTreeMap<String, serde_json::Value> = serde_json::from_str(&json_str)
        .map_err(|_| cursor_error("不正なカーソルフォーマットです"))?;

    // 署名抽出
    let sig = data
        .remove("sig")
        .and_then(|v| v.as_str().map(String::from))
        .ok_or_else(|| cursor_error("カーソル署名がありません"))?;

    // 署名検証 (sig を除いたペイロードで HMAC)
    let payload_json = serde_json::to_string(&data).unwrap_or_default();
    let expected_sig = hmac_hex_16(&secret, &payload_json);
    if !constant_time_eq(sig.as_bytes(), expected_sig.as_bytes()) {
        return Err(cursor_error("カーソル署名が不正です"));
    }

    // ソート順の一致確認
    let sort_val = data
        .get("s")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let expected_sort_str = match expected_sort {
        SortOrder::NameAsc => "name-asc",
        SortOrder::NameDesc => "name-desc",
        SortOrder::DateAsc => "date-asc",
        SortOrder::DateDesc => "date-desc",
    };
    if sort_val != expected_sort_str {
        return Err(cursor_error("カーソルのソート順がリクエストと一致しません"));
    }

    let node_id = data
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();

    Ok(CursorData { node_id })
}

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

    // ソート
    let mut sorted_entries = sort_entries(entries, sort);

    // カーソル適用
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

    // next_cursor 生成
    let next_cursor = if has_next {
        page.last().map(|last| encode_cursor(sort, last, etag))
    } else {
        None
    };

    Ok((page, next_cursor, total_count))
}

/// `NODE_SECRET` を取得する
fn get_secret() -> Vec<u8> {
    std::env::var("NODE_SECRET")
        .unwrap_or_else(|_| "local-viewer-default-secret".to_string())
        .into_bytes()
}

/// HMAC-SHA256 の先頭 16 hex 文字を返す
fn hmac_hex_16(secret: &[u8], input: &str) -> String {
    #[allow(clippy::expect_used, reason = "HMAC-SHA256 は任意長の鍵を受け付ける")]
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC は任意長の鍵を受け付ける");
    mac.update(input.as_bytes());
    let result = mac.finalize().into_bytes();
    hex::encode(result)[..16].to_string()
}

/// 定数時間比較
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// カーソルエラーのヘルパー
fn cursor_error(msg: &str) -> AppError {
    AppError::InvalidCursor(msg.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    // --- ヘルパー ---

    fn entry(name: &str) -> EntryMeta {
        EntryMeta {
            node_id: format!("id-{name}"),
            name: name.to_string(),
            kind: EntryKind::Image,
            size_bytes: None,
            mime_type: None,
            child_count: None,
            modified_at: None,
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
            preview_node_ids: None,
        }
    }

    // --- encode_cursor / decode_cursor ---

    #[test]
    fn ラウンドトリップで元のデータを復元できる() {
        let e = entry_with("a.jpg", EntryKind::Image, Some(100.0), Some(1024));
        let cursor = encode_cursor(SortOrder::NameAsc, &e, "etag-1");
        let data = decode_cursor(&cursor, SortOrder::NameAsc).unwrap();
        assert_eq!(data.node_id, "id-a.jpg");
    }

    #[test]
    fn modified_atがnoneでもエンコードできる() {
        let e = entry_with("dir", EntryKind::Directory, None, None);
        let cursor = encode_cursor(SortOrder::DateDesc, &e, "");
        let data = decode_cursor(&cursor, SortOrder::DateDesc).unwrap();
        assert_eq!(data.node_id, "id-dir");
    }

    #[test]
    fn 改ざんされたカーソルでエラー() {
        let e = entry("a.jpg");
        let cursor = encode_cursor(SortOrder::NameAsc, &e, "etag");
        // base64 デコード → JSON 改ざん → 再エンコード
        let raw = URL_SAFE_NO_PAD.decode(&cursor).unwrap();
        let mut data: BTreeMap<String, serde_json::Value> = serde_json::from_slice(&raw).unwrap();
        data.insert(
            "n".to_string(),
            serde_json::Value::String("tampered.jpg".to_string()),
        );
        let tampered_json = serde_json::to_string(&data).unwrap();
        let tampered = URL_SAFE_NO_PAD.encode(tampered_json.as_bytes());
        let err = decode_cursor(&tampered, SortOrder::NameAsc).unwrap_err();
        assert!(err.to_string().contains("署名が不正"));
    }

    #[test]
    fn 署名がないカーソルでエラー() {
        let payload = r#"{"s":"name-asc","id":"x"}"#;
        let cursor = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let err = decode_cursor(&cursor, SortOrder::NameAsc).unwrap_err();
        assert!(err.to_string().contains("署名がありません"));
    }

    #[test]
    fn 不正なbase64でエラー() {
        let err = decode_cursor("!!!invalid!!!", SortOrder::NameAsc).unwrap_err();
        assert!(err.to_string().contains("不正なカーソルフォーマット"));
    }

    #[test]
    fn ソート順が異なるカーソルでエラー() {
        let e = entry("a.jpg");
        let cursor = encode_cursor(SortOrder::NameAsc, &e, "etag");
        let err = decode_cursor(&cursor, SortOrder::DateDesc).unwrap_err();
        assert!(err.to_string().contains("ソート順"));
    }

    // --- sort_entries ---

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

    // --- apply_cursor ---

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

    // --- paginate ---

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

    // --- カーソル JSON 互換テスト ---

    #[test]
    fn カーソルjsonのゴールデンベクター() {
        // Python で生成済みベクター
        let expected_cursor = "eyJkIjpmYWxzZSwiZXQiOiJldGFnLTEiLCJpZCI6ImFiYzEyMyIsIm0iOjEwMC4wLCJuIjoiZmlsZS5qcGciLCJzIjoibmFtZS1hc2MiLCJzaWciOiI5MGZmYjYyNTA4ODg2ZjhlIiwic3oiOjEwMjR9";

        // 同じ入力で Rust 側でもエンコード
        let entry = EntryMeta {
            node_id: "abc123".to_string(),
            name: "file.jpg".to_string(),
            kind: EntryKind::Image,
            size_bytes: Some(1024),
            mime_type: None,
            child_count: None,
            modified_at: Some(100.0),
            preview_node_ids: None,
        };
        let cursor = encode_cursor(SortOrder::NameAsc, &entry, "etag-1");
        assert_eq!(cursor, expected_cursor);
    }
}
