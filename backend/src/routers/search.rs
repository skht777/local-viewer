//! 検索 API
//!
//! - `GET /api/search` — キーワード検索 (FTS5 trigram)
//! - `POST /api/index/rebuild` — インデックスリビルド

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::services::indexer::{Indexer, SearchParams};
use crate::services::node_registry::NodeRegistry;
use crate::state::AppState;

// --- 定数 ---

/// 許可される kind フィルタ値
const VALID_KINDS: &[&str] = &["directory", "image", "video", "pdf", "archive"];

/// 検索結果解決の最大リトライ回数
/// (削除済みファイルのスキップにより件数不足になった場合のリトライ)
const MAX_RESOLVE_ITERATIONS: usize = 5;

/// クエリ文字数の最小値
const MIN_QUERY_LENGTH: usize = 2;

/// クエリ文字数の最大値
const MAX_QUERY_LENGTH: usize = 200;

/// デフォルト取得件数
const DEFAULT_LIMIT: usize = 50;

/// 最大取得件数
const MAX_LIMIT: usize = 200;

// --- クエリパラメータ ---

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

// --- レスポンス型 ---

/// 検索結果の 1 件
#[derive(Debug, Serialize)]
pub(crate) struct SearchResultResponse {
    pub node_id: String,
    pub parent_node_id: Option<String>,
    pub name: String,
    pub kind: String,
    pub relative_path: String,
    pub size_bytes: Option<i64>,
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
struct RebuildResponse {
    message: String,
}

// --- ハンドラ ---

/// `GET /api/search`
///
/// FTS5 trigram でキーワード検索を実行する。
/// 検索結果のパスを `NodeRegistry` 経由で `node_id` に解決し、
/// 削除済みファイルはスキップする。
pub(crate) async fn search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, AppError> {
    // クエリ文字数バリデーション (2-200 文字)
    let q = query.q.trim().to_owned();
    let char_count = q.chars().count();
    if !(MIN_QUERY_LENGTH..=MAX_QUERY_LENGTH).contains(&char_count) {
        return Err(AppError::InvalidQuery(format!(
            "クエリは{MIN_QUERY_LENGTH}文字以上{MAX_QUERY_LENGTH}文字以下で指定してください"
        )));
    }

    // kind バリデーション
    if let Some(ref kind) = query.kind {
        if !VALID_KINDS.contains(&kind.as_str()) {
            return Err(AppError::InvalidQuery(format!(
                "無効な kind です: {kind} (有効値: {})",
                VALID_KINDS.join(", ")
            )));
        }
    }

    // インデックス準備状態チェック
    if !state.indexer.is_ready() {
        return Err(AppError::IndexNotReady(
            "インデックスが準備中です".to_string(),
        ));
    }

    // limit/offset のデフォルト値・上限補正
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = query.offset.unwrap_or(0);

    // scope node_id → ディレクトリプレフィックスの解決
    let scope_prefix = if let Some(ref scope_node_id) = query.scope {
        #[allow(
            clippy::expect_used,
            reason = "Mutex poison は致命的エラー、パニックが適切"
        )]
        let reg = state
            .node_registry
            .lock()
            .expect("NodeRegistry Mutex poisoned");

        // node_id → 絶対パス
        let abs_path = reg.resolve(scope_node_id)?.to_path_buf();

        // PathSecurity で存在確認 + マウントポイント配下か検証
        reg.path_security().validate_existing(&abs_path)?;

        // ディレクトリか確認
        if !abs_path.is_dir() {
            return Err(AppError::InvalidQuery(
                "scope はディレクトリの node_id を指定してください".to_string(),
            ));
        }

