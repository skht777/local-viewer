use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use tempfile::TempDir;
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

// --- テストヘルパー ---

fn test_state(
    root: &std::path::Path,
    mount_names: HashMap<std::path::PathBuf, String>,
) -> Arc<AppState> {
    let settings = Settings::from_map(&HashMap::from([(
        "MOUNT_BASE_DIR".to_string(),
        root.to_string_lossy().into_owned(),
    )]))
    .unwrap();
    let ps = Arc::new(PathSecurity::new(vec![root.to_path_buf()], false).unwrap());
    let registry = NodeRegistry::new(ps, 100_000, mount_names);
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
    Arc::new(AppState {
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
    })
}

fn app(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/browse/{node_id}", get(browse_directory))
        .route("/api/browse/{node_id}/first-viewable", get(first_viewable))
        .route("/api/browse/{node_id}/sibling", get(find_sibling))
        .with_state(state)
}

/// `node_id` を取得するヘルパー (register 経由)
fn register_node_id(state: &Arc<AppState>, path: &std::path::Path) -> String {
    #[allow(clippy::expect_used, reason = "テストコード")]
    let mut reg = state.node_registry.lock().expect("lock");
    #[allow(clippy::expect_used, reason = "テストコード")]
    reg.register(path).expect("register")
}

async fn get_response(app: Router, uri: &str) -> (StatusCode, String, HeaderMap) {
    let resp = app
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap(), headers)
}

async fn get_json(app: Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let (status, body, _) = get_response(app, uri).await;
    let json: serde_json::Value = if body.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&body).unwrap()
    };
    (status, json)
}

async fn get_json_with_headers(
    app: Router,
    uri: &str,
    extra_headers: Vec<(&str, &str)>,
) -> (StatusCode, serde_json::Value, HeaderMap) {
    let mut req = Request::get(uri);
    for (k, v) in extra_headers {
        req = req.header(k, v);
    }
    let resp = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = if body.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&body).unwrap()
    };
    (status, json, headers)
}

/// テスト用ディレクトリを作成するヘルパー
fn create_test_dir() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    // サブディレクトリ
    fs::create_dir_all(root.join("photos")).unwrap();
    fs::write(root.join("photos/img1.jpg"), "fake-jpg-1").unwrap();
    fs::write(root.join("photos/img2.png"), "fake-png-2").unwrap();
    fs::write(root.join("photos/doc.pdf"), "fake-pdf").unwrap();
    // ルート直下にファイル
    fs::write(root.join("readme.txt"), "hello").unwrap();
    fs::write(root.join("video.mp4"), "fake-video").unwrap();
    (dir, root)
}

// --- browse_directory テスト ---

#[tokio::test]
async fn ディレクトリ一覧が正しく返る() {
    let (_dir, root) = create_test_dir();
    let state = test_state(&root, HashMap::new());
    let node_id = register_node_id(&state, &root);

    let (status, json) = get_json(app(state), &format!("/api/browse/{node_id}")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["current_node_id"], node_id);
    let entries = json["entries"].as_array().unwrap();
    // photos (dir) + readme.txt + video.mp4 = 3
    assert_eq!(entries.len(), 3);
}

#[tokio::test]
async fn 存在しないnode_idで404を返す() {
    let (_dir, root) = create_test_dir();
    let state = test_state(&root, HashMap::new());

    let (status, json) = get_json(app(state), "/api/browse/nonexistent_node_id").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["code"], "NOT_FOUND");
}

#[tokio::test]
async fn ファイルのnode_idで422を返す() {
    let (_dir, root) = create_test_dir();
    let state = test_state(&root, HashMap::new());
    let file_id = register_node_id(&state, &root.join("readme.txt"));

    let (status, json) = get_json(app(state), &format!("/api/browse/{file_id}")).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json["code"], "NOT_A_DIRECTORY");
}

