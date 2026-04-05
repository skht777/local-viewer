//! API データモデル
//!
//! `browse_cursor` と `node_registry` の両方から参照される共通モデル。
//! Python 版 `node_registry.py` L49-98 と同一のフィールド構成。

use serde::{Deserialize, Serialize};

use super::extensions::EntryKind;

/// パンくずリスト用の祖先エントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AncestorEntry {
    pub node_id: String,
    pub name: String,
}

/// browse レスポンスの 1 エントリ
///
/// - `node_id`: 不透明 ID
/// - `name`: ファイル/ディレクトリ名
/// - `kind`: エントリの種類
/// - `size_bytes`: ファイルサイズ (ディレクトリは `None`)
/// - `mime_type`: MIME タイプ (ディレクトリは `None`)
/// - `child_count`: ディレクトリの子エントリ数 (ファイルは `None`)
/// - `modified_at`: 更新日時 POSIX epoch 秒 (アーカイブエントリ・マウントルートは `None`)
/// - `preview_node_ids`: ディレクトリ内の先頭画像 `node_id` (最大3件)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct EntryMeta {
    pub node_id: String,
    pub name: String,
    pub kind: EntryKind,
    pub size_bytes: Option<u64>,
    pub mime_type: Option<String>,
    pub child_count: Option<usize>,
    pub modified_at: Option<f64>,
    pub preview_node_ids: Option<Vec<String>>,
}

/// browse API のレスポンス
///
/// - `current_node_id`: 現在のディレクトリの `node_id` (ルートは `None`)
/// - `current_name`: 現在のディレクトリ名
/// - `parent_node_id`: 親ディレクトリの `node_id` (ルートは `None`)
/// - `ancestors`: 祖先エントリ (マウントルートから親まで、パンくず用)
/// - `entries`: 子エントリ一覧
/// - `next_cursor`: 次ページカーソル (null = 最終ページ or ページネーション未使用)
/// - `total_count`: 全エントリ数 (ページネーション使用時のみ)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BrowseResponse {
    pub current_node_id: Option<String>,
    pub current_name: String,
    pub parent_node_id: Option<String>,
    #[serde(default)]
    pub ancestors: Vec<AncestorEntry>,
    pub entries: Vec<EntryMeta>,
    pub next_cursor: Option<String>,
    pub total_count: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entrymeta_nullフィールドがnullとして出力される() {
        let entry = EntryMeta {
            node_id: "abc123".to_string(),
            name: "test.jpg".to_string(),
            kind: EntryKind::Image,
            size_bytes: Some(1024),
            mime_type: Some("image/jpeg".to_string()),
            child_count: None,
            modified_at: None,
            preview_node_ids: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        // None フィールドは null として常に出力 (フロントエンド T | null 互換)
        assert!(json.contains(r#""child_count":null"#));
        assert!(json.contains(r#""modified_at":null"#));
        assert!(json.contains(r#""preview_node_ids":null"#));
        assert!(json.contains("size_bytes"));
        assert!(json.contains("mime_type"));
    }

    #[test]
    fn browseresponse_json直列化が正しい() {
        let resp = BrowseResponse {
            current_node_id: Some("node1".to_string()),
            current_name: "photos".to_string(),
            parent_node_id: Some("parent1".to_string()),
            ancestors: vec![AncestorEntry {
                node_id: "root1".to_string(),
                name: "pictures".to_string(),
            }],
            entries: vec![],
            next_cursor: None,
            total_count: Some(0),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("current_name"));
        assert!(json.contains("ancestors"));
        // None フィールドは null として常に出力
        assert!(json.contains(r#""next_cursor":null"#));
    }

    #[test]
    fn entrymeta_デシリアライズでオプションフィールド省略可能() {
        let json = r#"{"node_id":"x","name":"f.jpg","kind":"image"}"#;
        let entry: EntryMeta = serde_json::from_str(json).unwrap();
        assert_eq!(entry.node_id, "x");
        assert!(entry.size_bytes.is_none());
        assert!(entry.modified_at.is_none());
    }

    #[test]
    fn ancestorentry_json直列化が正しい() {
        let a = AncestorEntry {
            node_id: "n1".to_string(),
            name: "dir".to_string(),
        };
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(json, r#"{"node_id":"n1","name":"dir"}"#);
    }
}
