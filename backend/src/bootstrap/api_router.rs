//! `/api/*` のルート登録と健康チェックハンドラ
//!
//! - `health`: liveness（常時 200）
//! - `ready`: readiness（初回スキャン完了時 200、未完了時 503）
//! - `build_api_router`: 各ルーターを 1 つの `Router` にまとめて `AppState` を注入

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;

use crate::routers;
use crate::services::node_registry::PopulateStats;
use crate::services::scan_diagnostics::ScanDiagnostics;
use crate::state::AppState;

#[derive(Serialize)]
pub(crate) struct HealthResponse {
    pub status: String,
}

/// `/api/health` に含める起動時 populate 統計
///
/// 縮退観測用: 再起動後 `node_id` deep link が正しく回復しているかを JSON で公開する
#[derive(Serialize)]
pub(crate) struct RegistryPopulateStats {
    pub registered: usize,
    pub skipped_missing_mount: usize,
    pub skipped_malformed: usize,
    pub skipped_traversal: usize,
    pub errors: usize,
    pub degraded: bool,
}

impl From<&PopulateStats> for RegistryPopulateStats {
    fn from(s: &PopulateStats) -> Self {
        Self {
            registered: s.registered,
            skipped_missing_mount: s.skipped_missing_mount,
            skipped_malformed: s.skipped_malformed,
            skipped_traversal: s.skipped_traversal,
            errors: s.errors,
            degraded: s.degraded,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct HealthResponseWithStats {
    pub status: String,
    pub registry_populate: RegistryPopulateStats,
    /// 起動時スキャン診断。完了前 / panic / `RwLock` poison 時は `None`
    pub last_scan: Option<ScanDiagnostics>,
}

/// `/api/health` — liveness + populate 統計 + 起動時スキャン診断
///
/// **liveness 契約**: `RwLock` poison を踏んでも panic せず `last_scan: null` で 200 を返す。
/// lock 内は shallow clone (`Arc::clone`) のみで即解放し、deep clone は guard 解放後に行う
pub(crate) async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponseWithStats> {
    let last_scan_arc: Option<Arc<ScanDiagnostics>> = match state.last_scan_report.read() {
        Ok(guard) => guard.as_ref().map(Arc::clone),
        Err(poisoned) => {
            tracing::error!("last_scan_report poisoned (read): {poisoned}");
            None
        }
    }; // ← ここで read guard を drop
    let last_scan = last_scan_arc.map(|arc| (*arc).clone());
    Json(HealthResponseWithStats {
        status: "ok".to_string(),
        registry_populate: RegistryPopulateStats::from(state.registry_populate_stats.as_ref()),
        last_scan,
    })
}

/// `/api/ready` — readiness プローブ
///
/// - warm start: `dir_index.is_ready()` が即 true → 200
/// - cold start: 全マウントスキャン完了後に `scan_complete` が true → 200
/// - いずれも false → 503
pub(crate) async fn ready(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let is_ready = state
        .scan_complete
        .load(std::sync::atomic::Ordering::Relaxed)
        || state.dir_index.is_ready();
    if is_ready {
        (
            StatusCode::OK,
            Json(HealthResponse {
                status: "ready".to_string(),
            }),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "warming_up".to_string(),
            }),
        )
    }
}

/// API ルーターを構築する
pub(crate) fn build_api_router(app_state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/ready", get(ready))
        .route("/api/mounts", get(routers::mounts::list_mounts))
        .route(
            "/api/mounts/reload",
            axum::routing::post(routers::mounts::reload_mounts_handler),
        )
        .route(
            "/api/browse/{node_id}",
            get(routers::browse::browse_directory),
        )
        .route(
            "/api/browse/{node_id}/first-viewable",
            get(routers::browse::first_viewable),
        )
        .route(
            "/api/browse/{parent_node_id}/sibling",
            get(routers::browse::find_sibling),
        )
        .route(
            "/api/browse/{parent_node_id}/siblings",
            get(routers::browse::find_siblings),
        )
        .route("/api/file/{node_id}", get(routers::file::serve_file))
        .route(
            "/api/thumbnail/{node_id}",
            get(routers::thumbnail::serve_thumbnail),
        )
        .route(
            "/api/thumbnails/batch",
            post(routers::thumbnail::serve_thumbnails_batch),
        )
        .route("/api/search", get(routers::search::search))
        .route("/api/index/rebuild", post(routers::search::rebuild_index))
        .with_state(app_state)
}
