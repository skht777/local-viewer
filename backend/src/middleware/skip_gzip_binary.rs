//! バイナリレスポンスの gzip 圧縮スキップミドルウェア
//!
//! `CompressionLayer` は `Content-Encoding` が設定済みのレスポンスをスキップする。
//! バイナリ `Content-Type` (画像/動画/PDF 等) に `identity` を設定して
//! 無駄な圧縮を防止する。Python 版 `_SkipGzipForBinaryMiddleware` と��一ロジック。

use axum::http::HeaderValue;
use axum::http::header;
use axum::middleware::Next;
use axum::response::Response;

/// gzip スキップ対象のバイナリ `Content-Type` プレフィックス
const BINARY_CONTENT_PREFIXES: &[&str] = &[
    "image/",
    "video/",
    "audio/",
    "application/pdf",
    "application/zip",
    "application/x-rar",
    "application/x-7z",
    "application/octet-stream",
];

/// バイナリレスポンスに `content-encoding: identity` を付与して gzip をバイパスする
#[allow(dead_code, reason = "Step 4 の main.rs 統合でレイヤー登録後に解消")]
pub(crate) async fn skip_gzip_for_binary(request: axum::extract::Request, next: Next) -> Response {
    let mut response = next.run(request).await;

    let is_binary = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| BINARY_CONTENT_PREFIXES.iter().any(|p| ct.starts_with(p)));

    if is_binary {
        response.headers_mut().insert(
            header::CONTENT_ENCODING,
            HeaderValue::from_static("identity"),
        );
    }

    response
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use axum::middleware::from_fn;
    use axum::response::IntoResponse;
    use axum::routing::get;
    use tower::ServiceExt;

    use super::*;

    async fn image_handler() -> impl IntoResponse {
        ([(header::CONTENT_TYPE, "image/jpeg")], "fake-image")
    }

    async fn json_handler() -> impl IntoResponse {
        (
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"ok":true}"#,
        )
    }

    async fn video_handler() -> impl IntoResponse {
        ([(header::CONTENT_TYPE, "video/mp4")], "fake-video")
    }

    async fn pdf_handler() -> impl IntoResponse {
        ([(header::CONTENT_TYPE, "application/pdf")], "fake-pdf")
    }

    fn test_app() -> Router {
        Router::new()
            .route("/image", get(image_handler))
            .route("/json", get(json_handler))
            .route("/video", get(video_handler))
            .route("/pdf", get(pdf_handler))
            .layer(from_fn(skip_gzip_for_binary))
    }

    #[tokio::test]
    async fn 画像レスポンスにcontent_encoding_identityが設定される() {
        let app = test_app();
        let resp = app
            .oneshot(Request::get("/image").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ce = resp.headers().get(header::CONTENT_ENCODING).unwrap();
        assert_eq!(ce, "identity");
    }

    #[tokio::test]
    async fn jsonレスポンスにcontent_encodingが設定されない() {
        let app = test_app();
        let resp = app
            .oneshot(Request::get("/json").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().get(header::CONTENT_ENCODING).is_none());
    }

    #[tokio::test]
    async fn 動画レスポンスにidentityが設定される() {
        let app = test_app();
        let resp = app
            .oneshot(Request::get("/video").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let ce = resp.headers().get(header::CONTENT_ENCODING).unwrap();
        assert_eq!(ce, "identity");
    }

    #[tokio::test]
    async fn pdfレスポンスにidentityが設定される() {
        let app = test_app();
        let resp = app
            .oneshot(Request::get("/pdf").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let ce = resp.headers().get(header::CONTENT_ENCODING).unwrap();
        assert_eq!(ce, "identity");
    }
}
