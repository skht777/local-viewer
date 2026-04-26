//! 通常ファイル配信
//!
//! - `ServeFile` (tower-http) で配信し、Range リクエスト (206) を自動処理
//! - `ETag` は Python 互換: `md5("{mtime_ns}:{size}:{name}")`
//! - MKV 等ブラウザ非対応コンテナは `VideoConverter` で MP4 に remux

use std::path::PathBuf;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use axum::http::{HeaderMap, HeaderValue, Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use md5::{Digest, Md5};
use tower::ServiceExt as _;
use tower_http::services::ServeFile;

use crate::errors::AppError;
use crate::services::extensions;
use crate::services::video_converter::VideoConverter;
use crate::state::AppState;

/// `ETag` を計算する (Python 互換)
///
/// `md5("{mtime_ns}:{size}:{name}")` でメタ情報ベースのハッシュを生成。
/// 全内容のハッシュは大きなファイルで遅いため避ける。
pub(super) fn compute_file_etag(metadata: &std::fs::Metadata, file_name: &str) -> String {
    let mtime_ns = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    let size = metadata.len();
    let raw = format!("{mtime_ns}:{size}:{file_name}");
    let digest = Md5::digest(raw.as_bytes());
    hex::encode(digest)
}

/// 通常ファイルを配信する (`ETag` + Range + `ServeFile`)
pub(super) async fn serve_regular_file(
    state: &Arc<AppState>,
    file_path: PathBuf,
    original_headers: &HeaderMap,
    original_uri: &axum::http::Uri,
) -> Result<Response, AppError> {
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
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let etag = compute_file_etag(&metadata, &file_name);
    let etag_quoted = format!("\"{etag}\"");

    // If-None-Match → 304 Not Modified
    if let Some(if_none_match) = original_headers.get(header::IF_NONE_MATCH)
        && let Ok(val) = if_none_match.to_str()
        && val.trim_matches('"') == etag
    {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [(header::ETAG, etag_quoted.clone())],
        )
            .into_response());
    }

    // MKV remux: ブラウザ非対応コンテナを MP4 に変換して配信
    let ext = extensions::extract_extension(&file_name).to_ascii_lowercase();
    let remuxed_path = if VideoConverter::needs_remux(&ext) && state.video_converter.is_available()
    {
        let vc = Arc::clone(&state.video_converter);
        let p = file_path.clone();
        let mtime_ns = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos());
        tokio::task::spawn_blocking(move || vc.get_remuxed(&p, mtime_ns))
            .await
            .map_err(|e| AppError::path_security(format!("タスク実行失敗: {e}")))?
    } else {
        None
    };
    let serve_path = remuxed_path.as_deref().unwrap_or(&file_path);

    // ServeFile で配信 (Range 自動処理)
    let mut serve_req = Request::builder()
        .uri(original_uri)
        .body(axum::body::Body::empty())
        .map_err(|e| AppError::path_security(format!("リクエスト構築失敗: {e}")))?;

    for key in [header::RANGE, header::IF_RANGE] {
        if let Some(val) = original_headers.get(&key) {
            serve_req.headers_mut().insert(key, val.clone());
        }
    }

    let mut response = ServeFile::new(serve_path)
        .oneshot(serve_req)
        .await
        .map_err(|e| AppError::path_security(format!("ファイル配信失敗: {e}")))?
        .into_response();

    // remux 成功時は Content-Type を video/mp4 に上書き
    if remuxed_path.is_some() {
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp4"));
    }

    // ETag + Cache-Control を上書き (Python 互換: 元ファイルの ETag を使用)
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
