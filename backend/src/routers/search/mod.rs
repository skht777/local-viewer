//! 検索 API
//!
//! - `GET /api/search` — キーワード検索 (FTS5 trigram)
//! - `POST /api/index/rebuild` — インデックスリビルド
//!
//! モジュール構成:
//! - `handler`: `search()` ハンドラ
//! - `rebuild`: `rebuild_index()` ハンドラ
//!
//! 横断ロジック (scope 検証 / 結果解決 / レート制限) は `services::search` 配下。

mod handler;
mod rebuild;

pub(crate) use handler::search;
pub(crate) use rebuild::rebuild_index;

use serde::{Deserialize, Serialize};

use crate::services::search::SearchResultResponse;

// --- 定数 ---

/// 許可される kind フィルタ値
pub(super) const VALID_KINDS: &[&str] = &["directory", "image", "video", "pdf", "archive"];

/// 許可される sort 値
pub(super) const VALID_SORTS: &[&str] = &[
    "relevance",
    "name-asc",
    "name-desc",
    "date-asc",
    "date-desc",
];

/// クエリ文字数の最小値
pub(super) const MIN_QUERY_LENGTH: usize = 2;

/// クエリ文字数の最大値
pub(super) const MAX_QUERY_LENGTH: usize = 200;

/// デフォルト取得件数
pub(super) const DEFAULT_LIMIT: usize = 50;

/// 最大取得件数
pub(super) const MAX_LIMIT: usize = 200;

// --- クエリ/レスポンス型 ---

/// `GET /api/search` のクエリパラメータ
#[derive(Debug, Deserialize)]
pub(crate) struct SearchQuery {
    /// 検索クエリ (2-200文字)
    pub q: String,
    /// kind フィルタ (directory, image, video, pdf, archive)
    pub kind: Option<String>,
    /// 取得件数 (1-200, デフォルト 50)
    pub limit: Option<usize>,
    /// オフセット (デフォルト 0)
    pub offset: Option<usize>,
    /// ディレクトリスコープ (`node_id`): 指定ディレクトリ配下のみ検索
    pub scope: Option<String>,
    /// ソート順 (relevance/name-asc/name-desc/date-asc/date-desc、デフォルト relevance)
    pub sort: Option<String>,
}

/// 検索 API のレスポンス
#[derive(Debug, Serialize)]
pub(crate) struct SearchResponse {
    pub results: Vec<SearchResultResponse>,
    pub has_more: bool,
    pub query: String,
    pub is_stale: bool,
    /// 次ページのオフセット（`has_more` が false のときは null）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
}

/// リビルド API のレスポンス
#[derive(Debug, Serialize)]
pub(super) struct RebuildResponse {
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- バリデーションテスト (定数の境界値) ---

    #[test]
    fn 最小クエリ文字数が2であること() {
        assert_eq!(MIN_QUERY_LENGTH, 2);
    }

    #[test]
    fn 最大クエリ文字数が200であること() {
        assert_eq!(MAX_QUERY_LENGTH, 200);
    }

    #[test]
    fn デフォルトlimitが50であること() {
        assert_eq!(DEFAULT_LIMIT, 50);
    }

    #[test]
    fn 最大limitが200であること() {
        assert_eq!(MAX_LIMIT, 200);
    }

    #[test]
    fn 有効なkind値がすべて含まれている() {
        assert!(VALID_KINDS.contains(&"directory"));
        assert!(VALID_KINDS.contains(&"image"));
        assert!(VALID_KINDS.contains(&"video"));
        assert!(VALID_KINDS.contains(&"pdf"));
        assert!(VALID_KINDS.contains(&"archive"));
    }

    #[test]
    fn 無効なkind値が含まれていない() {
        assert!(!VALID_KINDS.contains(&"text"));
        assert!(!VALID_KINDS.contains(&"audio"));
        assert!(!VALID_KINDS.contains(&""));
    }

    // --- /api/search?scope= の scope バリデーション統合テスト ---
    //
    // scope パラメータはルーター層で node_id 検証を行うため、
    // そのフローを oneshot で検証する。
    // Indexer 内の scope_prefix 本体の挙動はサービス層テスト側でカバー済み。

    mod scope_validation {
        use std::collections::HashMap;
        use std::fs;
        use std::sync::{Arc, Mutex};

        use axum::Router;
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use axum::routing::get;
        use tempfile::TempDir;
        use tower::ServiceExt;

