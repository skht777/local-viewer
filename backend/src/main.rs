//! Local Content Viewer — Rust バックエンド エントリポイント
//!
//! ローカルディレクトリの画像・動画・PDFを閲覧する Web アプリのバックエンド。
//! Python/FastAPI 版からの移行中。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::http::Method;
use axum::{Json, Router, routing::get};
use clap::Parser;
use serde::Serialize;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

mod config;
mod errors;
mod middleware;
mod routers;
mod services;
mod state;

use config::Settings;
use services::mount_config::load_mount_config;
use services::node_registry::NodeRegistry;
use services::path_security::PathSecurity;
use state::AppState;

/// CLI 引数
#[derive(Parser, Debug)]
#[command(name = "local-viewer-backend", about = "Local Content Viewer backend")]
struct Cli {
    /// バインドポート
    #[arg(long, default_value = "8000")]
    port: u16,

    /// バインドアドレス
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

/// サービス初期化 + ルーター構築
///
/// 初期化順序:
/// 1. `Settings::new()` — 環境変数パース
/// 2. `load_mount_config()` — mounts.json 読み込み
/// 3. `MountPoint::resolve_path()` — `root_dirs` 構築
/// 4. `PathSecurity::new()` — パストラバーサル防止
/// 5. `NodeRegistry::new()` — HMAC `node_id` マッピング
/// 6. `AppState` 構築
/// 7. ルーター + ミドルウェア登録
fn build_app(settings: Settings) -> anyhow::Result<Router> {
    // マウントポイント設定読み込み
    let config_path = PathBuf::from(&settings.mount_config_path);
    let config = load_mount_config(&config_path, &settings.mount_base_dir)
        .map_err(|e| anyhow::anyhow!("マウント設定の読み込みに失敗: {e}"))?;

    // root_dirs と mount_names を構築
    let mut root_dirs = Vec::new();
    let mut mount_names: HashMap<PathBuf, String> = HashMap::new();

    if config.mounts.is_empty() {
        // マウント定義なし → base_dir 自体をルートとして使用
        let base_resolved = std::fs::canonicalize(&settings.mount_base_dir)
            .unwrap_or_else(|_| settings.mount_base_dir.clone());
        tracing::warn!(
            "マウント定義がありません。base_dir をルートとして使用: {}",
            base_resolved.display()
        );
        root_dirs.push(base_resolved);
    } else {
        for mp in &config.mounts {
            match mp.resolve_path(&settings.mount_base_dir) {
                Ok(resolved) => {
                    mount_names.insert(resolved.clone(), mp.name.clone());
                    root_dirs.push(resolved);
                }
                Err(e) => {
                    tracing::warn!(
                        "マウントポイント '{}' (slug={}) の解決に失敗: {e}",
                        mp.name,
                        mp.slug
                    );
                }
            }
        }
    }

    if root_dirs.is_empty() {
        anyhow::bail!("有効なマウントポイントがありません");
    }

    tracing::info!("マウントポイント: {} 件 ({:?})", root_dirs.len(), root_dirs);

    // サービス初期化
    let path_security = Arc::new(PathSecurity::new(root_dirs, settings.is_allow_symlinks)?);
    let registry = NodeRegistry::new(
        Arc::clone(&path_security),
        settings.archive_registry_max_entries,
        mount_names,
    );

    let app_state = Arc::new(AppState {
        settings: Arc::new(settings),
        node_registry: Arc::new(Mutex::new(registry)),
    });

    // CORS: 開発用ポートを許可
    #[allow(clippy::expect_used, reason = "定数文字列のパースは失敗しない")]
    let cors = CorsLayer::new()
        .allow_origin([
            "http://localhost:5173".parse().expect("valid origin"),
            "http://localhost:5174".parse().expect("valid origin"),
        ])
        .allow_methods([Method::GET])
        .allow_headers(Any);

    // ルーター構築
    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/mounts", get(routers::mounts::list_mounts))
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
        .with_state(app_state)
        .layer(cors)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    Ok(app)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ログ初期化
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let cli = Cli::parse();

    let settings = Settings::new().map_err(|e| anyhow::anyhow!("設定エラー: {e}"))?;
    let app = build_app(settings)?;

    let addr: SocketAddr = format!("{}:{}", cli.bind, cli.port).parse()?;
    tracing::info!("サーバー起動: {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::*;

    fn test_app() -> Router {
        let dir = tempfile::TempDir::new().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        // TempDir を leak して test 中に消えないようにする
        std::mem::forget(dir);

        let settings = Settings::from_map(&HashMap::from([(
            "MOUNT_BASE_DIR".to_string(),
            root.to_string_lossy().to_string(),
        )]))
        .unwrap();

        let ps = Arc::new(PathSecurity::new(vec![root], false).unwrap());
        let registry = NodeRegistry::new(ps, 100_000, HashMap::new());

        let app_state = Arc::new(AppState {
            settings: Arc::new(settings),
            node_registry: Arc::new(Mutex::new(registry)),
        });

        Router::new()
            .route("/api/health", get(health))
            .route("/api/mounts", get(routers::mounts::list_mounts))
            .with_state(app_state)
    }

    #[tokio::test]
    async fn ヘルスチェックが200を返す() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn マウント一覧が200を返す() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/mounts")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
