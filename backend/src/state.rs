//! アプリケーション状態 (DI コンテナ相当)
//!
//! 全サービスを `Arc` で保持し、axum の `State` エクストラクタで各ハンドラに注入する。

use std::sync::{Arc, Mutex};

use crate::config::Settings;
use crate::services::node_registry::NodeRegistry;

#[allow(dead_code, reason = "Step 7 の main.rs 統合で使用")]
/// アプリケーション共有状態
///
/// - `settings`: 環境変数ベースの設定 (不変)
/// - `node_registry`: `Mutex` で保護 (`register` 等が `&mut self`)
///   `path_security` は `node_registry.path_security()` 経由でアクセス可能
pub(crate) struct AppState {
    #[allow(dead_code, reason = "Phase 3+ のルーターで使用")]
    pub settings: Arc<Settings>,
    pub node_registry: Arc<Mutex<NodeRegistry>>,
}
