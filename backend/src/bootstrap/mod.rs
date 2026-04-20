//! アプリケーション起動時のサービス構築とルーター組み立て
//!
//! - `state`: マウント設定読み込み / サービス初期化 / `AppState` 構築 / `NodeRegistry` rehydrate
//! - `api_router`: `/api/*` のルート登録 (`health`, `ready`, browse/file/thumbnail/search)
//! - `http_layers`: CORS + ミドルウェア
//! - `static_files`: 本番環境の SPA 静的配信
//! - `background_tasks`: ウォームスタート判定 + スキャン + `FileWatcher`

pub(crate) mod api_router;
pub(crate) mod background_tasks;
pub(crate) mod http_layers;
pub(crate) mod state;
pub(crate) mod static_files;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use axum::Router;

use crate::config::Settings;
use crate::services::dir_index::DirIndex;
use crate::services::file_watcher::FileWatcher;
use crate::services::indexer::Indexer;
use crate::services::path_security::PathSecurity;
use crate::services::rebuild_guard::RebuildGuard;
use crate::services::scan_diagnostics::ScanDiagnostics;

#[cfg(test)]
pub(crate) use api_router::{health, ready};

/// バックグラウンドタスク (ウォームスタート判定 + インデックススキャン) のコンテキスト
pub(crate) struct BackgroundContext {
    pub indexer: Arc<Indexer>,
    pub dir_index: Arc<DirIndex>,
    pub path_security: Arc<PathSecurity>,
    pub mount_id_map: HashMap<String, PathBuf>,
    pub scan_workers: usize,
    pub scan_complete: Arc<std::sync::atomic::AtomicBool>,
    /// `AppState.last_scan_report` と同じ `Arc` を共有。scan 完了時に書き込む
    pub last_scan_report: Arc<RwLock<Option<Arc<ScanDiagnostics>>>>,
    /// `AppState.rebuild_guard` と同じ `Arc` を共有。`FileWatcher` の flush 延期判定に使用
    pub rebuild_guard: Arc<RebuildGuard>,
    /// `AppState.file_watcher` と同じ `Arc` を共有。scan 完了時に `FileWatcher` を保持する slot
    pub file_watcher: Arc<std::sync::Mutex<Option<FileWatcher>>>,
}

/// サービス初期化 + ルーター構築
///
/// 初期化順序:
/// 1. `Settings::new()` — 環境変数パース (呼び出し元で実行済み)
/// 2. `state::build_state` — マウント解決 / サービス初期化 / `AppState` / populate
/// 3. `api_router::build_api_router` — `/api/*` ルート登録
/// 4. `static_files::attach_static_files` — SPA フォールバック
/// 5. `http_layers::apply_http_layers` — CORS + ミドルウェア
pub(crate) fn build_app(settings: Settings) -> anyhow::Result<(Router, BackgroundContext)> {
    let (app_state, bg_context) = state::build_state(settings)?;
    let api = api_router::build_api_router(app_state);
    let app = static_files::attach_static_files(api);
    let app = http_layers::apply_http_layers(app);
    Ok((app, bg_context))
}
