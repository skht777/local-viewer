//! `POST /api/mounts/reload` — `mounts.json` hot reload ハンドラ
//!
//! 順序（Phase F）:
//! 1. `ConnectInfo<SocketAddr>` で loopback 判定 → false なら 403
//! 2. `rebuild_guard.try_acquire()` → None なら 409 + `Retry-After: 30`
//! 3. `rate_limiter::peek` → 超過未満なら 429（guard は RAII Drop）
//! 4. `load_mount_config` → 失敗なら 400
//! 5. `reload_mounts(state, config).await` → 成功なら `commit_now` + 200 JSON、失敗なら 500

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::response::{IntoResponse, Response};

use crate::errors::AppError;
use crate::services::mount_config::load_mount_config;
use crate::services::mount_hot_reload::{MountReloadResult, reload_mounts};
use crate::services::search::rebuild_rate_limiter;
use crate::state::AppState;

/// `POST /api/mounts/reload`
///
/// `mounts.json` を再読み込みし、削除された mount に対して stale cleanup と
/// `NodeRegistry` / `PathSecurity` / `FileWatcher` の同期反映を行う。
/// 追加 mount は `tracing::warn!` でスキップ（docker-compose 再起動が必要）。
pub(crate) async fn reload_mounts_handler(
    State(state): State<Arc<AppState>>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
) -> Result<Response, AppError> {
    // 1. loopback チェック（docker 越しで非 loopback 経由の呼び出しを拒否）
    if !remote.ip().is_loopback() {
        return Err(AppError::path_security(
            "`/api/mounts/reload` は loopback 専用です",
        ));
    }

    // 2. rebuild / 他 reload との全体排他 guard を取得
    let Some(_guard) = state.rebuild_guard.try_acquire() else {
        return Err(AppError::RebuildInProgress(
            "リビルド / マウントリロードが実行中です".to_string(),
        ));
    };

    // 3. レート制限の read-only チェック（last_rebuild は更新しない）
    //    失敗時は guard Drop で release、429 を返す
    rebuild_rate_limiter::peek(
        &state.last_rebuild,
        state.settings.rebuild_rate_limit_seconds,
    )
    .await?;

    // 4. mounts.json を再読み込み
    let config_path = Path::new(&state.settings.mount_config_path);
    let base_dir = state.settings.mount_base_dir.clone();
    let config_path_for_task = config_path.to_path_buf();
    let new_config =
        tokio::task::spawn_blocking(move || load_mount_config(&config_path_for_task, &base_dir))
            .await
            .map_err(|e| AppError::path_security(format!("タスク実行エラー: {e}")))??;

    // 5. 本処理: reload_mounts
    let result: MountReloadResult = reload_mounts(Arc::clone(&state), new_config).await?;

    // 成功時のみ last_rebuild を commit（失敗経路はレート制限を消費しない）
    rebuild_rate_limiter::commit_now(&state.last_rebuild).await;

    Ok(Json(result).into_response())
}

#[cfg(test)]
#[allow(
    non_snake_case,
    reason = "日本語テスト名で振る舞いを記述する規約 (07_testing.md)"
)]
mod tests {
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use axum::routing::post;
    use tempfile::TempDir;
    use tokio::time::Instant;
    use tower::ServiceExt;

    use super::*;
    use crate::config::Settings;
    use crate::services::archive::ArchiveService;
    use crate::services::dir_index::DirIndex;
    use crate::services::indexer::Indexer;
    use crate::services::node_registry::NodeRegistry;
    use crate::services::path_security::PathSecurity;
    use crate::services::rebuild_guard::RebuildGuard;
    use crate::services::temp_file_cache::TempFileCache;
    use crate::services::thumbnail_service::ThumbnailService;
    use crate::services::thumbnail_warmer::ThumbnailWarmer;
    use crate::services::video_converter::VideoConverter;

    struct Env {
        state: Arc<AppState>,
        #[allow(dead_code, reason = "TempDir を drop させないため保持")]
        base: TempDir,
        #[allow(dead_code, reason = "TempDir を drop させないため保持")]
        db_dir: TempDir,
    }

