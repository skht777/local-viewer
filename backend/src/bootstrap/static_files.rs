//! 静的ファイル配信 + SPA フォールバック
//!
//! Docker 本番環境 (`static/` が存在する場合) のみ有効。
//! 開発時は Vite dev server がフロントエンドを処理する。

use axum::Router;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

/// 静的ファイル配信 + SPA フォールバックを追加する
pub(crate) fn attach_static_files(router: Router) -> Router {
    let static_dir = std::env::current_dir().unwrap_or_default().join("static");

    if !static_dir.exists() {
        return router;
    }

    tracing::info!("静的ファイル配信有効: {}", static_dir.display());

    // /assets/* — Vite ハッシュ付きアセット (immutable 長期キャッシュ)
    let assets_service = tower::ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("public, max-age=31536000, immutable"),
        ))
        .service(ServeDir::new(static_dir.join("assets")));

    // SPA フォールバック: non-API パスは index.html を返す
    let spa_fallback =
        ServeDir::new(&static_dir).not_found_service(ServeFile::new(static_dir.join("index.html")));

    router
        .nest_service("/assets", assets_service)
        .fallback_service(spa_fallback)
}
