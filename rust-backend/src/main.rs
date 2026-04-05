//! Local Content Viewer — Rust バックエンド エントリポイント
//!
//! ローカルディレクトリの画像・動画・PDFを閲覧する Web アプリのバックエンド。
//! Python/FastAPI 版からの移行中。

use std::net::SocketAddr;

use axum::{Json, Router, routing::get};
use clap::Parser;
use serde::Serialize;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

mod config;
mod errors;
mod middleware;
mod routers;
mod services;
mod state;

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

fn api_router() -> Router {
    Router::new().route("/api/health", get(health))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ログ初期化
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let cli = Cli::parse();

    let app = api_router().layer(TraceLayer::new_for_http());

    let addr: SocketAddr = format!("{}:{}", cli.bind, cli.port).parse()?;
    tracing::info!("サーバー起動: {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn ヘルスチェックが200を返す() {
        let app = api_router();
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
}
