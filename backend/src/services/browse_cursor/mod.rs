//! browse API のカーソルベースページネーション
//!
//! - カーソル: base64 エンコード JSON + HMAC-SHA256 署名
//! - ソート順: `name-asc`, `name-desc`, `date-asc`, `date-desc`
//! - 位置復元: `node_id` + ソートキー (名前/日付/サイズ) で一意に特定
//! - 改ざん耐性: `NODE_SECRET` を使った HMAC 署名 (`services/security/cursor_hmac`) で検証
//!
//! モジュール構成:
//! - `codec`: `SortOrder` / `CursorData` / `encode_cursor` / `decode_cursor`
//! - `sorting`: `sort_entries` / `apply_cursor`
//! - `pagination`: `paginate`

mod codec;
mod pagination;
mod sorting;

pub(crate) use codec::{MAX_LIMIT, SortOrder, decode_cursor, encode_cursor};
pub(crate) use pagination::paginate;
pub(crate) use sorting::sort_entries;
