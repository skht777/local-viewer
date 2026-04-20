//! マウントポイント API
//!
//! - `GET /api/mounts` — 全マウントポイントを返す（`list` サブモジュール）
//! - `POST /api/mounts/reload` — `mounts.json` hot reload を実行（`reload` サブモジュール）

mod list;
mod reload;

pub(crate) use list::list_mounts;
pub(crate) use reload::reload_mounts_handler;
