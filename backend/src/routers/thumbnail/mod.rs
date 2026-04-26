//! サムネイル API
//!
//! - `GET /api/thumbnail/{node_id}` — 単一サムネイル (JPEG)
//! - `POST /api/thumbnails/batch` — バッチサムネイル (base64 JSON)

mod batch;
pub(crate) use batch::serve_thumbnails_batch;

use std::sync::Arc;
use std::time::UNIX_EPOCH;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use md5::{Digest, Md5};
use serde::Deserialize;

use crate::errors::AppError;
use crate::services::extensions::{IMAGE_EXTENSIONS, PDF_EXTENSIONS, VIDEO_EXTENSIONS};
use crate::state::AppState;

/// サムネイル `ETag` を計算する
///
/// `MD5("thumb:{mtime_ns}:{node_id}")`
pub(super) fn compute_thumb_etag(mtime_ns: u128, node_id: &str) -> String {
    let raw = format!("thumb:{mtime_ns}:{node_id}");
    let digest = Md5::digest(raw.as_bytes());
    format!("\"{digest:x}\"")
}

/// ファイルの `mtime_ns` を取得する
pub(super) fn get_mtime_ns(path: &std::path::Path) -> Result<u128, AppError> {
    let meta = std::fs::metadata(path).map_err(|_| AppError::NodeNotFound {
        node_id: String::new(),
    })?;
    Ok(meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos()))
}

/// アーカイブサムネイル生成をスキップするファイルサイズ上限 (1.5 GB)
///
/// WSL2 drvfs 上の大規模アーカイブはエントリ列挙だけで数秒かかるため、
/// サムネイル生成を諦めて応答速度を優先する。
const ARCHIVE_THUMB_MAX_BYTES: u64 = 1_500_000_000;

