use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::routing::get;
use axum::{Router, body};
use tower::ServiceExt;

use super::*;
use crate::config::Settings;
use crate::services::dir_index::DirIndex;
use crate::services::node_registry::NodeRegistry;
use crate::services::path_security::PathSecurity;
use crate::services::temp_file_cache::TempFileCache;
use crate::services::thumbnail_service::ThumbnailService;
use crate::services::thumbnail_warmer::ThumbnailWarmer;
use crate::services::video_converter::VideoConverter;

// --- 通常ファイル用セットアップ ---

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
        root.to_string_lossy().into_owned(),
    )]))
    .unwrap();

    let ps = Arc::new(PathSecurity::new(vec![root], false).unwrap());
    let registry = NodeRegistry::new(Arc::clone(&ps), 100_000, HashMap::new());
    let archive_service = Arc::new(crate::services::archive::ArchiveService::new(&settings));
    let temp_file_cache = Arc::new(
        TempFileCache::new(tempfile::TempDir::new().unwrap().keep(), 10 * 1024 * 1024).unwrap(),
    );
    let thumbnail_service = Arc::new(ThumbnailService::new(Arc::clone(&temp_file_cache)));
    let video_converter = Arc::new(VideoConverter::new(Arc::clone(&temp_file_cache), &settings));
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
        thumb_semaphore: Arc::new(tokio::sync::Semaphore::new(8)),
        archive_thumb_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
        indexer,
        dir_index,
        last_rebuild: tokio::sync::Mutex::new(None),
        scan_complete: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        registry_populate_stats: Arc::new(crate::services::node_registry::PopulateStats::default()),
        last_scan_report: Arc::new(std::sync::RwLock::new(None)),
        rebuild_guard: Arc::new(crate::services::rebuild_guard::RebuildGuard::new()),
        file_watcher: Arc::new(Mutex::new(None)),
        path_security: ps,
        shutdown: crate::state::ShutdownFields::fresh(),
    });

    let app = Router::new()
        .route("/api/file/{node_id}", get(serve_file))
        .with_state(Arc::clone(&app_state));

    (app, app_state, dir)
}

// --- アーカイブエントリ用セットアップ ---

fn create_archive_setup() -> (Router, Arc<AppState>, tempfile::TempDir) {
    use std::io::Write;
    let dir = tempfile::TempDir::new().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();

    // テスト用 ZIP ファイルを作成
    let zip_path = root.join("photos.zip");
    let file = std::fs::File::create(&zip_path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
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
        root.to_string_lossy().into_owned(),
    )]))
    .unwrap();

    let ps = Arc::new(PathSecurity::new(vec![root], false).unwrap());
    let registry = NodeRegistry::new(Arc::clone(&ps), 100_000, HashMap::new());
    let archive_service = Arc::new(crate::services::archive::ArchiveService::new(&settings));
    let temp_file_cache = Arc::new(
        TempFileCache::new(tempfile::TempDir::new().unwrap().keep(), 10 * 1024 * 1024).unwrap(),
    );
    let thumbnail_service = Arc::new(ThumbnailService::new(Arc::clone(&temp_file_cache)));
    let video_converter = Arc::new(VideoConverter::new(Arc::clone(&temp_file_cache), &settings));
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
        thumb_semaphore: Arc::new(tokio::sync::Semaphore::new(8)),
        archive_thumb_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
        indexer,
        dir_index,
        last_rebuild: tokio::sync::Mutex::new(None),
        scan_complete: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        registry_populate_stats: Arc::new(crate::services::node_registry::PopulateStats::default()),
        last_scan_report: Arc::new(std::sync::RwLock::new(None)),
        rebuild_guard: Arc::new(crate::services::rebuild_guard::RebuildGuard::new()),
        file_watcher: Arc::new(Mutex::new(None)),
        path_security: ps,
        shutdown: crate::state::ShutdownFields::fresh(),
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

// --- 通常ファイル配信テスト ---

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
    let expected_etag = super::regular::compute_file_etag(&meta, "etag_override.txt");

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

// --- MKV remux (通常ファイル) ---

#[tokio::test]
async fn mkv_remuxが不要な拡張子ではそのまま配信される() {
    let (app, state, dir) = full_setup();
    let file_path = dir.path().join("clip.mp4");
    std::fs::write(&file_path, b"fake-mp4").unwrap();
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
    // mp4 はそのまま配信 (remux されない)
    assert!(
        ct.contains("mp4"),
        "Content-Type に mp4 が含まれるべき: {ct}"
    );
}

#[tokio::test]
async fn mkv_remux対象でffmpegがない場合は元ファイルをフォールバック配信する() {
    let (app, state, dir) = full_setup();
    let file_path = dir.path().join("movie.mkv");
    std::fs::write(&file_path, b"fake-mkv-data").unwrap();
    let node_id = register_file(&state, &file_path);

    let resp = app
        .oneshot(
            Request::get(format!("/api/file/{node_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // ffmpeg がない環境では remux 失敗 → フォールバックで 200 返却
    assert_eq!(resp.status(), StatusCode::OK);
    let body_bytes = body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body_bytes[..], b"fake-mkv-data");
}

// --- アーカイブエントリ配信テスト ---

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

// --- MKV remux (アーカイブ内) ---

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
