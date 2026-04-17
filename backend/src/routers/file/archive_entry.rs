//! アーカイブエントリ配信
//!
//! - 画像エントリ: インメモリ配信 (`archive_service.extract_entry()`)
//! - 動画/PDF エントリ: `TempFileCache` + `ServeFile` でディスク配信 (Range 対応)
//! - `ETag` は `md5("{archive_mtime_ns}:{entry_name}")` で計算

use std::sync::Arc;
use std::time::UNIX_EPOCH;

use axum::http::{HeaderMap, HeaderValue, Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use md5::{Digest, Md5};
use tower::ServiceExt as _;
use tower_http::services::ServeFile;

use crate::errors::AppError;
use crate::services::extensions::{self, PDF_EXTENSIONS, VIDEO_EXTENSIONS};
use crate::services::video_converter::VideoConverter;
use crate::state::AppState;

/// アーカイブエントリ `ETag` + `mtime_ns` の計算結果
struct ArchiveEntryMeta {
    etag: String,
    etag_quoted: String,
    mtime_ns: u128,
}

/// アーカイブの mtime から `ETag` を計算する
fn compute_archive_entry_meta(
    archive_path: &std::path::Path,
    entry_name: &str,
) -> Result<ArchiveEntryMeta, AppError> {
    let meta = std::fs::metadata(archive_path)
        .map_err(|e| AppError::InvalidArchive(format!("メタデータ取得失敗: {e}")))?;
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    let raw = format!("{mtime_ns}:{entry_name}");
    let digest = Md5::digest(raw.as_bytes());
    let etag = format!("{digest:x}");
    let etag_quoted = format!("\"{etag}\"");
    Ok(ArchiveEntryMeta {
        etag,
        etag_quoted,
        mtime_ns,
    })
}

/// 動画/PDF エントリかどうかを判定する
///
/// これらのエントリは `TempFileCache` 経由でディスクに展開し `ServeFile` で配信する。
fn is_large_archive_entry(ext: &str) -> bool {
    VIDEO_EXTENSIONS.contains(&ext) || PDF_EXTENSIONS.contains(&ext)
}

/// `TempFileCache` 用のキャッシュキーを生成する (Python `TempFileCache.make_key` 互換)
///
/// `MD5("{archive_path}:{mtime_ns}:{entry_name}")`
fn make_archive_temp_cache_key(
    archive_path: &std::path::Path,
    mtime_ns: u128,
    entry_name: &str,
) -> String {
    let raw = format!("{}:{mtime_ns}:{entry_name}", archive_path.display());
    let digest = Md5::digest(raw.as_bytes());
    format!("{digest:x}")
}

/// アーカイブエントリを配信する
///
/// - 動画/PDF エントリ: `TempFileCache` + `ServeFile` (Range 対応)
/// - 画像エントリ: インメモリ配信 (キャッシュ付き)
/// - `ETag` + `If-None-Match` → 304
pub(crate) async fn serve_archive_entry(
    state: &Arc<AppState>,
    archive_path: &std::path::Path,
    entry_name: &str,
    headers: &HeaderMap,
    original_uri: &axum::http::Uri,
) -> Result<Response, AppError> {
    // ETag + mtime_ns を計算
    let a_path = archive_path.to_path_buf();
    let e_name = entry_name.to_string();
    let meta = tokio::task::spawn_blocking({
        let ap = a_path.clone();
        let en = e_name.clone();
        move || compute_archive_entry_meta(&ap, &en)
    })
    .await
    .map_err(|e| AppError::path_security(format!("タスク実行失敗: {e}")))??;

    // If-None-Match → 304
    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH) {
        if let Ok(val) = if_none_match.to_str() {
            if val.trim_matches('"') == meta.etag {
                return Ok((
                    StatusCode::NOT_MODIFIED,
                    [(header::ETAG, meta.etag_quoted.clone())],
                )
                    .into_response());
            }
        }
    }

    let ext = extensions::extract_extension(entry_name).to_ascii_lowercase();

    // 動画/PDF エントリは TempFileCache 経由で ServeFile 配信
    if is_large_archive_entry(&ext) {
        let ctx = LargeEntryContext {
            archive_path: &a_path,
            entry_name: &e_name,
            ext: &ext,
            meta: &meta,
            headers,
            original_uri,
        };
        return serve_archive_large_entry(state, &ctx).await;
    }

    // 画像エントリ: インメモリ配信 (キャッシュ付き)
    let svc = Arc::clone(&state.archive_service);
    let data: Bytes = tokio::task::spawn_blocking(move || svc.extract_entry(&a_path, &e_name))
        .await
        .map_err(|e| AppError::path_security(format!("タスク実行失敗: {e}")))??;

    let content_type = extensions::mime_for_extension(&ext).unwrap_or("application/octet-stream");

    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "private, max-age=3600"),
            (header::ETAG, &meta.etag_quoted),
        ],
        data,
    )
        .into_response())
}