    fn setup() -> Env {
        let base = TempDir::new().unwrap();
        let base_dir = std::fs::canonicalize(base.path()).unwrap();
        let config_path = base_dir.join("mounts.json");
        std::fs::write(&config_path, r#"{"version":2,"mounts":[]}"#).unwrap();

        let settings = Settings::from_map(&HashMap::from([
            (
                "MOUNT_BASE_DIR".to_string(),
                base_dir.to_string_lossy().into_owned(),
            ),
            (
                "MOUNT_CONFIG_PATH".to_string(),
                config_path.to_string_lossy().into_owned(),
            ),
        ]))
        .unwrap();

        let ps = Arc::new(PathSecurity::new(vec![base_dir.clone()], false).unwrap());
        let registry = NodeRegistry::new(Arc::clone(&ps), 100_000, HashMap::new());
        let archive_service = Arc::new(ArchiveService::new(&settings));
        let temp_file_cache =
            Arc::new(TempFileCache::new(TempDir::new().unwrap().keep(), 10 * 1024 * 1024).unwrap());
        let thumbnail_service = Arc::new(ThumbnailService::new(Arc::clone(&temp_file_cache)));
        let video_converter =
            Arc::new(VideoConverter::new(Arc::clone(&temp_file_cache), &settings));
        let thumbnail_warmer = Arc::new(ThumbnailWarmer::new(4));
        let db_dir = TempDir::new().unwrap();
        let indexer = Arc::new(Indexer::new(
            db_dir.path().join("index.db").to_str().unwrap(),
        ));
        indexer.init_db().unwrap();
        let dir_index = Arc::new(DirIndex::new(
            db_dir.path().join("dir.db").to_str().unwrap(),
        ));
        dir_index.init_db().unwrap();

        let state = Arc::new(AppState {
            settings: Arc::new(settings),
            node_registry: Arc::new(Mutex::new(registry)),
            archive_service,
            temp_file_cache,
            thumbnail_service,
            video_converter,
            thumbnail_warmer,
            thumb_semaphore: Arc::new(tokio::sync::Semaphore::new(8)),
            archive_thumb_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
            indexer,
            dir_index,
            last_rebuild: tokio::sync::Mutex::new(None),
            scan_complete: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            registry_populate_stats: Arc::new(
                crate::services::node_registry::PopulateStats::default(),
            ),
            last_scan_report: Arc::new(std::sync::RwLock::new(None)),
            rebuild_guard: Arc::new(RebuildGuard::new()),
            file_watcher: Arc::new(Mutex::new(None)),
            path_security: ps,
            shutdown_token: tokio_util::sync::CancellationToken::new(),
            rebuild_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            rebuild_task: Arc::new(std::sync::Mutex::new(None)),
        });

        Env {
            state,
            base,
            db_dir,
        }
    }

    fn loopback_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345)
    }

    fn non_loopback_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 12345)
    }

    fn app_with_connect_info(state: Arc<AppState>, addr: SocketAddr) -> Router {
        Router::new()
            .route("/api/mounts/reload", post(reload_mounts_handler))
            .layer(axum::extract::Extension(ConnectInfo(addr)))
            .with_state(state)
    }

    async fn send_reload(app: Router, addr: SocketAddr) -> StatusCode {
        let mut req = Request::post("/api/mounts/reload");
        // axum ConnectInfo extractor 経由で渡すために Extension/ConnectInfo レイヤを併用
        req = req.extension(ConnectInfo(addr));
        let resp = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
        resp.status()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn mount_reload中にrebuildは409を返す() {
        let env = setup();
        // guard を保持して reload 実行中の状態を再現
        let _held = env
            .state
            .rebuild_guard
            .try_acquire()
            .expect("初期は未取得のため成功");
        let status = send_reload(
            app_with_connect_info(Arc::clone(&env.state), loopback_addr()),
            loopback_addr(),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn mount_reloadは非loopback_bind時に403を返す() {
        let env = setup();
        let status = send_reload(
            app_with_connect_info(Arc::clone(&env.state), non_loopback_addr()),
            non_loopback_addr(),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn mount_reloadはrebuildと同じrate_limitで429を返す() {
        let env = setup();
        // last_rebuild を直近時刻で埋める（レート制限に抵触）
        {
            let mut last = env.state.last_rebuild.lock().await;
            *last = Some(Instant::now());
        }
        let status = send_reload(
            app_with_connect_info(Arc::clone(&env.state), loopback_addr()),
            loopback_addr(),
        )
        .await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn mount_reload失敗時にrebuild_guardは解放される() {
        let env = setup();
        // mounts.json を壊して 400 を誘発
        let config_path = std::path::Path::new(&env.state.settings.mount_config_path);
        std::fs::write(config_path, "not valid json").unwrap();

        let status = send_reload(
            app_with_connect_info(Arc::clone(&env.state), loopback_addr()),
            loopback_addr(),
        )
        .await;
        // パースエラーは PathSecurity 経由のため 500 系（AppError::PathSecurity）
        assert!(!status.is_success());
        // guard は RAII で解放されているはず
        assert!(
            !env.state.rebuild_guard.is_held(),
            "失敗後は guard が解放されるべき"
        );
    }
}
