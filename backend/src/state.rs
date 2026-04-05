//! アプリケーション状態 (DI コンテナ相当)
//!
//! 全サービスを `Arc` で保持し、axum の `State` エクストラクタで各ハンドラに注入する。

use std::sync::{Arc, Mutex};

use crate::config::Settings;
use crate::services::archive::ArchiveService;
use crate::services::node_registry::NodeRegistry;
use crate::services::temp_file_cache::TempFileCache;
use crate::services::thumbnail_service::ThumbnailService;
use crate::services::thumbnail_warmer::ThumbnailWarmer;
use crate::services::video_converter::VideoConverter;

/// アプリケーション共有状態
///
/// - `settings`: 環境変数ベースの設定 (不変)
/// - `node_registry`: `Mutex` で保護 (`register` 等が `&mut self`)
/// - `archive_service`: アーカイブ読み取り + キャッシュ (内部で thread-safe)
/// - `temp_file_cache`: ディスク LRU キャッシュ (内部 Mutex)
/// - `thumbnail_service`: 画像リサイズ + PDF サムネイル
/// - `video_converter`: `FFmpeg` subprocess
/// - `thumbnail_warmer`: バックグラウンドプリウォーム
#[allow(
    dead_code,
    reason = "temp_file_cache と thumbnail_warmer は warm/browse 統合で使用"
)]
pub(crate) struct AppState {
    pub settings: Arc<Settings>,
    pub node_registry: Arc<Mutex<NodeRegistry>>,
    pub archive_service: Arc<ArchiveService>,
    pub temp_file_cache: Arc<TempFileCache>,
    pub thumbnail_service: Arc<ThumbnailService>,
    pub video_converter: Arc<VideoConverter>,
    pub thumbnail_warmer: Arc<ThumbnailWarmer>,
}