#[tokio::test]
async fn etagヘッダが含まれる() {
    let (_dir, root) = create_test_dir();
    let state = test_state(&root, HashMap::new());
    let node_id = register_node_id(&state, &root);

    let (status, _body, headers) =
        get_response(app(state), &format!("/api/browse/{node_id}")).await;

    assert_eq!(status, StatusCode::OK);
    assert!(headers.contains_key("etag"));
    assert_eq!(
        headers.get("cache-control").unwrap().to_str().unwrap(),
        "private, no-cache"
    );
}

#[tokio::test]
async fn if_none_matchで304を返す() {
    let (_dir, root) = create_test_dir();
    let state = test_state(&root, HashMap::new());
    let node_id = register_node_id(&state, &root);
    let uri = format!("/api/browse/{node_id}");

    // 1回目: ETag を取得
    let (_status, _body, headers) = get_response(app(Arc::clone(&state)), &uri).await;
    let etag = headers.get("etag").unwrap().to_str().unwrap().to_string();

    // 2回目: if-none-match で 304
    let (status, _json, _headers) =
        get_json_with_headers(app(state), &uri, vec![("if-none-match", &etag)]).await;

    assert_eq!(status, StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn limitでページネーションが効く() {
    let (_dir, root) = create_test_dir();
    let state = test_state(&root, HashMap::new());
    let node_id = register_node_id(&state, &root);

    let (status, json) = get_json(app(state), &format!("/api/browse/{node_id}?limit=2")).await;

    assert_eq!(status, StatusCode::OK);
    let entries = json["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(json["next_cursor"].is_string());
    assert_eq!(json["total_count"], 3);
}

#[tokio::test]
async fn cursorで2ページ目を取得して重複がない() {
    // 4ファイルのディレクトリを作成
    let dir = TempDir::new().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::write(root.join("large.png"), "fake-png").unwrap();
    fs::write(root.join("photo1.jpg"), "fake-jpg-1").unwrap();
    fs::write(root.join("photo2.jpg"), "fake-jpg-2").unwrap();
    fs::write(root.join("photo3.jpg"), "fake-jpg-3").unwrap();

    let state = test_state(&root, HashMap::new());
    let node_id = register_node_id(&state, &root);

    // 1ページ目: limit=2, sort=name-asc
    let (status1, json1) = get_json(
        app(Arc::clone(&state)),
        &format!("/api/browse/{node_id}?limit=2&sort=name-asc"),
    )
    .await;
    assert_eq!(status1, StatusCode::OK);
    let entries1 = json1["entries"].as_array().unwrap();
    assert_eq!(entries1.len(), 2);
    let cursor = json1["next_cursor"].as_str().unwrap();

    // 2ページ目: cursor 使用
    let (status2, json2) = get_json(
        app(Arc::clone(&state)),
        &format!("/api/browse/{node_id}?limit=2&sort=name-asc&cursor={cursor}"),
    )
    .await;
    assert_eq!(status2, StatusCode::OK);
    let entries2 = json2["entries"].as_array().unwrap();
    assert!(!entries2.is_empty(), "2ページ目にエントリがあるはず");

    // 重複なし
    let ids1: Vec<&str> = entries1
        .iter()
        .map(|e| e["node_id"].as_str().unwrap())
        .collect();
    let ids2: Vec<&str> = entries2
        .iter()
        .map(|e| e["node_id"].as_str().unwrap())
        .collect();
    let overlap: Vec<&&str> = ids1.iter().filter(|id| ids2.contains(id)).collect();
    assert!(
        overlap.is_empty(),
        "ページ間に重複があってはならない: {overlap:?}"
    );

    // 全4ファイルが網羅されている
    assert_eq!(ids1.len() + ids2.len(), 4);
}

#[tokio::test]
async fn cursorのbase64がクエリパラメータで正しくデコードされる() {
    let dir = TempDir::new().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::write(root.join("a.jpg"), "1").unwrap();
    fs::write(root.join("b.jpg"), "2").unwrap();
    fs::write(root.join("c.jpg"), "3").unwrap();

    let state = test_state(&root, HashMap::new());
    let node_id = register_node_id(&state, &root);

    let (_, json1) = get_json(
        app(Arc::clone(&state)),
        &format!("/api/browse/{node_id}?limit=1&sort=name-asc"),
    )
    .await;
    let cursor = json1["next_cursor"].as_str().unwrap();
    // URL_SAFE_NO_PAD のためパディングなし
    assert!(
        !cursor.ends_with('='),
        "カーソルに base64 パディングがないこと"
    );

    let (status2, json2) = get_json(
        app(Arc::clone(&state)),
        &format!("/api/browse/{node_id}?limit=1&sort=name-asc&cursor={cursor}"),
    )
    .await;
    assert_eq!(status2, StatusCode::OK);
    let entries2 = json2["entries"].as_array().unwrap();
    assert_eq!(
        entries2[0]["name"].as_str().unwrap(),
        "b.jpg",
        "カーソルで正しい位置から取得できるはず"
    );
}

#[tokio::test]
async fn limitが0で400エラー() {
    let (_dir, root) = create_test_dir();
    let state = test_state(&root, HashMap::new());
    let node_id = register_node_id(&state, &root);

    let (status, _json) = get_json(app(state), &format!("/api/browse/{node_id}?limit=0")).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn parent_node_idとancestorsが含まれる() {
    let (_dir, root) = create_test_dir();
    let state = test_state(&root, HashMap::new());
    let photos_id = register_node_id(&state, &root.join("photos"));

    let (status, json) = get_json(app(state), &format!("/api/browse/{photos_id}")).await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["parent_node_id"].is_string());
    let ancestors = json["ancestors"].as_array().unwrap();
    // マウントルート 1 件
    assert!(!ancestors.is_empty());
}

#[tokio::test]
async fn limitなしでtotal_countがnull() {
    let (_dir, root) = create_test_dir();
    let state = test_state(&root, HashMap::new());
    let node_id = register_node_id(&state, &root);

    let (status, json) = get_json(app(state), &format!("/api/browse/{node_id}")).await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["total_count"].is_null());
    assert!(json["next_cursor"].is_null());
}

// --- first_viewable テスト ---

#[tokio::test]
async fn first_viewableで画像が見つかる() {
    let (_dir, root) = create_test_dir();
    let state = test_state(&root, HashMap::new());
    let photos_id = register_node_id(&state, &root.join("photos"));

    let (status, json) = get_json(
        app(state),
        &format!("/api/browse/{photos_id}/first-viewable"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let entry = &json["entry"];
    assert!(!entry.is_null());
    // pdf が archive/pdf/image の優先順位で最初に見つかるはず
    assert_eq!(entry["kind"], "pdf");
    assert!(json["parent_node_id"].is_string());
}

#[tokio::test]
async fn first_viewableで再帰降下する() {
    let dir = TempDir::new().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    // 深い階層構造: root/a/b/img.jpg
    fs::create_dir_all(root.join("a/b")).unwrap();
    fs::write(root.join("a/b/img.jpg"), "jpg").unwrap();

    let state = test_state(&root, HashMap::new());
    let root_id = register_node_id(&state, &root);

    let (status, json) =
        get_json(app(state), &format!("/api/browse/{root_id}/first-viewable")).await;

    assert_eq!(status, StatusCode::OK);
    let entry = &json["entry"];
    assert!(!entry.is_null());
    assert_eq!(entry["name"], "img.jpg");
    assert_eq!(entry["kind"], "image");
}

#[tokio::test]
async fn first_viewableで空ディレクトリはnullを返す() {
    let dir = TempDir::new().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(root.join("empty")).unwrap();

    let state = test_state(&root, HashMap::new());
    let empty_id = register_node_id(&state, &root.join("empty"));

    let (status, json) = get_json(
        app(state),
        &format!("/api/browse/{empty_id}/first-viewable"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["entry"].is_null());
}

// --- find_sibling テスト ---

#[tokio::test]
async fn siblingでnextが見つかる() {
    let dir = TempDir::new().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(root.join("set_a")).unwrap();
    fs::create_dir_all(root.join("set_b")).unwrap();
    fs::create_dir_all(root.join("set_c")).unwrap();

    let state = test_state(&root, HashMap::new());
    let root_id = register_node_id(&state, &root);
    let set_a_id = register_node_id(&state, &root.join("set_a"));

    let (status, json) = get_json(
        app(state),
        &format!("/api/browse/{root_id}/sibling?current={set_a_id}&direction=next"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let entry = &json["entry"];
    assert!(!entry.is_null());
    assert_eq!(entry["name"], "set_b");
}

#[tokio::test]
async fn siblingでprevが見つかる() {
    let dir = TempDir::new().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(root.join("set_a")).unwrap();
    fs::create_dir_all(root.join("set_b")).unwrap();
    fs::create_dir_all(root.join("set_c")).unwrap();

    let state = test_state(&root, HashMap::new());
    let root_id = register_node_id(&state, &root);
    let set_c_id = register_node_id(&state, &root.join("set_c"));

    let (status, json) = get_json(
        app(state),
        &format!("/api/browse/{root_id}/sibling?current={set_c_id}&direction=prev"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let entry = &json["entry"];
    assert!(!entry.is_null());
    assert_eq!(entry["name"], "set_b");
}

#[tokio::test]
async fn siblingで末尾のnextはnull() {
    let dir = TempDir::new().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(root.join("set_a")).unwrap();
    fs::create_dir_all(root.join("set_b")).unwrap();

    let state = test_state(&root, HashMap::new());
    let root_id = register_node_id(&state, &root);
    let set_b_id = register_node_id(&state, &root.join("set_b"));

    let (status, json) = get_json(
        app(state),
        &format!("/api/browse/{root_id}/sibling?current={set_b_id}&direction=next"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["entry"].is_null());
}

#[tokio::test]
async fn siblingで存在しないcurrentはnull() {
    let dir = TempDir::new().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(root.join("set_a")).unwrap();

    let state = test_state(&root, HashMap::new());
    let root_id = register_node_id(&state, &root);

    let (status, json) = get_json(
        app(state),
        &format!("/api/browse/{root_id}/sibling?current=nonexistent&direction=next"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["entry"].is_null());
}

// --- compute_etag テスト ---

#[test]
fn etagが同じエントリで同じ値を返す() {
    let entries = vec![EntryMeta {
        node_id: "abc".to_string(),
        name: "test.jpg".to_string(),
        kind: EntryKind::Image,
        size_bytes: Some(1024),
        mime_type: None,
        child_count: None,
        modified_at: Some(100.0),
        mtime_ns: None,
        preview_node_ids: None,
    }];
    let etag1 = compute_etag(&entries);
    let etag2 = compute_etag(&entries);
    assert_eq!(etag1, etag2);
    // MD5 hex = 32 文字
    assert_eq!(etag1.len(), 32);
}

#[test]
fn etagが異なるエントリで異なる値を返す() {
    let entries_a = vec![EntryMeta {
        node_id: "abc".to_string(),
        name: "a.jpg".to_string(),
        kind: EntryKind::Image,
        size_bytes: Some(1024),
        mime_type: None,
        child_count: None,
        modified_at: Some(100.0),
        mtime_ns: None,
        preview_node_ids: None,
    }];
    let entries_b = vec![EntryMeta {
        node_id: "abc".to_string(),
        name: "b.jpg".to_string(),
        kind: EntryKind::Image,
        size_bytes: Some(1024),
        mime_type: None,
        child_count: None,
        modified_at: Some(100.0),
        mtime_ns: None,
        preview_node_ids: None,
    }];
    assert_ne!(compute_etag(&entries_a), compute_etag(&entries_b));
}

// --- アーカイブ閲覧 ---

fn create_archive_setup() -> (Router, Arc<AppState>, tempfile::TempDir) {
    use std::io::Write;
    let dir = tempfile::TempDir::new().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();

    // テスト用 ZIP ファイルを作成
    let zip_path = root.join("images.zip");
    let file = std::fs::File::create(&zip_path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file("img01.jpg", options).unwrap();
    writer.write_all(b"fake jpg").unwrap();
    writer.start_file("img02.png", options).unwrap();
    writer.write_all(b"fake png").unwrap();
    writer.finish().unwrap();

    let state = test_state(&root, HashMap::from([(root.clone(), "test".to_string())]));
    let app = Router::new()
        .route("/api/browse/{node_id}", get(browse_directory))
        .with_state(Arc::clone(&state));

    (app, state, dir)
}

#[tokio::test]
async fn アーカイブファイルのnode_idでエントリ一覧を返す() {
    let (app, state, dir) = create_archive_setup();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let zip_node_id = register_node_id(&state, &root.join("images.zip"));

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/browse/{zip_node_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let resp: BrowseResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.entries.len(), 2);
    assert_eq!(resp.current_name, "images.zip");
    // エントリ名はファイル名部分のみ
    let names: Vec<&str> = resp.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"img01.jpg"));
    assert!(names.contains(&"img02.png"));
}

#[tokio::test]
async fn アーカイブ閲覧でlimit指定時にページネーションされる() {
    let (app, state, dir) = create_archive_setup();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let zip_node_id = register_node_id(&state, &root.join("images.zip"));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/browse/{zip_node_id}?limit=1"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let resp: BrowseResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.entries.len(), 1, "limit=1 なので 1 件のみ");
    assert!(resp.next_cursor.is_some(), "次ページがあるはず");
    assert_eq!(resp.total_count, Some(2), "全エントリ数は 2");

    // next_cursor で2ページ目を取得
    let cursor = resp.next_cursor.unwrap();
    let response2 = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/browse/{zip_node_id}?limit=1&cursor={cursor}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body2 = axum::body::to_bytes(response2.into_body(), usize::MAX)
        .await
        .unwrap();
    let resp2: BrowseResponse = serde_json::from_slice(&body2).unwrap();
    assert_eq!(resp2.entries.len(), 1, "2ページ目も 1 件");
    assert!(resp2.next_cursor.is_none(), "最終ページ");

    // 重複なし
    assert_ne!(resp.entries[0].name, resp2.entries[0].name);
}

#[tokio::test]
async fn アーカイブ閲覧でtotal_countが返される() {
    let (app, state, dir) = create_archive_setup();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let zip_node_id = register_node_id(&state, &root.join("images.zip"));

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/browse/{zip_node_id}?limit=10"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let resp: BrowseResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.total_count, Some(2));
    assert!(resp.next_cursor.is_none(), "全件収まるので次ページなし");
}

#[tokio::test]
async fn アーカイブのbrowse_responseにparent_node_idが設定される() {
    let (app, state, dir) = create_archive_setup();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let zip_node_id = register_node_id(&state, &root.join("images.zip"));

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/browse/{zip_node_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let resp: BrowseResponse = serde_json::from_slice(&body).unwrap();
    // parent_node_id はルートディレクトリの node_id
    assert!(resp.parent_node_id.is_some());
}

// --- first-viewable アーカイブ対応 ---

#[tokio::test]
async fn first_viewableがアーカイブの中身から最初の閲覧対象を返す() {
    let (_app, state, dir) = create_archive_setup();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let zip_node_id = register_node_id(&state, &root.join("images.zip"));

    let app = Router::new()
        .route("/api/browse/{node_id}/first-viewable", get(first_viewable))
        .with_state(Arc::clone(&state));

    let (status, json) = get_json(app, &format!("/api/browse/{zip_node_id}/first-viewable")).await;

    assert_eq!(status, StatusCode::OK);
    let entry = &json["entry"];
    assert!(!entry.is_null(), "アーカイブ内の画像が返されるはず");
    assert_eq!(entry["kind"], "image");
    // parent_node_id はアーカイブ自体の node_id
    assert_eq!(json["parent_node_id"], zip_node_id);
}