        use crate::config::Settings;
        use crate::routers::search::search;
        use crate::services::archive::ArchiveService;
        use crate::services::dir_index::DirIndex;
        use crate::services::indexer::Indexer;
        use crate::services::node_registry::NodeRegistry;
        use crate::services::path_security::PathSecurity;
        use crate::services::temp_file_cache::TempFileCache;
        use crate::services::thumbnail_inflight::InflightLocks;
        use crate::services::thumbnail_service::ThumbnailService;
        use crate::services::thumbnail_warmer::ThumbnailWarmer;
        use crate::services::video_converter::VideoConverter;
        use crate::state::AppState;

        fn test_state(root: &std::path::Path, db_dir: &std::path::Path) -> Arc<AppState> {
            let settings = Settings::from_map(&HashMap::from([(
                "MOUNT_BASE_DIR".to_string(),
                root.to_string_lossy().into_owned(),
            )]))
            .unwrap();
            let ps = Arc::new(PathSecurity::new(vec![root.to_path_buf()], false).unwrap());
            let mut registry = NodeRegistry::new(Arc::clone(&ps), 100_000, HashMap::new());
            let mut mount_id_map = HashMap::new();
            mount_id_map.insert("testmount".to_string(), root.to_path_buf());
            registry.set_mount_id_map(mount_id_map);
            let archive_service = Arc::new(ArchiveService::new(&settings));
            let temp_file_cache = Arc::new(
                TempFileCache::new(TempDir::new().unwrap().keep(), 10 * 1024 * 1024).unwrap(),
            );
            let thumbnail_service = Arc::new(ThumbnailService::new(
                Arc::clone(&temp_file_cache),
                InflightLocks::new(),
            ));
            let video_converter =
                Arc::new(VideoConverter::new(Arc::clone(&temp_file_cache), &settings));
            let thumbnail_warmer = Arc::new(ThumbnailWarmer::new(4));
            // NamedTempFile ではハンドルを手放した瞬間に unlink されるため、
            // TempDir 内の固定名ファイルを使って state 生存中は DB を保持する
            let index_db_path = db_dir.join("index.db");
            let indexer = Arc::new(Indexer::new(index_db_path.to_str().unwrap()));
            indexer.init_db().unwrap();
            // search() は is_ready() を要求するため warm start で ready 化
            indexer.mark_warm_start();
            let dir_index_db_path = db_dir.join("dir_index.db");
            let dir_index = Arc::new(DirIndex::new(dir_index_db_path.to_str().unwrap()));
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
                scan_complete: Arc::new(std::sync::atomic::AtomicBool::new(true)),
                registry_populate_stats: Arc::new(
                    crate::services::node_registry::PopulateStats::default(),
                ),
                last_scan_report: Arc::new(std::sync::RwLock::new(None)),
                rebuild_guard: Arc::new(crate::services::rebuild_guard::RebuildGuard::new()),
                file_watcher: Arc::new(std::sync::Mutex::new(None)),
                path_security: ps,
                shutdown: crate::state::ShutdownFields::fresh(),
            })
        }

        fn register_node_id(state: &Arc<AppState>, path: &std::path::Path) -> String {
            let mut reg = state.node_registry.lock().unwrap();
            reg.register(path).unwrap()
        }

        fn app(state: Arc<AppState>) -> Router {
            Router::new()
                .route("/api/search", get(search))
                .with_state(state)
        }

        async fn get_status_and_code(app: Router, uri: &str) -> (StatusCode, serde_json::Value) {
            let resp = app
                .oneshot(Request::get(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let status = resp.status();
            let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap();
            let json: serde_json::Value = if body.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_slice(&body).unwrap()
            };
            (status, json)
        }

        /// テスト用ディレクトリ + DB 用 `TempDir` をペアで作成する
        ///
        /// DB 用 `TempDir` を state とは別ライフタイムで保持することで、
        /// ハンドル drop による DB ファイル消失を防ぐ。
        fn create_test_tree() -> (TempDir, std::path::PathBuf, TempDir) {
            let dir = TempDir::new().unwrap();
            let root = fs::canonicalize(dir.path()).unwrap();
            fs::create_dir_all(root.join("photos")).unwrap();
            fs::write(root.join("photos/img.jpg"), "fake").unwrap();
            fs::write(root.join("readme.txt"), "hello").unwrap();
            let db_dir = TempDir::new().unwrap();
            (dir, root, db_dir)
        }

        #[tokio::test]
        async fn 存在しないscope_node_idで404を返す() {
            let (_dir, root, db_dir) = create_test_tree();
            let state = test_state(&root, db_dir.path());

            let (status, json) =
                get_status_and_code(app(state), "/api/search?q=hello&scope=deadbeef1234").await;

            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(json["code"], "NOT_FOUND");
        }

        #[tokio::test]
        async fn ファイルのnode_idをscopeに指定すると422を返す() {
            let (_dir, root, db_dir) = create_test_tree();
            let state = test_state(&root, db_dir.path());
            let file_id = register_node_id(&state, &root.join("readme.txt"));

            let (status, json) =
                get_status_and_code(app(state), &format!("/api/search?q=hello&scope={file_id}"))
                    .await;

            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(json["code"], "INVALID_QUERY");
        }

        #[tokio::test]
        async fn 有効なディレクトリscopeで200を返す() {
            let (_dir, root, db_dir) = create_test_tree();
            let state = test_state(&root, db_dir.path());
            let dir_id = register_node_id(&state, &root.join("photos"));

            let (status, json) = get_status_and_code(
                app(state),
                &format!("/api/search?q=anything&scope={dir_id}"),
            )
            .await;

            // インデックスは空だが、スコープ検証に通れば 200 で空結果を返す
            assert_eq!(status, StatusCode::OK, "body: {json}");
            assert_eq!(json["results"].as_array().unwrap().len(), 0);
        }

        #[tokio::test]
        async fn 不正なsort値で422を返す() {
            // 既存 kind バリデーションと同じく InvalidQuery=422 を返す（errors.rs に整合）
            let (_dir, root, db_dir) = create_test_tree();
            let state = test_state(&root, db_dir.path());

            let (status, json) =
                get_status_and_code(app(state), "/api/search?q=hello&sort=invalid_sort").await;

            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(json["code"], "INVALID_QUERY");
        }

        #[tokio::test]
        async fn 有効なsort値で200を返す() {
            let (_dir, root, db_dir) = create_test_tree();
            let state = test_state(&root, db_dir.path());

            for sort in &[
                "relevance",
                "name-asc",
                "name-desc",
                "date-asc",
                "date-desc",
            ] {
                let (status, json) = get_status_and_code(
                    app(Arc::clone(&state)),
                    &format!("/api/search?q=hello&sort={sort}"),
                )
                .await;
                assert_eq!(status, StatusCode::OK, "sort={sort} body={json}");
            }
        }

        #[tokio::test]
        async fn next_offsetは結果が足りるときに返り終端でnullになる() {
            let (_dir, root, db_dir) = create_test_tree();
            let state = test_state(&root, db_dir.path());

            // 結果が 0 件 → has_more=false → next_offset 省略
            let (status, json) = get_status_and_code(app(state), "/api/search?q=anything").await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(json["has_more"], false);
            // skip_serializing_if=Option::is_none で None は省略される
            assert!(json.get("next_offset").is_none() || json["next_offset"].is_null());
        }
    }

    // --- POST /api/index/rebuild の排他制御テスト (Phase B) ---
    //
    // rebuild_guard による 409 返却と RAII release の統合検証。
    #[allow(
        non_snake_case,
        reason = "日本語テスト名で振る舞いを記述する規約 (07_testing.md)"
    )]
    mod rebuild_exclusion {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        use axum::Router;
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use axum::routing::post;
        use tempfile::TempDir;
        use tower::ServiceExt;

        use crate::config::Settings;
        use crate::routers::search::rebuild_index;
        use crate::services::archive::ArchiveService;
        use crate::services::dir_index::DirIndex;
        use crate::services::indexer::Indexer;
        use crate::services::node_registry::NodeRegistry;
        use crate::services::path_security::PathSecurity;
        use crate::services::rebuild_guard::RebuildGuard;
        use crate::services::temp_file_cache::TempFileCache;
        use crate::services::thumbnail_inflight::InflightLocks;
        use crate::services::thumbnail_service::ThumbnailService;
        use crate::services::thumbnail_warmer::ThumbnailWarmer;
        use crate::services::video_converter::VideoConverter;
        use crate::state::AppState;

        fn test_state() -> (Arc<AppState>, TempDir, TempDir) {
            let dir = TempDir::new().unwrap();
            let root = std::fs::canonicalize(dir.path()).unwrap();
            let settings = Settings::from_map(&HashMap::from([(
                "MOUNT_BASE_DIR".to_string(),
                root.to_string_lossy().into_owned(),
            )]))
            .unwrap();
            let ps = Arc::new(PathSecurity::new(vec![root.clone()], false).unwrap());
            let mut registry = NodeRegistry::new(Arc::clone(&ps), 100_000, HashMap::new());
            let mut mount_id_map = HashMap::new();
            // mount_id invariant: 16 桁 lowercase hex（indexer::helpers::mount_scope_range 要件）
            mount_id_map.insert("deadbeefcafe0001".to_string(), root.clone());
            registry.set_mount_id_map(mount_id_map);
            let archive_service = Arc::new(ArchiveService::new(&settings));
            let temp_file_cache = Arc::new(
                TempFileCache::new(TempDir::new().unwrap().keep(), 10 * 1024 * 1024).unwrap(),
            );
            let thumbnail_service = Arc::new(ThumbnailService::new(
                Arc::clone(&temp_file_cache),
                InflightLocks::new(),
            ));
            let video_converter =
                Arc::new(VideoConverter::new(Arc::clone(&temp_file_cache), &settings));
            let thumbnail_warmer = Arc::new(ThumbnailWarmer::new(4));
            let db_dir = TempDir::new().unwrap();
            let indexer = Arc::new(Indexer::new(
                db_dir.path().join("index.db").to_str().unwrap(),
            ));
            indexer.init_db().unwrap();
            let dir_index = Arc::new(DirIndex::new(
                db_dir.path().join("dir.db").to_str().unwrap(),
            ));
            dir_index.init_db().unwrap();
            let state = Arc::new(AppState {
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
                registry_populate_stats: Arc::new(
                    crate::services::node_registry::PopulateStats::default(),
                ),
                last_scan_report: Arc::new(std::sync::RwLock::new(None)),
                rebuild_guard: Arc::new(RebuildGuard::new()),
                file_watcher: Arc::new(std::sync::Mutex::new(None)),
                path_security: ps,
                shutdown: crate::state::ShutdownFields::fresh(),
            });
            (state, dir, db_dir)
        }

        fn app(state: Arc<AppState>) -> Router {
            Router::new()
                .route("/api/index/rebuild", post(rebuild_index))
                .with_state(state)
        }

        #[tokio::test]
        async fn rebuildは初回POSTで202を返す() {
            let (state, _dir, _db_dir) = test_state();
            let resp = app(state)
                .oneshot(
                    Request::post("/api/index/rebuild")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::ACCEPTED);
        }

        #[tokio::test]
        async fn rebuild中の再POSTは409を返す() {
            let (state, _dir, _db_dir) = test_state();
            // guard を事前に取得して rebuild 実行中の状態を再現
            let _held = state
                .rebuild_guard
                .try_acquire()
                .expect("初期は未取得のため成功");
            let resp = app(Arc::clone(&state))
                .oneshot(
                    Request::post("/api/index/rebuild")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::CONFLICT);
        }

        #[tokio::test]
        async fn rebuild_guard_releaseで再POSTが202を返す() {
            let (state, _dir, _db_dir) = test_state();
            {
                let _held = state.rebuild_guard.try_acquire().unwrap();
                // スコープ抜けで Drop → release
            }
            let resp = app(Arc::clone(&state))
                .oneshot(
                    Request::post("/api/index/rebuild")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::ACCEPTED);
        }

        /// cold partial init (`scan_complete=false` 固定) 状態から rebuild を発火し、
        /// 非同期 task 完了後に `/api/ready` 用の readiness flag が昇格することを確認
        /// する（Phase C の主目的）。
        ///
        /// `tokio::task::spawn_blocking` を使う関係で `multi_thread` runtime が必要
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn rebuildはcold_partial_init後にscan_completeとdir_index_readyを昇格する() {
            let (state, _dir, _db_dir) = test_state();
            // cold partial 状態を再現: scan_complete=false / dir_index.is_ready=false
            state
                .scan_complete
                .store(false, std::sync::atomic::Ordering::Relaxed);
            assert!(!state.dir_index.is_ready());

            let resp = app(Arc::clone(&state))
                .oneshot(
                    Request::post("/api/index/rebuild")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::ACCEPTED);

            // 非同期 task の完了を polling（合計最大 5 秒）
            for _ in 0..50 {
                if state
                    .scan_complete
                    .load(std::sync::atomic::Ordering::Relaxed)
                    && state.dir_index.is_ready()
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            assert!(
                state
                    .scan_complete
                    .load(std::sync::atomic::Ordering::Relaxed),
                "rebuild 完了後に scan_complete が昇格されなかった"
            );
            assert!(
                state.dir_index.is_ready(),
                "rebuild 完了後に dir_index.is_ready が昇格されなかった"
            );
            // last_scan_report も rebuild diagnostics で更新されているはず
            let report_opt = state.last_scan_report.read().unwrap().as_ref().cloned();
            let report = report_opt.expect("rebuild 完了後に last_scan_report が None のまま");
            assert!(report.all_ok);
            assert!(!report.is_warm_start, "rebuild は cold 経路扱い");
        }
    }
}