        // {mount_id}/{relative} プレフィックスを算出
        reg.compute_parent_path_key(&abs_path)
    } else {
        None
    };

    // mount_id_map をクローン (不変データ、ロック時間を最小化)
    let mount_id_map = {
        #[allow(
            clippy::expect_used,
            reason = "Mutex poison は致命的エラー、パニックが適切"
        )]
        let reg = state
            .node_registry
            .lock()
            .expect("NodeRegistry Mutex poisoned");
        reg.mount_id_map().clone()
    };

    let indexer = Arc::clone(&state.indexer);
    let registry = Arc::clone(&state.node_registry);
    let kind = query.kind.clone();
    let q_clone = q.clone();
    let is_stale = state.indexer.is_stale();

    // ブロッキング処理を spawn_blocking で実行
    let (results, has_more) = tokio::task::spawn_blocking(move || {
        resolve_search_results(
            &indexer,
            &registry,
            &mount_id_map,
            &q_clone,
            kind.as_deref(),
            limit,
            offset,
            scope_prefix.as_deref(),
        )
    })
    .await
    .map_err(|e| AppError::path_security(format!("タスク実行エラー: {e}")))??;

    Ok(Json(SearchResponse {
        results,
        has_more,
        query: q,
        is_stale,
    }))
}

/// `POST /api/index/rebuild`
///
/// インデックスの全件リビルドをバックグラウンドで開始する。
/// 同時実行制御 + レート制限付き。
pub(crate) async fn rebuild_index(
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    // リビルド実行中チェック
    if state.indexer.is_rebuilding() {
        return Err(AppError::RebuildInProgress(
            "リビルドが実行中です".to_string(),
        ));
    }

    // レート制限チェック
    {
        let mut last = state.last_rebuild.lock().await;
        if let Some(instant) = *last {
            let elapsed = instant.elapsed().as_secs();
            if elapsed < state.settings.rebuild_rate_limit_seconds {
                return Err(AppError::RateLimited("レート制限に達しました".to_string()));
            }
        }
        *last = Some(tokio::time::Instant::now());
    }

    // バックグラウンドでリビルドを実行
    let indexer = Arc::clone(&state.indexer);
    let registry = Arc::clone(&state.node_registry);

    // mount_id_map からリビルド対象のルートを収集
    let mount_entries: Vec<(String, PathBuf)> = {
        #[allow(
            clippy::expect_used,
            reason = "Mutex poison は致命的エラー、パニックが適切"
        )]
        let reg = registry.lock().expect("NodeRegistry Mutex poisoned");
        reg.mount_id_map()
            .iter()
            .map(|(id, root)| (id.clone(), root.clone()))
            .collect()
    };

    tokio::spawn(async move {
        for (mount_id, root) in &mount_entries {
            let indexer_ref = Arc::clone(&indexer);
            let registry_ref = Arc::clone(&registry);
            let root = root.clone();
            let mount_id_for_task = mount_id.clone();
            let mount_id_for_log = mount_id.clone();

            let result = tokio::task::spawn_blocking(move || {
                #[allow(
                    clippy::expect_used,
                    reason = "Mutex poison は致命的エラー、パニックが適切"
                )]
                let reg = registry_ref.lock().expect("NodeRegistry Mutex poisoned");
                let path_security = reg.path_security();
                indexer_ref.rebuild(&root, path_security, &mount_id_for_task)
            })
            .await;

            match result {
                Ok(Ok(count)) => {
                    tracing::info!(
                        "インデックスリビルド完了: mount_id={mount_id_for_log}, entries={count}"
                    );
                }
                Ok(Err(e)) => {
                    tracing::error!(
                        "インデックスリビルドエラー: mount_id={mount_id_for_log}, error={e}"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "リビルドタスク実行エラー: mount_id={mount_id_for_log}, error={e}"
                    );
                }
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(RebuildResponse {
            message: "リビルドを開始しました".to_string(),
        }),
    )
        .into_response())
}

// --- 内部関数 ---

