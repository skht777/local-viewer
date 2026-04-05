//! ファイル配信 API
//!
//! `GET /api/file/{node_id}` — ファイル配信 (Range 対応, ETag/Cache-Control 付き)
//!
//! - `ServeFile` (tower-http) で配信し、Range リクエスト (206) を自動処理
//! - `ETag` は Python 互換: `md5("{mtime_ns}:{size}:{name}")`
//! - アーカイブエントリは Phase 4、MKV remux は Phase 5 で対応

use std::path::PathBuf;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use axum::extract::{Path, State};
use axum::http::header;
use axum::http::{HeaderValue, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use md5::{Digest, Md5};
use tower::ServiceExt as _;
use tower_http::services::ServeFile;

use crate::errors::AppError;
use crate::state::AppState;

/// `ETag` を計算する (Python 互換)
///
/// `md5("{mtime_ns}:{size}:{name}")` でメタ情報ベースのハッシュを生成。
/// 全内容のハッシュは大きなファイルで遅いため避ける。
fn compute_file_etag(metadata: &std::fs::Metadata, file_name: &str) -> String {
    let mtime_ns = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    let size = metadata.len();
    let raw = format!("{mtime_ns}:{size}:{file_name}");
    let digest = Md5::digest(raw.as_bytes());
    format!("{digest:x}")
}

/// ファイルまたはアーカイブエントリを配信する
///
/// - 通常ファイル: `ServeFile` で配信 (Range 自動処理)
/// - アーカイブエントリ: Phase 4 で実装 (現在 501)
/// - ディレクトリ: 422 `NOT_A_FILE`
pub(crate) async fn serve_file(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
    req: Request<axum::body::Body>,
) -> Result<Response, AppError> {
    let registry = Arc::clone(&state.node_registry);
    let original_headers = req.headers().clone();
    let original_uri = req.uri().clone();

    // spawn_blocking 内で node_id 解決 + メタデータ取得
    let (file_path, is_archive) =
        tokio::task::spawn_blocking(move || -> Result<(PathBuf, bool), AppError> {
            let mut reg = registry
                .lock()
                .map_err(|e| AppError::path_security(format!("ロック取得失敗: {e}")))?;

            // アーカイブエントリかチェック
            if reg.resolve_archive_entry(&node_id).is_some() {
                return Ok((PathBuf::new(), true));
            }

            let path = reg.resolve(&node_id)?.to_path_buf();
            Ok((path, false))
        })
        .await
        .map_err(|e| AppError::path_security(format!("タスク実行失敗: {e}")))??;

    // アーカイブエントリは Phase 4 で実装
    if is_archive {
        return Ok(StatusCode::NOT_IMPLEMENTED.into_response());
    }

    // ディレクトリチェック
    if file_path.is_dir() {
        return Err(AppError::NotAFile {
            path: file_path.display().to_string(),
        });
    }

    // ファイル存在チェック
    if !file_path.exists() {
        return Err(AppError::FileNotFound {
            path: file_path.display().to_string(),
        });
    }

    // ETag 計算
    let metadata = file_path.metadata().map_err(|_| AppError::FileNotFound {
        path: file_path.display().to_string(),
    })?;
    let file_name = file_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let etag = compute_file_etag(&metadata, &file_name);
    let etag_quoted = format!("\"{etag}\"");

    // If-None-Match → 304 Not Modified
    if let Some(if_none_match) = original_headers.get(header::IF_NONE_MATCH) {
        if let Ok(val) = if_none_match.to_str() {
            if val.trim_matches('"') == etag {
                return Ok((
                    StatusCode::NOT_MODIFIED,
                    [(header::ETAG, etag_quoted.clone())],
                )
                    .into_response());
            }
        }
    }

    // ServeFile で配信 (Range 自動処理)
    // 元リクエストの Range/If-Range ヘッダを転送する
    let mut serve_req = Request::builder()
        .uri(&original_uri)
        .body(axum::body::Body::empty())
        .map_err(|e| AppError::path_security(format!("リクエスト構築失敗: {e}")))?;

    for key in [header::RANGE, header::IF_RANGE] {
        if let Some(val) = original_headers.get(&key) {
            serve_req.headers_mut().insert(key, val.clone());
        }
    }

    let mut response = ServeFile::new(&file_path)
        .oneshot(serve_req)
        .await
        .map_err(|e| AppError::path_security(format!("ファイル配信失敗: {e}")))?
        .into_response();

    // ETag + Cache-Control を上書き (ServeFile のデフォルトを置き換え)
    let headers = response.headers_mut();
    headers.insert(
        header::ETAG,
        HeaderValue::from_str(&etag_quoted)
            .map_err(|e| AppError::path_security(format!("ETag ヘッダ設定失敗: {e}")))?,
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=3600"),
    );

    Ok(response)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use axum::routing::get;
    use axum::{Router, body};
    use tower::ServiceExt;

    use super::*;
    use crate::config::Settings;
    use crate::services::node_registry::NodeRegistry;
    use crate::services::path_security::PathSecurity;

    /// `AppState` の `NodeRegistry` にファイルを登録して `node_id` を取得するヘルパー
    fn register_file(state: &Arc<AppState>, path: &std::path::Path) -> String {
        let mut reg = state.node_registry.lock().unwrap();
        reg.register(path).unwrap()
    }

    /// テストセットアップ: Router + state + temp dir
    fn full_setup() -> (Router, Arc<AppState>, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();

        let settings = Settings::from_map(&HashMap::from([(
            "MOUNT_BASE_DIR".to_string(),
            root.to_string_lossy().to_string(),
        )]))
        .unwrap();

        let ps = Arc::new(PathSecurity::new(vec![root], false).unwrap());
        let registry = NodeRegistry::new(ps, 100_000, HashMap::new());
        let archive_service = Arc::new(crate::services::archive::ArchiveService::new(&settings));

        let app_state = Arc::new(AppState {
            settings: Arc::new(settings),
            node_registry: Arc::new(Mutex::new(registry)),
            archive_service,
        });

        let app = Router::new()
            .route("/api/file/{node_id}", get(serve_file))
            .with_state(Arc::clone(&app_state));

        (app, app_state, dir)
    }

    #[tokio::test]
    async fn ファイルのnode_idで200を返す() {
        let (app, state, dir) = full_setup();
        let file_path = dir.path().join("test.jpg");
        std::fs::write(&file_path, b"fake-jpeg-data").unwrap();
        let node_id = register_file(&state, &file_path);

        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{node_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn 正しいcontent_typeを返す() {
        let (app, state, dir) = full_setup();
        let file_path = dir.path().join("photo.png");
        std::fs::write(&file_path, b"fake-png").unwrap();
        let node_id = register_file(&state, &file_path);

        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{node_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let ct = resp.headers().get(header::CONTENT_TYPE).unwrap();
        assert!(ct.to_str().unwrap().contains("image/png"));
    }

    #[tokio::test]
    async fn ディレクトリのnode_idで422を返す() {
        let (app, state, dir) = full_setup();
        let sub = dir.path().join("subdir");
        std::fs::create_dir(&sub).unwrap();
        let node_id = register_file(&state, &sub);

        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{node_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body_bytes = body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(body_bytes.to_vec()).unwrap();
        assert!(body.contains("NOT_A_FILE"));
    }

    #[tokio::test]
    async fn 存在しないnode_idで404を返す() {
        let (app, _state, _dir) = full_setup();
        let resp = app
            .oneshot(
                Request::get("/api/file/nonexistent123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn 登録済みだがファイル削除済みで404を返す() {
        let (app, state, dir) = full_setup();
        let file_path = dir.path().join("will_delete.txt");
        std::fs::write(&file_path, b"temporary").unwrap();
        let node_id = register_file(&state, &file_path);
        std::fs::remove_file(&file_path).unwrap();

        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{node_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn etagヘッダが含まれる() {
        let (app, state, dir) = full_setup();
        let file_path = dir.path().join("etag_test.jpg");
        std::fs::write(&file_path, b"etag-data").unwrap();
        let node_id = register_file(&state, &file_path);

        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{node_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let etag = resp.headers().get(header::ETAG).unwrap();
        let etag_str = etag.to_str().unwrap();
        // ETag はダブルクォートで囲まれている
        assert!(etag_str.starts_with('"'));
        assert!(etag_str.ends_with('"'));
        // 中身は32文字の hex (MD5)
        let inner = &etag_str[1..etag_str.len() - 1];
        assert_eq!(inner.len(), 32);
    }

    #[tokio::test]
    async fn if_none_match一致で304を返す() {
        let (app, state, dir) = full_setup();
        let file_path = dir.path().join("cached.jpg");
        std::fs::write(&file_path, b"cached-data").unwrap();
        let node_id = register_file(&state, &file_path);

        // まず ETag を取得
        let resp = app
            .clone()
            .oneshot(
                Request::get(format!("/api/file/{node_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let etag = resp.headers().get(header::ETAG).unwrap().clone();

        // If-None-Match で再リクエスト → 304
        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{node_id}"))
                    .header(header::IF_NONE_MATCH, etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
    }

    #[tokio::test]
    async fn cache_controlがprivate_max_age_3600() {
        let (app, state, dir) = full_setup();
        let file_path = dir.path().join("cache_test.txt");
        std::fs::write(&file_path, b"cache").unwrap();
        let node_id = register_file(&state, &file_path);

        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{node_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let cc = resp
            .headers()
            .get(header::CACHE_CONTROL)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(cc, "private, max-age=3600");
    }

    #[tokio::test]
    async fn rangeリクエストで206を返す() {
        let (app, state, dir) = full_setup();
        let file_path = dir.path().join("range_test.bin");
        std::fs::write(&file_path, b"0123456789abcdef").unwrap();
        let node_id = register_file(&state, &file_path);

        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{node_id}"))
                    .header(header::RANGE, "bytes=0-3")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);

        let content_range = resp.headers().get(header::CONTENT_RANGE).unwrap();
        assert!(content_range.to_str().unwrap().contains("bytes 0-3/16"));

        let body_bytes = body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body_bytes[..], b"0123");
    }

    #[tokio::test]
    async fn 拡張子なしファイルでoctet_streamを返す() {
        let (app, state, dir) = full_setup();
        let file_path = dir.path().join("no_extension");
        std::fs::write(&file_path, b"binary-data").unwrap();
        let node_id = register_file(&state, &file_path);

        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{node_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            ct.contains("octet-stream"),
            "拡張子なしは octet-stream が期待されるが、{ct} が返された"
        );
    }

    #[tokio::test]
    async fn 自前etagがservefileの内部etagを上書きする() {
        let (app, state, dir) = full_setup();
        let file_path = dir.path().join("etag_override.txt");
        std::fs::write(&file_path, b"content").unwrap();
        let node_id = register_file(&state, &file_path);

        // 自前 ETag を計算
        let meta = std::fs::metadata(&file_path).unwrap();
        let expected_etag = super::compute_file_etag(&meta, "etag_override.txt");

        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{node_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let actual_etag = resp.headers().get(header::ETAG).unwrap().to_str().unwrap();
        // 自前 ETag (Python 互換) が使われていること
        assert_eq!(actual_etag, format!("\"{expected_etag}\""));
    }
}