/// 大容量エントリ配信に必要なリクエスト情報
struct LargeEntryContext<'a> {
    archive_path: &'a std::path::Path,
    entry_name: &'a str,
    ext: &'a str,
    meta: &'a ArchiveEntryMeta,
    headers: &'a HeaderMap,
    original_uri: &'a axum::http::Uri,
}

/// 動画/PDF エントリを `TempFileCache` 経由で配信する (Range 対応)
///
/// 1. キャッシュヒット → そのままファイル配信
/// 2. キャッシュミス → `extract_entry_to_file` でストリーミング展開 → 配信
/// 3. `ServeFile` で Range リクエストを自動処理
async fn serve_archive_large_entry(
    state: &Arc<AppState>,
    ctx: &LargeEntryContext<'_>,
) -> Result<Response, AppError> {
    let cache_key =
        make_archive_temp_cache_key(ctx.archive_path, ctx.meta.mtime_ns, ctx.entry_name);
    let suffix = extensions::extract_extension(ctx.entry_name);

    // TempFileCache ヒットチェック
    let entry_path = if let Some(path) = state.temp_file_cache.get(&cache_key) {
        path
    } else {
        // キャッシュミス: extract_entry_to_file → TempFileCache
        let svc = Arc::clone(&state.archive_service);
        let tfc = Arc::clone(&state.temp_file_cache);
        let ap = ctx.archive_path.to_path_buf();
        let en = ctx.entry_name.to_string();
        let sfx = suffix.to_string();
        let ck = cache_key.clone();

        tokio::task::spawn_blocking(move || {
            tfc.put_with_writer(
                &ck,
                |dest| {
                    svc.extract_entry_to_file(&ap, &en, dest)
                        .map_err(|e| std::io::Error::other(format!("{e}")))
                },
                &sfx,
            )
        })
        .await
        .map_err(|e| AppError::path_security(format!("タスク実行失敗: {e}")))?
        .map_err(|e| AppError::InvalidArchive(format!("エントリ展開失敗: {e}")))?
    };

    // MKV remux: アーカイブ内 MKV エントリを MP4 に変換
    let ext_with_dot = format!(".{}", ctx.ext.trim_start_matches('.'));
    let remuxed_path =
        if VideoConverter::needs_remux(&ext_with_dot) && state.video_converter.is_available() {
            let vc = Arc::clone(&state.video_converter);
            let p = entry_path.clone();
            let mtime = ctx.meta.mtime_ns;
            tokio::task::spawn_blocking(move || vc.get_remuxed(&p, mtime))
                .await
                .map_err(|e| AppError::path_security(format!("タスク実行失敗: {e}")))?
        } else {
            None
        };
    let serve_path = remuxed_path.as_deref().unwrap_or(&entry_path);

    // ServeFile で配信 (Range 対応)
    let mut serve_req = Request::builder()
        .uri(ctx.original_uri)
        .body(axum::body::Body::empty())
        .map_err(|e| AppError::path_security(format!("リクエスト構築失敗: {e}")))?;
    for key in [header::RANGE, header::IF_RANGE] {
        if let Some(val) = ctx.headers.get(&key) {
            serve_req.headers_mut().insert(key, val.clone());
        }
    }

    let mut response = ServeFile::new(serve_path)
        .oneshot(serve_req)
        .await
        .map_err(|e| AppError::path_security(format!("ファイル配信失敗: {e}")))?
        .into_response();

    // Content-Type + ETag + Cache-Control を上書き
    // remux 成功時は video/mp4 に上書き
    let content_type = if remuxed_path.is_some() {
        "video/mp4"
    } else {
        extensions::mime_for_extension(ctx.ext).unwrap_or("application/octet-stream")
    };
    let resp_headers = response.headers_mut();
    resp_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .map_err(|e| AppError::path_security(format!("Content-Type 設定失敗: {e}")))?,
    );
    resp_headers.insert(
        header::ETAG,
        HeaderValue::from_str(&ctx.meta.etag_quoted)
            .map_err(|e| AppError::path_security(format!("ETag ヘッダ設定失敗: {e}")))?,
    );
    resp_headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=3600"),
    );

    Ok(response)
}
