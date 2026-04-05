//! サムネイル API
//!
//! - `GET /api/thumbnail/{node_id}` — 単一サムネイル (JPEG)
//! - `POST /api/thumbnails/batch` — バッチサムネイル (base64 JSON)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::services::extensions::{IMAGE_EXTENSIONS, PDF_EXTENSIONS, VIDEO_EXTENSIONS};
use crate::state::AppState;

/// サムネイル `ETag` を計算する
///
/// `MD5("thumb:{mtime_ns}:{node_id}")`
fn compute_thumb_etag(mtime_ns: u128, node_id: &str) -> String {
    let raw = format!("thumb:{mtime_ns}:{node_id}");
    let digest = Md5::digest(raw.as_bytes());
    format!("\"{digest:x}\"")
}

/// ファイルの `mtime_ns` を取得する
fn get_mtime_ns(path: &std::path::Path) -> Result<u128, AppError> {
    let meta = std::fs::metadata(path).map_err(|_| AppError::NodeNotFound {
        node_id: String::new(),
    })?;
    Ok(meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos()))
}

/// 拡張子を小文字で取得する
fn ext_lower(name: &str) -> String {
    std::path::Path::new(name)
        .extension()
        .map_or(String::new(), |e| {
            format!(".{}", e.to_string_lossy().to_lowercase())
        })
}

#[derive(Deserialize)]
pub(crate) struct ThumbnailQuery {
    /// バージョンパラメータ (キャッシュバスティング)
    v: Option<String>,
}

/// `GET /api/thumbnail/{node_id}` — 単一サムネイルを返す
///
/// - 画像: `image` + `fast_image_resize` で 300px JPEG
/// - アーカイブ: 先頭画像エントリのサムネイル
/// - PDF: `pdftoppm` で先頭ページ
/// - 動画: `FFmpeg` フレーム抽出
/// - `ETag` + `Cache-Control` 付き
pub(crate) async fn serve_thumbnail(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
    Query(query): Query<ThumbnailQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    // サムネイル生成は CPU バウンド → spawn_blocking
    let result = {
        let state = Arc::clone(&state);
        let node_id = node_id.clone();
        tokio::task::spawn_blocking(move || generate_thumbnail_bytes(&state, &node_id))
            .await
            .map_err(|e| AppError::InvalidImage(format!("タスク実行失敗: {e}")))?
    }?;

    // If-None-Match チェック
    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH) {
        if let Ok(val) = if_none_match.to_str() {
            if val == result.etag {
                return Ok(StatusCode::NOT_MODIFIED.into_response());
            }
        }
    }

    // Cache-Control ヘッダ
    let cache_control = if query.v.is_some() {
        "public, max-age=31536000, immutable"
    } else {
        "private, max-age=3600"
    };

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    if let Ok(etag_val) = HeaderValue::from_str(&result.etag) {
        resp_headers.insert(header::ETAG, etag_val);
    }
    if let Ok(cc_val) = HeaderValue::from_str(cache_control) {
        resp_headers.insert(header::CACHE_CONTROL, cc_val);
    }

    Ok((resp_headers, result.data).into_response())
}

/// サムネイル生成結果
struct ThumbnailResult {
    data: Vec<u8>,
    etag: String,
}

