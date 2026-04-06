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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use axum::routing::get;
    use axum::{Router, body};
    use tower::ServiceExt;

    use crate::config::Settings;
    use crate::routers::file::serve_file;
    use crate::services::dir_index::DirIndex;
    use crate::services::node_registry::NodeRegistry;
    use crate::services::path_security::PathSecurity;
    use crate::services::temp_file_cache::TempFileCache;
    use crate::services::thumbnail_service::ThumbnailService;
    use crate::services::thumbnail_warmer::ThumbnailWarmer;
    use crate::services::video_converter::VideoConverter;
    use crate::state::AppState;

    fn create_archive_setup() -> (Router, Arc<AppState>, tempfile::TempDir) {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();

        // テスト用 ZIP ファイルを作成
        let zip_path = root.join("photos.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file("photo.jpg", options).unwrap();
        writer.write_all(b"fake jpeg data here").unwrap();
        writer.start_file("video.mp4", options).unwrap();
        writer
            .write_all(b"fake video data for range test!!")
            .unwrap();
        writer.start_file("document.pdf", options).unwrap();
        writer.write_all(b"fake pdf content here").unwrap();
        writer.start_file("movie.mkv", options).unwrap();
        writer.write_all(b"fake mkv data here").unwrap();
        writer.finish().unwrap();

        let settings = Settings::from_map(&HashMap::from([(
            "MOUNT_BASE_DIR".to_string(),
            root.to_string_lossy().to_string(),
        )]))
        .unwrap();

        let ps = Arc::new(PathSecurity::new(vec![root], false).unwrap());
        let registry = NodeRegistry::new(ps, 100_000, HashMap::new());
        let archive_service = Arc::new(crate::services::archive::ArchiveService::new(&settings));
        let temp_file_cache = Arc::new(
            TempFileCache::new(tempfile::TempDir::new().unwrap().keep(), 10 * 1024 * 1024).unwrap(),
        );
        let thumbnail_service = Arc::new(ThumbnailService::new(Arc::clone(&temp_file_cache)));
        let video_converter =
            Arc::new(VideoConverter::new(Arc::clone(&temp_file_cache), &settings));
        let thumbnail_warmer = Arc::new(ThumbnailWarmer::new(4));
        let index_db = tempfile::NamedTempFile::new().unwrap();
        let indexer = Arc::new(crate::services::indexer::Indexer::new(
            index_db.path().to_str().unwrap(),
        ));
        indexer.init_db().unwrap();
        let dir_index_db = tempfile::NamedTempFile::new().unwrap();
        let dir_index = Arc::new(DirIndex::new(dir_index_db.path().to_str().unwrap()));
        dir_index.init_db().unwrap();

        let app_state = Arc::new(AppState {
            settings: Arc::new(settings),
            node_registry: Arc::new(Mutex::new(registry)),
            archive_service,
            temp_file_cache,
            thumbnail_service,
            video_converter,
            thumbnail_warmer,
            indexer,
            dir_index,
            last_rebuild: tokio::sync::Mutex::new(None),
        });

        let app = Router::new()
            .route("/api/file/{node_id}", get(serve_file))
            .with_state(Arc::clone(&app_state));

        (app, app_state, dir)
    }

    /// アーカイブエントリの `node_id` を取得するヘルパー
    fn register_archive_entry(
        state: &Arc<AppState>,
        archive_path: &std::path::Path,
        entry_name: &str,
    ) -> String {
        let mut reg = state.node_registry.lock().unwrap();
        reg.register_archive_entry(archive_path, entry_name)
            .unwrap()
    }

    #[tokio::test]
    async fn アーカイブエントリのnode_idで画像データを返す() {
        let (app, state, dir) = create_archive_setup();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let entry_nid = register_archive_entry(&state, &root.join("photos.zip"), "photo.jpg");

        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{entry_nid}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body_bytes = body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body_bytes[..], b"fake jpeg data here");
    }

    #[tokio::test]
    async fn アーカイブエントリに正しいcontent_typeを返す() {
        let (app, state, dir) = create_archive_setup();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let entry_nid = register_archive_entry(&state, &root.join("photos.zip"), "photo.jpg");

        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{entry_nid}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let ct = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(ct, "image/jpeg");
    }

    #[tokio::test]
    async fn アーカイブエントリのetag_304が機能する() {
        let (app, state, dir) = create_archive_setup();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let entry_nid = register_archive_entry(&state, &root.join("photos.zip"), "photo.jpg");

        // 1回目: ETag を取得
        let resp = app
            .clone()
            .oneshot(
                Request::get(format!("/api/file/{entry_nid}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let etag = resp
            .headers()
            .get(header::ETAG)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        // 2回目: If-None-Match で 304
        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{entry_nid}"))
                    .header(header::IF_NONE_MATCH, &etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
    }

    // --- 大容量エントリ (動画/PDF) の TempFileCache + ServeFile 配信 ---

    #[tokio::test]
    async fn アーカイブ内動画エントリがservefileで配信される() {
        let (app, state, dir) = create_archive_setup();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let entry_nid = register_archive_entry(&state, &root.join("photos.zip"), "video.mp4");

        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{entry_nid}"))
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
        assert_eq!(ct, "video/mp4");
        let body_bytes = body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body_bytes[..], b"fake video data for range test!!");
    }

    #[tokio::test]
    async fn アーカイブ内動画エントリがrangeリクエストで206を返す() {
        let (app, state, dir) = create_archive_setup();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let entry_nid = register_archive_entry(&state, &root.join("photos.zip"), "video.mp4");

        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{entry_nid}"))
                    .header(header::RANGE, "bytes=0-3")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
        let body_bytes = body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body_bytes[..], b"fake");
    }

    #[tokio::test]
    async fn アーカイブ内pdfエントリがservefileで配信される() {
        let (app, state, dir) = create_archive_setup();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let entry_nid = register_archive_entry(&state, &root.join("photos.zip"), "document.pdf");

        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{entry_nid}"))
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
        assert_eq!(ct, "application/pdf");
        let body_bytes = body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body_bytes[..], b"fake pdf content here");
    }

    #[tokio::test]
    async fn アーカイブ内動画エントリのetag_304が機能する() {
        let (app, state, dir) = create_archive_setup();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let entry_nid = register_archive_entry(&state, &root.join("photos.zip"), "video.mp4");

        // 1回目: ETag を取得
        let resp = app
            .clone()
            .oneshot(
                Request::get(format!("/api/file/{entry_nid}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let etag = resp
            .headers()
            .get(header::ETAG)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        // 2回目: If-None-Match で 304
        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{entry_nid}"))
                    .header(header::IF_NONE_MATCH, &etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
    }

    // --- MKV remux ---

    #[tokio::test]
    async fn アーカイブ内mkv_remux対象でffmpegがない場合はフォールバック配信する() {
        let (app, state, dir) = create_archive_setup();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let entry_nid = register_archive_entry(&state, &root.join("photos.zip"), "movie.mkv");

        let resp = app
            .oneshot(
                Request::get(format!("/api/file/{entry_nid}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // ffmpeg がない環境では remux 失敗 → フォールバックで元データを配信
        assert_eq!(resp.status(), StatusCode::OK);
        let body_bytes = body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body_bytes[..], b"fake mkv data here");
    }
}
