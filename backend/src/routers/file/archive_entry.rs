//! アーカイブエントリ配信
//!
//! アーカイブ内のファイルをインメモリ配信する。
//! - `archive_service.extract_entry()` でバイト抽出 (キャッシュ付き)
//! - `Content-Type` は拡張子から判定
//! - `ETag` は `md5("{archive_mtime_ns}:{entry_name}")` で計算

use std::sync::Arc;
use std::time::UNIX_EPOCH;

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use md5::{Digest, Md5};

use crate::errors::AppError;
use crate::services::extensions;
use crate::state::AppState;

/// アーカイブエントリをインメモリ配信する
///
/// - `archive_service.extract_entry()` でバイト抽出 (キャッシュ付き)
/// - `Content-Type` は拡張子から判定
/// - `ETag` は `md5("{archive_mtime_ns}:{entry_name}")` で計算
/// - `If-None-Match` → 304
pub(crate) async fn serve_archive_entry(
    state: &Arc<AppState>,
    archive_path: &std::path::Path,
    entry_name: &str,
    headers: &HeaderMap,
) -> Result<Response, AppError> {
    // ETag: アーカイブの mtime + エントリ名
    let a_path = archive_path.to_path_buf();
    let e_name = entry_name.to_string();
    let etag = tokio::task::spawn_blocking({
        let ap = a_path.clone();
        let en = e_name.clone();
        move || -> Result<String, AppError> {
            let meta = std::fs::metadata(&ap)
                .map_err(|e| AppError::InvalidArchive(format!("メタデータ取得失敗: {e}")))?;
            let mtime_ns = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            let raw = format!("{mtime_ns}:{en}");
            let digest = Md5::digest(raw.as_bytes());
            Ok(format!("{digest:x}"))
        }
    })
    .await
    .map_err(|e| AppError::path_security(format!("タスク実行失敗: {e}")))??;

    let etag_quoted = format!("\"{etag}\"");

    // If-None-Match → 304
    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH) {
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

    // アーカイブエントリを抽出 (キャッシュ付き)
    let svc = Arc::clone(&state.archive_service);
    let data: Bytes = tokio::task::spawn_blocking(move || svc.extract_entry(&a_path, &e_name))
        .await
        .map_err(|e| AppError::path_security(format!("タスク実行失敗: {e}")))??;

    // Content-Type を拡張子から判定
    let ext = extensions::extract_extension(entry_name).to_ascii_lowercase();
    let content_type = extensions::mime_for_extension(&ext).unwrap_or("application/octet-stream");

    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "private, max-age=3600"),
            (header::ETAG, &etag_quoted),
        ],
        data,
    )
        .into_response())
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
        writer.write_all(b"fake video data").unwrap();
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

    /// アーカイブエントリの `node_id` を取得���るヘルパー
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
}
