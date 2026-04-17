//! セキュリティ関連の共通プリミティブ
//!
//! - `cursor_hmac`: `NODE_SECRET` ベースの HMAC-SHA256 署名ヘルパー

pub(crate) mod cursor_hmac;
