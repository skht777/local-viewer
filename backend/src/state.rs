//! アプリケーション状態 (DI コンテナ相当)
//!
//! 全サービスを `Arc` で保持し、axum の `State` エクストラクタで各ハンドラに注入する。

use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex, RwLock};

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::config::Settings;
use crate::services::archive::ArchiveService;
use crate::services::dir_index::DirIndex;
use crate::services::file_watcher::FileWatcher;
use crate::services::indexer::Indexer;
use crate::services::node_registry::{NodeRegistry, PopulateStats};
use crate::services::path_security::PathSecurity;
use crate::services::rebuild_guard::RebuildGuard;
use crate::services::rebuild_task::RebuildTaskHandle;
use crate::services::scan_diagnostics::ScanDiagnostics;
use crate::services::temp_file_cache::TempFileCache;
use crate::services::thumbnail_service::ThumbnailService;
use crate::services::thumbnail_warmer::ThumbnailWarmer;
use crate::services::video_converter::VideoConverter;

/// graceful shutdown 関連の共有フィールド
///
/// - `token`: `main` が SIGINT / SIGTERM で `cancel()` を呼ぶ協調キャンセルトークン。
///   scan / rebuild / `mount_hot_reload` / `parallel_walk` / `batch_insert` の各経路が
///   `is_cancelled()` を見て早期 return する。`FileWatcher` late-install 抑止にも使用
/// - `rebuild_generation`: rebuild slot 登録時に `fetch_add(1, SeqCst)` で採番。
///   wrapper task が自 slot かを判別するため
/// - `rebuild_task`: 実行中 rebuild の追跡ハンドル。shutdown 時は
///   `drain_long_tasks` が `join.lock().take()` で `JoinHandle` を奪って await
///
/// shutdown 系フィールドを追加する場合は本 struct に追加し `fresh()` を更新するだけでよい。
/// 14 箇所の `AppState { .. }` リテラルはそのまま `shutdown: ShutdownFields::fresh()` を
/// 保持し続けるため、追従コストが生じない。
pub(crate) struct ShutdownFields {
    pub token: CancellationToken,
    pub rebuild_generation: Arc<AtomicU64>,
    pub rebuild_task: Arc<Mutex<Option<Arc<RebuildTaskHandle>>>>,
}

impl ShutdownFields {
    /// 全フィールド未発火・未登録で初期化する（bootstrap / テスト共通）
    pub(crate) fn fresh() -> Self {
        Self {
            token: CancellationToken::new(),
            rebuild_generation: Arc::new(AtomicU64::new(0)),
            rebuild_task: Arc::new(Mutex::new(None)),
        }
    }
}

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
    /// rebuild / hot reload 全体排他 guard
    ///
    /// - `POST /api/index/rebuild` と `POST /api/mounts/reload` の同時実行を防ぐ
    /// - `try_acquire` で 1 者のみ成功、RAII ハンドル Drop で自動解放（panic 安全）
    /// - `FileWatcher` flush 抑止の判定にも使用（`is_held()`）
    pub rebuild_guard: Arc<RebuildGuard>,
    /// 起動後の `FileWatcher` インスタンス所有権
    ///
    /// - スキャン完了後に `Some(..)` が書き込まれる（`bootstrap/background_tasks.rs`）
    /// - hot reload は `take()` → `stop()` → 新 `FileWatcher` を `replace()` で差し替え
    /// - `Drop` はアプリ終了時 / hot reload 時のみ発火。通常動作中は起動時 leak 相当
    ///   （旧実装は `std::mem::forget` で同じ寿命を実現していたが、AppState に置くことで
    ///   hot reload からの lifecycle 操作を可能にする）
    pub file_watcher: Arc<Mutex<Option<FileWatcher>>>,
    /// パス検証サービス（`NodeRegistry` / `FileWatcher` と同一 `Arc` を共有）
    ///
    /// 内部の `roots` / `root_entries` は `RwLock` で保護されており、hot reload 時に
    /// `replace_roots` で atomic に差し替えられる。本フィールドは hot reload
    /// サービスが直接参照するために保持する（NodeRegistry の Mutex を経由せず
    /// 読み書きできる）
    pub path_security: Arc<PathSecurity>,
    /// graceful shutdown 関連の 3 フィールドをネスト化した struct
    ///
    /// `shutdown.token` / `shutdown.rebuild_generation` / `shutdown.rebuild_task` で参照する。
    /// 個別初期化のコピペを避けるため `ShutdownFields::fresh()` を使う
    pub shutdown: ShutdownFields,
}

#[cfg(test)]
#[allow(
    non_snake_case,
    reason = "日本語テスト名の可読性のため ShutdownFields を PascalCase 残存"
)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn ShutdownFields_freshはcancelされていないtokenを返す() {
        let shutdown = ShutdownFields::fresh();
        assert!(!shutdown.token.is_cancelled());
    }

    #[test]
    fn ShutdownFields_freshはgenerationが0で始まる() {
        let shutdown = ShutdownFields::fresh();
        assert_eq!(shutdown.rebuild_generation.load(Ordering::Acquire), 0);
    }

    #[test]
    fn ShutdownFields_freshはrebuild_taskがNoneで始まる() {
        let shutdown = ShutdownFields::fresh();
        let slot = shutdown
            .rebuild_task
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(slot.is_none());
    }
}