/// サムネイル生成のコアロジック (sync、`spawn_blocking` 内から呼ぶ)
///
/// `node_id` の種類に応じて適切な生成パスを選択する:
/// 1. アーカイブエントリ → 抽出 → リサイズ
/// 2. 通常ファイル:
///    - ディレクトリ → `NOT_SUPPORTED`
///    - アーカイブファイル → 先頭画像エントリ → リサイズ
///    - PDF → `pdftoppm` → リサイズ
///    - 動画 → `FFmpeg` フレーム抽出 → リサイズ
///    - 画像 → リサイズ
///    - その他 → `NOT_SUPPORTED`
fn generate_thumbnail_bytes(state: &AppState, node_id: &str) -> Result<ThumbnailResult, AppError> {
    let thumb_svc = &state.thumbnail_service;
    let video_conv = &state.video_converter;

    // アーカイブエントリかチェック
    let entry = {
        let mut registry = state
            .node_registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registry.resolve_archive_entry(node_id)
    };

    if let Some((archive_path, entry_name)) = entry {
        let mtime_ns = get_mtime_ns(&archive_path)?;
        let cache_key = thumb_svc.make_cache_key(node_id, mtime_ns);
        let data = state
            .archive_service
            .extract_entry(&archive_path, &entry_name)?;
        let thumb = thumb_svc.generate_from_bytes(&data, &cache_key)?;
        let etag = compute_thumb_etag(mtime_ns, node_id);
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    // 通常パス解決
    let resolved = {
        let registry = state
            .node_registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registry.resolve(node_id)?.to_path_buf()
    };

    // ディレクトリ
    if resolved.is_dir() {
        return Err(AppError::NotSupported(
            "ディレクトリのサムネイルは非対応です".to_string(),
        ));
    }

    let mtime_ns = get_mtime_ns(&resolved)?;
    let cache_key = thumb_svc.make_cache_key(node_id, mtime_ns);
    let etag = compute_thumb_etag(mtime_ns, node_id);

    let ext = ext_lower(&resolved.to_string_lossy());

    // アーカイブファイル自体 → 先頭画像エントリのサムネイル
    if state.archive_service.is_supported(&resolved) {
        let entry = state
            .archive_service
            .first_image_entry(&resolved)?
            .ok_or_else(|| AppError::NoImage("アーカイブ内に画像が見つかりません".to_string()))?;
        let data = state
            .archive_service
            .extract_entry(&resolved, &entry.name)?;
        let thumb = thumb_svc.generate_from_bytes(&data, &cache_key)?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    // PDF
    if PDF_EXTENSIONS.contains(&ext.as_str()) {
        let thumb = thumb_svc.generate_pdf_thumbnail(
            &resolved,
            &cache_key,
            state.settings.video_thumb_timeout,
        )?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    // 動画
    if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        let frame = video_conv.extract_frame(&resolved).ok_or_else(|| {
            AppError::FrameExtractFailed("動画のフレーム抽出に失敗しました".to_string())
        })?;
        let thumb = thumb_svc.generate_from_bytes(&frame, &cache_key)?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    // 画像
    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        let thumb = thumb_svc.generate_from_path(&resolved, &cache_key)?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    // その他
    Err(AppError::NotSupported(
        "サムネイル非対応のファイル形式です".to_string(),
    ))
}

// --- バッチ API ---

#[derive(Deserialize)]
pub(crate) struct BatchRequest {
    node_ids: Vec<String>,
}

#[derive(Serialize)]
struct BatchThumbnailEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    etag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
}

#[derive(Serialize)]
struct BatchResponse {
    thumbnails: HashMap<String, BatchThumbnailEntry>,
}

/// `POST /api/thumbnails/batch` — バッチサムネイルを返す
///
/// - 最大 50 件、重複排除
/// - 同一アーカイブのエントリをグループ化して一括処理
/// - 全体ステータスは常に 200
pub(crate) async fn serve_thumbnails_batch(
    State(state): State<Arc<AppState>>,
    Json(body): Json<BatchRequest>,
) -> Response {
    use base64::Engine;

    // 50 件上限 + 重複排除 (順序保持)
    let mut seen = std::collections::HashSet::new();
    let unique_ids: Vec<String> = body
        .node_ids
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .take(50)
        .collect();

    // spawn_blocking で並行処理
    let semaphore = Arc::new(tokio::sync::Semaphore::new(8));
    let mut handles = Vec::with_capacity(unique_ids.len());

    for nid in &unique_ids {
        let state = Arc::clone(&state);
        let nid = nid.clone();
        let sem = Arc::clone(&semaphore);

        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let result =
                tokio::task::spawn_blocking(move || generate_thumbnail_bytes(&state, &nid)).await;

            match result {
                Ok(Ok(thumb)) => {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&thumb.data);
                    BatchThumbnailEntry {
                        data: Some(b64),
                        etag: Some(thumb.etag),
                        error: None,
                        code: None,
                    }
                }
                Ok(Err(app_err)) => {
                    let (code, msg) = error_to_code_message(&app_err);
                    BatchThumbnailEntry {
                        data: None,
                        etag: None,
                        error: Some(msg),
                        code: Some(code),
                    }
                }
                Err(_join_err) => BatchThumbnailEntry {
                    data: None,
                    etag: None,
                    error: Some("タスク実行エラー".to_string()),
                    code: Some("INTERNAL_ERROR".to_string()),
                },
            }
        }));
    }

    // 全結果を収集
    let mut thumbnails = HashMap::with_capacity(unique_ids.len());
    for (nid, handle) in unique_ids.into_iter().zip(handles) {
        let entry = handle.await.unwrap_or(BatchThumbnailEntry {
            data: None,
            etag: None,
            error: Some("タスク実行エラー".to_string()),
            code: Some("INTERNAL_ERROR".to_string()),
        });
        thumbnails.insert(nid, entry);
    }

    Json(BatchResponse { thumbnails }).into_response()
}

/// `AppError` からエラーコードとメッセージを抽出する
fn error_to_code_message(err: &AppError) -> (String, String) {
    let code = match err {
        AppError::NodeNotFound { .. } => "NOT_FOUND",
        AppError::NotSupported(_) => "NOT_SUPPORTED",
        AppError::InvalidImage(_) => "INVALID_IMAGE",
        AppError::NoImage(_) => "NO_IMAGE",
        AppError::FrameExtractFailed(_) => "FRAME_EXTRACT_FAILED",
        AppError::InvalidArchive(_) => "INVALID_ARCHIVE",
        _ => "INTERNAL_ERROR",
    };
    (code.to_string(), err.to_string())
}
