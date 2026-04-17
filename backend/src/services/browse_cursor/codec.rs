//! カーソルエンコード / デコード + 署名検証
//!
//! - 形式: `{base64(JSON)}.{sig_hex16}`
//! - 署名: `services/security/cursor_hmac` 経由で HMAC-SHA256 先頭 16 hex
//! - ソート順不一致は decode でエラー

use std::collections::BTreeMap;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::services::extensions::EntryKind;
use crate::services::models::EntryMeta;
use crate::services::security::cursor_hmac::{constant_time_eq, get_secret, hmac_hex_16};

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

impl SortOrder {
    /// シリアライズ表現 (カーソル内部とエラー比較で共有)
    pub(crate) fn as_wire_str(self) -> &'static str {
        match self {
            Self::NameAsc => "name-asc",
            Self::NameDesc => "name-desc",
            Self::DateAsc => "date-asc",
            Self::DateDesc => "date-desc",
        }
    }
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
    payload.insert(
        "s".to_string(),
        serde_json::Value::String(sort.as_wire_str().to_string()),
    );
    payload.insert(
        "sz".to_string(),
        match last_entry.size_bytes {
            Some(v) => serde_json::Value::from(v),
            None => serde_json::Value::Null,
        },
    );

    // base64(payload_json) + "." + hmac(base64_payload)
    // JSON 再シリアライズに依存しない形式で署名する
    let payload_json = serde_json::to_string(&payload).unwrap_or_default();
    let b64_payload = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
    let sig = hmac_hex_16(&secret, &b64_payload);
    format!("{b64_payload}.{sig}")
}

/// カーソルをデコードし、署名と整合性を検証する
///
/// 形式: `{base64_payload}.{sig_hex16}`
/// base64 部分を文字列のまま HMAC 検証し、JSON 再シリアライズに依存しない。
pub(crate) fn decode_cursor(
    cursor_str: &str,
    expected_sort: SortOrder,
) -> Result<CursorData, AppError> {
    let cursor_head = &cursor_str[..cursor_str.len().min(24)];
    let secret = get_secret();

    // "." で分割: base64_payload + sig
    let (b64_payload, sig) = cursor_str.rsplit_once('.').ok_or_else(|| {
        tracing::warn!(
            reason = "format_no_dot",
            cursor_head,
            "カーソルデコード失敗"
        );
        cursor_error("不正なカーソルフォーマットです")
    })?;

    // 署名検証 (base64 文字列をそのまま HMAC 入力に使用)
    let expected_sig = hmac_hex_16(&secret, b64_payload);
    if !constant_time_eq(sig.as_bytes(), expected_sig.as_bytes()) {
        tracing::warn!(
            reason = "sig_mismatch",
            cursor_head,
            sig_in_cursor = sig,
            expected_sig = %expected_sig,
            "カーソルデコード失敗"
        );
        return Err(cursor_error("カーソル署名が不正です"));
    }

    // base64 デコード → JSON パース
    let raw = URL_SAFE_NO_PAD.decode(b64_payload).map_err(|_| {
        tracing::warn!(
            reason = "base64_decode",
            cursor_head,
            "カーソルデコード失敗"
        );
        cursor_error("不正なカーソルフォーマットです")
    })?;
    let json_str = String::from_utf8(raw).map_err(|_| {
        tracing::warn!(reason = "utf8", cursor_head, "カーソルデコード失敗");
        cursor_error("不正なカーソルフォーマットです")
    })?;
    let data: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(&json_str).map_err(|_| {
            tracing::warn!(reason = "json_parse", cursor_head, "カーソルデコード失敗");
            cursor_error("不正なカーソルフォーマットです")
        })?;

    // ソート順の一致確認
    let sort_val = data
        .get("s")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if sort_val != expected_sort.as_wire_str() {
        tracing::warn!(
            reason = "sort_mismatch",
            cursor_head,
            cursor_sort = sort_val,
            expected_sort = expected_sort.as_wire_str(),
            "カーソルデコード失敗"
        );
        return Err(cursor_error("カーソルのソート順がリクエストと一致しません"));
    }

    let node_id = data
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();

    Ok(CursorData { node_id })
}

