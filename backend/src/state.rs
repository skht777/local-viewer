//! アプリケーション状態 (DI コンテナ相当)
//!
//! 全サービスを `Arc` で保持し、axum の `State` エクストラクタで各ハンドラに注入する。

use std::sync::{Arc, Mutex};

use crate::config::Settings;
use crate::services::archive::ArchiveService;
use crate::services::node_registry::NodeRegistry;

/// アプリケーション共有状態
///
/// - `settings`: 環境変数ベースの設定 (不変)
/// - `node_registry`: `Mutex` で保護 (`register` 等が `&mut self`)
/// - `archive_service`: アーカイブ読み取り + キャッシュ (内部で thread-safe)
#[allow(dead_code, reason = "Step 10-11 で settings/archive_service を使用")]
pub(crate) struct AppState {
    pub settings: Arc<Settings>,
    pub node_registry: Arc<Mutex<NodeRegistry>>,
    pub archive_service: Arc<ArchiveService>,
}
