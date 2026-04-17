//! ビジネスロジック層
//!
//! レイヤード依存: `routers` → `services` → 外部ライブラリ/stdlib

#[allow(dead_code, reason = "Phase 4 で段階的に使用開始")]
pub(crate) mod archive;
#[allow(dead_code, reason = "Phase 5+ で DEFAULT_LIMIT を使用")]
pub(crate) mod browse_cursor;
#[allow(dead_code, reason = "Phase 6b で browse ルーターから使用")]
pub(crate) mod dir_index;
#[allow(dead_code, reason = "Phase 5 で REMUX_EXTENSIONS を使用")]
pub(crate) mod extensions;
#[allow(dead_code, reason = "Phase 6b の main.rs 統合で使用")]
pub(crate) mod file_watcher;
#[allow(dead_code, reason = "Phase 6a の search ルーターで使用")]
pub(crate) mod indexer;
pub(crate) mod models;
#[allow(dead_code, reason = "Phase 5+ で save/add/remove を使用")]
pub(crate) mod mount_config;
#[allow(dead_code, reason = "Phase 6b で encode_sort_key を使用")]
pub(crate) mod natural_sort;
#[allow(dead_code, reason = "Phase 4 でアーカイブメソッドを使用")]
pub(crate) mod node_registry;
#[allow(dead_code, reason = "Phase 6a の Indexer スキャンで使用")]
pub(crate) mod parallel_walk;
#[allow(dead_code, reason = "Phase 4+ で追加メソッドを使用")]
pub(crate) mod path_security;
pub(crate) mod security;
#[allow(dead_code, reason = "Phase 5 のサムネイル・動画サービスで使用")]
pub(crate) mod temp_file_cache;
#[allow(dead_code, reason = "Phase 5 のサムネイルルーターで使用")]
pub(crate) mod thumbnail_service;
#[allow(dead_code, reason = "Phase 5 のサムネイルルーターで使用")]
pub(crate) mod thumbnail_warmer;
#[allow(dead_code, reason = "Phase 5 のサムネイルルーターで使用")]
pub(crate) mod video_converter;
