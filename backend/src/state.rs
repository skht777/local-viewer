//! アプリケーション状態 (DI コンテナ相当)
//!
//! 全サービスを `Arc` で保持し、axum の `State` エクストラクタで各ハンドラに注入する。

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

use tokio::time::Instant;

use crate::config::Settings;
use crate::services::archive::ArchiveService;
use crate::services::dir_index::DirIndex;
use crate::services::indexer::Indexer;
use crate::services::node_registry::{NodeRegistry, PopulateStats};
use crate::services::scan_diagnostics::ScanDiagnostics;
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
/// - `thumb_semaphore`: バッチサムネイル生成の並行度制限 (非アーカイブ)
/// - `archive_thumb_semaphore`: アーカイブグループのサムネイル生成並行度制限
/// - `indexer`: FTS5 trigram 検索インデクサー
/// - `dir_index`: ディレクトリリスティングインデックス (browse 高速化)
/// - `last_rebuild`: 最後のインデックスリビルド時刻 (レート制限用)
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
    pub thumb_semaphore: Arc<tokio::sync::Semaphore>,
    pub archive_thumb_semaphore: Arc<tokio::sync::Semaphore>,
    pub indexer: Arc<Indexer>,
    pub dir_index: Arc<DirIndex>,
    pub last_rebuild: tokio::sync::Mutex<Option<Instant>>,
    /// 全マウントの初回スキャンが完了したか (cold start 時に使用)
    pub scan_complete: Arc<AtomicBool>,
    /// 起動時 `NodeRegistry` populate の結果統計
    ///
    /// - 再起動後の `node_id` deep link 回復状況を運用から確認するため `/api/health` に含める
    /// - 値は `build_app` で一度だけ設定され、以降 immutable
    pub registry_populate_stats: Arc<PopulateStats>,
    /// 起動時スキャン 1 回の診断結果
    ///
    /// - partial init (`/api/ready=503`) の原因を `/api/health` 経由で識別可能にする
    /// - `scan_handle` が完了時に `Some(..)` を書き込む。完了前 / panic 時は `None`
    /// - `/api/health` は read/write 両側で poison を `tracing::error!` + fallback し、
    ///   liveness 契約を守る
    pub last_scan_report: Arc<RwLock<Option<Arc<ScanDiagnostics>>>>,
}
