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
}

/// 検索 API のレスポンス
#[derive(Debug, Serialize)]
pub(crate) struct SearchResponse {
    pub results: Vec<SearchResultResponse>,
    pub has_more: bool,
    pub query: String,
    pub is_stale: bool,
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
            let mut registry = NodeRegistry::new(ps, 100_000, HashMap::new());
            let mut mount_id_map = HashMap::new();
            mount_id_map.insert("testmount".to_string(), root.to_path_buf());
            registry.set_mount_id_map(mount_id_map);
            let archive_service = Arc::new(ArchiveService::new(&settings));
            let temp_file_cache = Arc::new(
                TempFileCache::new(TempDir::new().unwrap().keep(), 10 * 1024 * 1024).unwrap(),
            );
            let thumbnail_service = Arc::new(ThumbnailService::new(Arc::clone(&temp_file_cache)));
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
    }
}
