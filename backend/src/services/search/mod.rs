//! 検索 API のサービス層
//!
//! - `scope`: scope `node_id` 検証 + `{mount_id}/{relative}` プレフィックス解決
//! - `resolver`: DB 検索結果を絶対パス経由で `node_id` に解決
//! - `rebuild_rate_limiter`: インデックスリビルドのレート制限

pub(crate) mod rebuild_rate_limiter;
pub(crate) mod resolver;
pub(crate) mod scope;

pub(crate) use resolver::{SearchResultResponse, resolve_search_results};
