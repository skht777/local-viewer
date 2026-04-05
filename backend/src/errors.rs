//! 共通エラー型
//!
//! Python 版 `errors.py` と同一のエラーコード・レスポンス形式を使用する。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// 共通エラーレスポンスモデル
///
/// - `error`: 人間可読なエラーメッセージ
/// - `code`: 機械可読なエラーコード
/// - `detail`: 追加情報 (任意)
#[derive(Debug, Serialize)]
pub(crate) struct ErrorResponse {
    pub error: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// アプリケーションエラー
///
/// サービス層で発生するエラーを `IntoResponse` で HTTP レスポンスに変換する。
#[derive(Debug, thiserror::Error)]
pub(crate) enum AppError {
    /// パスセキュリティ違反 (traversal, symlink 等)
    #[error("{message}")]
    PathSecurity { message: String },

    /// `node_id` に対応するパスが見つからない
    #[error("node_id が見つかりません: {node_id}")]
    NodeNotFound { node_id: String },

    /// アーカイブセキュリティ違反 (zip bomb, traversal 等)
    #[error("{0}")]
    ArchiveSecurity(String),

    /// パスワード付きアーカイブ
    #[error("{0}")]
    ArchivePassword(String),

    /// ファイルが存在しない (`validate_existing` 用)
    #[error("パスが存在しません: {path}")]
    FileNotFound { path: String },

    /// 不正なカーソル (改ざん、期限切れ等)
    #[error("{0}")]
    InvalidCursor(String),

    /// ディレクトリではないパスへの browse 操作
    #[error("ディレクトリではありません: {path}")]
    NotADirectory { path: String },
}

impl AppError {
    /// パスセキュリティエラーを生成するヘルパー
    pub(crate) fn path_security(message: impl Into<String>) -> Self {
        Self::PathSecurity {
            message: message.into(),
        }
    }

    /// ノード未発見エラーを生成するヘルパー
    pub(crate) fn node_not_found(node_id: impl Into<String>) -> Self {
        Self::NodeNotFound {
            node_id: node_id.into(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::PathSecurity { .. } => (StatusCode::FORBIDDEN, "FORBIDDEN_PATH"),
            Self::NodeNotFound { .. } => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            Self::ArchiveSecurity(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "ARCHIVE_SECURITY_ERROR")
            }
            Self::ArchivePassword(_) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "ARCHIVE_PASSWORD_REQUIRED",
            ),
            Self::FileNotFound { .. } => (StatusCode::NOT_FOUND, "FILE_NOT_FOUND"),
            Self::InvalidCursor(_) => (StatusCode::BAD_REQUEST, "INVALID_CURSOR"),
            Self::NotADirectory { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "NOT_A_DIRECTORY"),
        };

        let body = ErrorResponse {
            error: self.to_string(),
            code: code.to_string(),
            detail: None,
        };

        (status, axum::Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use tower::ServiceExt;

    use super::*;

    // テスト用ハンドラ
    async fn path_security_handler() -> Result<String, AppError> {
        Err(AppError::path_security("アクセスが拒否されました"))
    }

    async fn not_found_handler() -> Result<String, AppError> {
        Err(AppError::node_not_found("abc123"))
    }

    async fn archive_security_handler() -> Result<String, AppError> {
        Err(AppError::ArchiveSecurity("zip bomb 検出".to_string()))
    }

    async fn archive_password_handler() -> Result<String, AppError> {
        Err(AppError::ArchivePassword(
            "パスワード付きアーカイブ".to_string(),
        ))
    }

    async fn file_not_found_handler() -> Result<String, AppError> {
        Err(AppError::FileNotFound {
            path: "/mnt/data/missing.txt".to_string(),
        })
    }

    async fn not_a_directory_handler() -> Result<String, AppError> {
        Err(AppError::NotADirectory {
            path: "/mnt/data/file.jpg".to_string(),
        })
    }

    async fn invalid_cursor_handler() -> Result<String, AppError> {
        Err(AppError::InvalidCursor(
            "不正なカーソルフォーマットです".to_string(),
        ))
    }

    async fn call(app: Router, uri: &str) -> (StatusCode, String) {
        let resp = app
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8(body.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn path_security_errorが403とforbidden_pathを返す() {
        let app = Router::new().route("/test", get(path_security_handler));
        let (status, body) = call(app, "/test").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(body.contains("FORBIDDEN_PATH"));
        assert!(body.contains("アクセスが拒否されました"));
    }

    #[tokio::test]
    async fn node_not_found_errorが404とnot_foundを返す() {
        let app = Router::new().route("/test", get(not_found_handler));
        let (status, body) = call(app, "/test").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains("NOT_FOUND"));
        assert!(body.contains("abc123"));
    }

    #[tokio::test]
    async fn archive_security_errorが422とarchive_security_errorを返す() {
        let app = Router::new().route("/test", get(archive_security_handler));
        let (status, body) = call(app, "/test").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body.contains("ARCHIVE_SECURITY_ERROR"));
    }

    #[tokio::test]
    async fn archive_password_errorが422とarchive_password_requiredを返す() {
        let app = Router::new().route("/test", get(archive_password_handler));
        let (status, body) = call(app, "/test").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body.contains("ARCHIVE_PASSWORD_REQUIRED"));
    }

    #[tokio::test]
    async fn file_not_found_errorが404とfile_not_foundを返す() {
        let app = Router::new().route("/test", get(file_not_found_handler));
        let (status, body) = call(app, "/test").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains("FILE_NOT_FOUND"));
    }

    #[tokio::test]
    async fn not_a_directory_errorが422とnot_a_directoryを返す() {
        let app = Router::new().route("/test", get(not_a_directory_handler));
        let (status, body) = call(app, "/test").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body.contains("NOT_A_DIRECTORY"));
        assert!(body.contains("ディレクトリではありません"));
    }

    #[tokio::test]
    async fn invalid_cursor_errorが400とinvalid_cursorを返す() {
        let app = Router::new().route("/test", get(invalid_cursor_handler));
        let (status, body) = call(app, "/test").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.contains("INVALID_CURSOR"));
    }

    #[test]
    fn error_responseのdetailがnoneの場合jsonに含まれない() {
        let resp = ErrorResponse {
            error: "test".to_string(),
            code: "TEST".to_string(),
            detail: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains("detail"));
    }

    #[test]
    fn error_responseのdetailがsomeの場合jsonに含まれる() {
        let resp = ErrorResponse {
            error: "test".to_string(),
            code: "TEST".to_string(),
            detail: Some("extra info".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("detail"));
        assert!(json.contains("extra info"));
    }
}
