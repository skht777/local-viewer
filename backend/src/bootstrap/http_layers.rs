//! HTTP ミドルウェア層の適用
//!
//! レスポンスパス: Handler → CORS → `SkipGzipBinary` → Compression → Trace

use axum::Router;
use axum::http::Method;
use axum::middleware::from_fn;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::middleware;

/// CORS + 圧縮 + トレーシングを追加する
pub(crate) fn apply_http_layers(router: Router) -> Router {
    // CORS: 開発用ポートを許可
    #[allow(clippy::expect_used, reason = "定数文字列のパースは失敗しない")]
    let cors = CorsLayer::new()
        .allow_origin([
            "http://localhost:5173".parse().expect("valid origin"),
            "http://localhost:5174".parse().expect("valid origin"),
        ])
        .allow_methods([Method::GET])
        .allow_headers(Any);

    router
        .layer(cors)
        .layer(from_fn(middleware::skip_gzip_binary::skip_gzip_for_binary))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}