fn cursor_error(msg: &str) -> AppError {
    AppError::InvalidCursor(msg.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::extensions::EntryKind;

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
        // payload 部分を改ざんして sig を維持 → sig_mismatch
        let (b64_payload, sig) = cursor.rsplit_once('.').unwrap();
        let tampered = format!("{}.{sig}", &b64_payload[..b64_payload.len() - 1]);
        let err = decode_cursor(&tampered, SortOrder::NameAsc).unwrap_err();
        assert!(err.to_string().contains("署名が不正"));
    }

    #[test]
    fn ドット区切りがないカーソルでエラー() {
        let cursor = URL_SAFE_NO_PAD.encode(b"no-dot-separator");
        let err = decode_cursor(&cursor, SortOrder::NameAsc).unwrap_err();
        assert!(err.to_string().contains("不正なカーソルフォーマット"));
    }

    #[test]
    fn 不正なbase64でエラー() {
        let err = decode_cursor("!!!.invalid", SortOrder::NameAsc).unwrap_err();
        assert!(err.to_string().contains("署名が不正"));
    }

    #[test]
    fn ソート順が異なるカーソルでエラー() {
        let e = entry("a.jpg");
        let cursor = encode_cursor(SortOrder::NameAsc, &e, "etag");
        let err = decode_cursor(&cursor, SortOrder::DateDesc).unwrap_err();
        assert!(err.to_string().contains("ソート順"));
    }

    #[test]
    fn ゴールデンベクターが新形式でエンコードされる() {
        let entry = EntryMeta {
            node_id: "abc123".to_string(),
            name: "file.jpg".to_string(),
            kind: EntryKind::Image,
            size_bytes: Some(1024),
            mime_type: None,
            child_count: None,
            modified_at: Some(100.0),
            mtime_ns: None,
            preview_node_ids: None,
        };
        let cursor = encode_cursor(SortOrder::NameAsc, &entry, "etag-1");
        // 新形式: "{base64_payload}.{sig_hex16}"
        let (b64, sig) = cursor.rsplit_once('.').expect("ドット区切りが必要");
        assert_eq!(sig.len(), 16, "sig は 16 hex 文字");
        // base64 部分がデコード可能
        let raw = URL_SAFE_NO_PAD.decode(b64).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(json["id"], "abc123");
        assert_eq!(json["n"], "file.jpg");
        assert_eq!(json["s"], "name-asc");
    }

    #[test]
    fn 精度境界のf64値でラウンドトリップが成功する() {
        // DirIndex 経由: (i64 ns) as f64 / 1e9
        let mtime_ns: i64 = 1_754_710_694_537_162_500;
        #[allow(
            clippy::cast_precision_loss,
            reason = "テスト: DirIndex 経由の変換を再現"
        )]
        let modified_at = mtime_ns as f64 / 1_000_000_000.0;
        let e = entry_with(
            "krm_168o.zip",
            EntryKind::Archive,
            Some(modified_at),
            Some(39_549_293),
        );
        let cursor = encode_cursor(SortOrder::NameAsc, &e, "test-etag");
        let data = decode_cursor(&cursor, SortOrder::NameAsc).unwrap();
        assert_eq!(data.node_id, "id-krm_168o.zip");
    }

    #[test]
    fn 新形式カーソルはドット区切りでsigを分離する() {
        let e = entry_with("a.jpg", EntryKind::Image, Some(100.0), Some(1024));
        let cursor = encode_cursor(SortOrder::NameAsc, &e, "etag-1");
        assert!(
            cursor.contains('.'),
            "新形式カーソルはドットを含むべき: {cursor}"
        );
        let parts: Vec<&str> = cursor.splitn(2, '.').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].len(), 16, "sig は 16 hex 文字");
    }
}