/// ファイルの `mtime_ns` + サイズを取得する（ディレクトリは拒否、metadata 1 回のみ）
///
/// `is_dir()` + `get_mtime_ns()` の 2 回 stat を統合。
/// 類似: `thumbnail_warmer.rs` の `mtime_ns_of`（`std::io::Error` 版、ディレクトリ判定なし）
pub(super) fn file_meta(path: &std::path::Path, node_id: &str) -> Result<(u128, u64), AppError> {
    let meta = std::fs::metadata(path).map_err(|_| AppError::NodeNotFound {
        node_id: node_id.to_string(),
    })?;
    if meta.is_dir() {
        return Err(AppError::NotSupported(
            "ディレクトリのサムネイルは非対応です".to_string(),
        ));
    }
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    Ok((mtime_ns, meta.len()))
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
pub(super) struct ThumbnailResult {
    pub(super) data: Vec<u8>,
    pub(super) etag: String,
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
        let etag = compute_thumb_etag(mtime_ns, node_id);
        // archive-entry: extract_entry も dedup 対象に含める
        let thumb = thumb_svc.generate_with_dedup(&cache_key, || {
            let data = state
                .archive_service
                .extract_entry(&archive_path, &entry_name)?;
            thumb_svc.generate_from_bytes_inner(&data, &cache_key)
        })?;
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

    // ディレクトリチェック + mtime + サイズ取得 (metadata 1 回)
    let (mtime_ns, file_size) = file_meta(&resolved, node_id)?;
    let cache_key = thumb_svc.make_cache_key(node_id, mtime_ns);
    let etag = compute_thumb_etag(mtime_ns, node_id);

    let ext = ext_lower(&resolved.to_string_lossy());

    // アーカイブファイル自体 → 先頭画像エントリのサムネイル
    if state.archive_service.is_supported(&resolved) {
        if file_size > ARCHIVE_THUMB_MAX_BYTES {
            return Err(AppError::NotSupported(
                "アーカイブが大きすぎるためサムネイル生成をスキップします".to_string(),
            ));
        }
        // archive-file: first_image_entry + extract_entry も dedup 対象に含める
        let thumb = thumb_svc.generate_with_dedup(&cache_key, || {
            let entry = state
                .archive_service
                .first_image_entry(&resolved)?
                .ok_or_else(|| {
                    AppError::NoImage("アーカイブ内に画像が見つかりません".to_string())
                })?;
            let data = state
                .archive_service
                .extract_entry(&resolved, &entry.name)?;
            thumb_svc.generate_from_bytes_inner(&data, &cache_key)
        })?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    // PDF (generate_pdf_thumbnail 内で dedup 済み)
    if PDF_EXTENSIONS.contains(&ext.as_str()) {
        let thumb = thumb_svc.generate_pdf_thumbnail(
            &resolved,
            &cache_key,
            state.settings.video_thumb_timeout,
        )?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    // 動画: extract_frame も dedup 対象に含める
    if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        let thumb = thumb_svc.generate_with_dedup(&cache_key, || {
            let frame = video_conv.extract_frame(&resolved).ok_or_else(|| {
                AppError::FrameExtractFailed("動画のフレーム抽出に失敗しました".to_string())
            })?;
            thumb_svc.generate_from_bytes_inner(&frame, &cache_key)
        })?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    // 画像 (generate_from_path 内で dedup 済み)
    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        let thumb = thumb_svc.generate_from_path(&resolved, &cache_key)?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    // その他
    Err(AppError::NotSupported(
        "サムネイル非対応のファイル形式です".to_string(),
    ))
}

/// `mtime_ns` 事前取得済みパスからサムネイルを生成する (batch API プレチェック連携用)
///
/// `pre_check_regular_entries` で stat 済みの `mtime_ns` を受け取り、再 stat を回避する。
pub(super) fn generate_thumbnail_with_mtime(
    state: &AppState,
    node_id: &str,
    resolved: &std::path::Path,
    mtime_ns: u128,
    file_size: u64,
) -> Result<ThumbnailResult, AppError> {
    let thumb_svc = &state.thumbnail_service;
    let video_conv = &state.video_converter;

    let cache_key = thumb_svc.make_cache_key(node_id, mtime_ns);
    let etag = compute_thumb_etag(mtime_ns, node_id);
    let ext = ext_lower(&resolved.to_string_lossy());

    // アーカイブファイル自体 → 先頭画像エントリのサムネイル
    if state.archive_service.is_supported(resolved) {
        if file_size > ARCHIVE_THUMB_MAX_BYTES {
            return Err(AppError::NotSupported(
                "アーカイブが大きすぎるためサムネイル生成をスキップします".to_string(),
            ));
        }
        // archive-file: first_image_entry + extract_entry も dedup 対象に含める
        let thumb = thumb_svc.generate_with_dedup(&cache_key, || {
            let entry = state
                .archive_service
                .first_image_entry(resolved)?
                .ok_or_else(|| {
                    AppError::NoImage("アーカイブ内に画像が見つかりません".to_string())
                })?;
            let data = state.archive_service.extract_entry(resolved, &entry.name)?;
            thumb_svc.generate_from_bytes_inner(&data, &cache_key)
        })?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    // PDF (generate_pdf_thumbnail 内で dedup 済み)
    if PDF_EXTENSIONS.contains(&ext.as_str()) {
        let thumb = thumb_svc.generate_pdf_thumbnail(
            resolved,
            &cache_key,
            state.settings.video_thumb_timeout,
        )?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    // 動画: extract_frame も dedup 対象に含める
    if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        let thumb = thumb_svc.generate_with_dedup(&cache_key, || {
            let frame = video_conv.extract_frame(resolved).ok_or_else(|| {
                AppError::FrameExtractFailed("動画のフレーム抽出に失敗しました".to_string())
            })?;
            thumb_svc.generate_from_bytes_inner(&frame, &cache_key)
        })?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    // 画像 (generate_from_path 内で dedup 済み)
    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        let thumb = thumb_svc.generate_from_path(resolved, &cache_key)?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    Err(AppError::NotSupported(
        "サムネイル非対応のファイル形式です".to_string(),
    ))
}