/// 検索結果を解決する (ブロッキング処理)
///
/// - `Indexer` の検索結果を absolute path に解決
/// - `PathSecurity` で存在確認
/// - `NodeRegistry` で `node_id` を生成
/// - 削除されたファイルはスキップし、limit を満たすまで最大5回リトライ
#[allow(
    clippy::too_many_arguments,
    reason = "検索解決に必要なコンテキストを受け取る内部ヘルパー"
)]
fn resolve_search_results(
    indexer: &Indexer,
    registry: &Arc<Mutex<NodeRegistry>>,
    mount_id_map: &HashMap<String, PathBuf>,
    query: &str,
    kind: Option<&str>,
    limit: usize,
    offset: usize,
    scope_prefix: Option<&str>,
) -> Result<(Vec<SearchResultResponse>, bool), AppError> {
    let mut results = Vec::new();
    let mut db_offset = offset;
    // DB から多めに取得して削除済みファイルのスキップに備える
    let collect_limit = (limit + 1) * 2;

    for _ in 0..MAX_RESOLVE_ITERATIONS {
        let (hits, _) = indexer
            .search(&SearchParams {
                query,
                kind,
                limit: collect_limit,
                offset: db_offset,
                scope_prefix,
            })
            .map_err(|e| AppError::path_security(format!("検索エラー: {e}")))?;

        let hit_count = hits.len();

        for hit in hits {
            // `relative_path` から `mount_id` と実際の相対パスを分解
            // format: "{mount_id}/{actual_relative_path}"
            let Some((mount_root, actual_relative)) =
                resolve_mount_path(&hit.relative_path, mount_id_map)
            else {
                continue;
            };

            let abs_path = mount_root.join(&actual_relative);

            // `PathSecurity` で存在確認 + `NodeRegistry` で `node_id` 生成 (短ロック)
            #[allow(
                clippy::expect_used,
                reason = "Mutex poison は致命的エラー、パニックが適切"
            )]
            let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");

            // 存在しないファイルはスキップ
            if reg.path_security().validate_existing(&abs_path).is_err() {
                continue;
            }

            let Ok(node_id) = reg.register_resolved(&abs_path) else {
                continue;
            };
            let parent_node_id = reg.get_parent_node_id(&abs_path);

            // クライアント向け `relative_path` (`mount_id` プレフィックスを除去)
            results.push(SearchResultResponse {
                node_id,
                parent_node_id,
                name: hit.name,
                kind: hit.kind,
                relative_path: actual_relative,
                size_bytes: hit.size_bytes,
            });

            // limit + 1 件集まったら終了 (`has_more` 判定用)
            if results.len() > limit {
                break;
            }
        }

        // 十分な件数が集まった場合
        if results.len() > limit {
            break;
        }

        // DB にこれ以上の結果がない場合
        if hit_count < collect_limit {
            break;
        }

        // リトライ: DB オフセットを進めて再取得
        db_offset += collect_limit;
    }

    let has_more = results.len() > limit;
    results.truncate(limit);

    Ok((results, has_more))
}

/// `relative_path` から `mount_id` とマウントルート + 実際の相対パスを解決する
///
/// `relative_path` の format: `{mount_id}/{actual_relative_path}`
/// `mount_id` が `mount_id_map` に存在しない場合は `None` を返す
fn resolve_mount_path(
    relative_path: &str,
    mount_id_map: &HashMap<String, PathBuf>,
) -> Option<(PathBuf, String)> {
    // 最初の '/' で分割
    let (mount_id, actual_relative) = if let Some(pos) = relative_path.find('/') {
        (&relative_path[..pos], &relative_path[pos + 1..])
    } else {
        // '/' がない場合は `mount_id` のみ (ルートディレクトリ自体)
        (relative_path, "")
    };

    let mount_root = mount_id_map.get(mount_id)?;
    Some((mount_root.clone(), actual_relative.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- resolve_mount_path テスト ---

    #[test]
    fn mount_idとrelative_pathが正しく分解される() {
        let mut map = HashMap::new();
        map.insert("pictures".to_string(), PathBuf::from("/mnt/data/pictures"));

        let result = resolve_mount_path("pictures/photos/sunset.jpg", &map);
        assert!(result.is_some());
        let (root, rel) = result.unwrap();
        assert_eq!(root, PathBuf::from("/mnt/data/pictures"));
        assert_eq!(rel, "photos/sunset.jpg");
    }

    #[test]
    fn mount_idのみの場合はrelative_pathが空文字になる() {
        let mut map = HashMap::new();
        map.insert("pictures".to_string(), PathBuf::from("/mnt/data/pictures"));

        let result = resolve_mount_path("pictures", &map);
        assert!(result.is_some());
        let (root, rel) = result.unwrap();
        assert_eq!(root, PathBuf::from("/mnt/data/pictures"));
        assert_eq!(rel, "");
    }

    #[test]
    fn 存在しないmount_idでnoneを返す() {
        let map = HashMap::new();
        let result = resolve_mount_path("unknown/file.jpg", &map);
        assert!(result.is_none());
    }

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
