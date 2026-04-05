//! ビジネスロジック層
//!
//! レイヤード依存: `routers` → `services` → 外部ライブラリ/stdlib

// Phase 1: 後続フェーズで使用する基盤モジュール (Phase 2+ で参照開始)
#[allow(dead_code, reason = "Phase 2+ で routers / state から参照される")]
pub(crate) mod extensions;
#[allow(dead_code, reason = "Phase 2+ で routers / state から参照される")]
pub(crate) mod models;
#[allow(
    dead_code,
    reason = "Phase 2+ で node_registry / browse_cursor から参照される"
)]
pub(crate) mod natural_sort;
#[allow(
    dead_code,
    reason = "Phase 2+ で node_registry / routers から参照される"
)]
pub(crate) mod path_security;
