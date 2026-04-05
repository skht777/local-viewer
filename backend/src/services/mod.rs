//! ビジネスロジック層
//!
//! レイヤード依存: `routers` → `services` → 外部ライブラリ/stdlib

// Phase 2 で使用中。Phase 3+ で追加 API が使われるまで一部が未使用。
#[allow(dead_code, reason = "Phase 3+ で追加のAPI/定数を使用")]
pub(crate) mod browse_cursor;
#[allow(dead_code, reason = "Phase 3+ で追加のAPI/定数を使用")]
pub(crate) mod extensions;
pub(crate) mod models;
#[allow(dead_code, reason = "Phase 3+ で save/add/remove を使用")]
pub(crate) mod mount_config;
#[allow(dead_code, reason = "Phase 6b で encode_sort_key を使用")]
pub(crate) mod natural_sort;
#[allow(dead_code, reason = "Phase 3+ でアーカイブ/追加メソッドを使用")]
pub(crate) mod node_registry;
#[allow(dead_code, reason = "Phase 3+ で追加メソッドを使用")]
pub(crate) mod path_security;
