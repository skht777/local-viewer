//! ディレクトリ閲覧 API
//!
//! - `GET /api/browse/{node_id}` — ディレクトリ一覧 (ページネーション + `ETag` + 304)
//! - `GET /api/browse/{node_id}/first-viewable` — 再帰的に最初の閲覧対象を探索
//! - `GET /api/browse/{parent_node_id}/sibling` — 次/前の兄弟セットを返す

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::services::browse_cursor::{self, MAX_LIMIT, SortOrder};
use crate::services::extensions::EntryKind;
use crate::services::models::{AncestorEntry, BrowseResponse, EntryMeta};
use crate::services::node_registry::{NodeRegistry, PageOptions};
use crate::state::AppState;

// --- クエリパラメータ ---

/// `GET /api/browse/{node_id}` のクエリパラメータ
#[derive(Debug, Deserialize)]
pub(crate) struct BrowseQuery {
    /// ソート順 (デフォルト: name-asc)
    #[serde(default = "default_sort")]
    pub sort: SortOrder,
    /// 1 ページあたりの件数 (省略時: 全件返却)
    pub limit: Option<usize>,
    /// ページネーションカーソル
    pub cursor: Option<String>,
}

fn default_sort() -> SortOrder {
    SortOrder::NameAsc
}

/// `GET /api/browse/{node_id}/first-viewable` のクエリパラメータ
#[derive(Debug, Deserialize)]
pub(crate) struct FirstViewableQuery {
    #[serde(default = "default_sort")]
    pub sort: SortOrder,
}

/// `GET /api/browse/{parent_node_id}/sibling` のクエリパラメータ
#[derive(Debug, Deserialize)]
pub(crate) struct SiblingQuery {
    /// 現在のエントリの `node_id`
    pub current: String,
    /// "next" or "prev"
    pub direction: String,
    #[serde(default = "default_sort")]
    pub sort: SortOrder,
}

// --- レスポンス型 ---

/// first-viewable API のレスポンス
#[derive(Debug, Serialize)]
pub(crate) struct FirstViewableResponse {
    pub entry: Option<EntryMeta>,
    pub parent_node_id: Option<String>,
}

/// sibling API のレスポンス
#[derive(Debug, Serialize)]
pub(crate) struct SiblingResponse {
    pub entry: Option<EntryMeta>,
}

// --- ETag 計算 ---

/// エントリ一覧から `ETag` (MD5 hex) を計算する
///
/// `node_id,name,kind,size_bytes,child_count,modified_at` を `|` 区切りで連結し、
/// MD5 ハッシュを生成する。Python 版と同一のロジック。
fn compute_etag(entries: &[EntryMeta]) -> String {
    let mut hasher = Md5::new();
    for (i, e) in entries.iter().enumerate() {
        if i > 0 {
            hasher.update(b"|");
        }
        // Python: f"{e.node_id},{e.name},{e.kind},{e.size_bytes},{e.child_count},{e.modified_at}"
        let fragment = format!(
            "{},{},{},{},{},{}",
            e.node_id,
            e.name,
            serde_json::to_string(&e.kind)
                .unwrap_or_default()
                .trim_matches('"'),
            e.size_bytes
                .map_or_else(|| "None".to_string(), |v| v.to_string()),
            e.child_count
                .map_or_else(|| "None".to_string(), |v| v.to_string()),
            e.modified_at
                .map_or_else(|| "None".to_string(), |v| format!("{v}")),
        );
        hasher.update(fragment.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

// --- ハンドラ ---

/// `GET /api/browse/{node_id}`
///
/// ディレクトリ一覧を返す。`ETag` + 304 対応。
/// カーソルベースページネーション (limit + cursor + sort)。
pub(crate) async fn browse_directory(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
    Query(query): Query<BrowseQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    // limit のバリデーション (1..=500)
    if let Some(limit) = query.limit {
        if limit == 0 || limit > MAX_LIMIT {
            return Err(AppError::InvalidCursor(format!(
                "limit は 1 以上 {MAX_LIMIT} 以下で指定してください"
            )));
        }
    }

    let registry = Arc::clone(&state.node_registry);
    let sort = query.sort;
    let limit = query.limit;
    let cursor = query.cursor.clone();

    // spawn_blocking 内でファイルシステム操作を実行
    let (response, etag) = tokio::task::spawn_blocking(move || {
        #[allow(
            clippy::expect_used,
            reason = "Mutex poison は致命的エラー、パニックが適切"
        )]
        let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");
        browse_directory_blocking(&mut reg, &node_id, sort, limit, cursor.as_deref())
    })
    .await
    .map_err(|e| AppError::path_security(format!("タスク実行エラー: {e}")))??;

    // `ETag` 比較 → 304 Not Modified
    if let Some(if_none_match) = headers.get("if-none-match") {
        if let Ok(value) = if_none_match.to_str() {
            if value == etag {
                return Ok((
                    StatusCode::NOT_MODIFIED,
                    [
                        ("etag", etag.as_str()),
                        ("cache-control", "private, no-cache"),
                    ],
                )
                    .into_response());
            }
        }
    }

    // JSON レスポンス + `ETag` + Cache-Control ヘッダ
    Ok((
        [
            ("etag", etag.as_str()),
            ("cache-control", "private, no-cache"),
        ],
        Json(response),
    )
        .into_response())
}

/// `browse_directory` のブロッキング処理本体
///
/// ディレクトリの検証、エントリ取得、ソート/ページネーション、パンくずリスト構築を行う。
fn browse_directory_blocking(
    reg: &mut NodeRegistry,
    node_id: &str,
    sort: SortOrder,
    limit: Option<usize>,
    cursor: Option<&str>,
) -> Result<(BrowseResponse, String), AppError> {
    // node_id → パス解決
    let path = reg.resolve(node_id)?.to_path_buf();

    // アーカイブエントリのチェック
    if reg.is_archive_entry(node_id) {
        return Err(AppError::NotADirectory {
            path: format!("アーカイブエントリは browse 対象外です: {node_id}"),
        });
    }

    // ディレクトリかチェック
    if !path.is_dir() {
        return Err(AppError::NotADirectory {
            path: path.display().to_string(),
        });
    }

    let (page_entries, next_cursor, total_count, etag) =
        fetch_page(reg, &path, sort, limit, cursor)?;

    // パンくずリスト用の parent_node_id と ancestors
    let parent_node_id = reg.get_parent_node_id(&path);
    let ancestors = reg
        .get_ancestors(&path)
        .into_iter()
        .map(|(nid, name)| AncestorEntry { node_id: nid, name })
        .collect();

    let response = BrowseResponse {
        current_node_id: Some(node_id.to_string()),
        current_name: path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().to_string()),
        parent_node_id,
        ancestors,
        entries: page_entries,
        next_cursor,
        total_count: if limit.is_some() {
            Some(total_count)
        } else {
            None
        },
    };

    Ok((response, etag))
}

/// ディレクトリエントリを取得し、ソート/ページネーションを適用する
///
/// name ソート + limit 指定時は `list_directory_page` で最適化。
/// それ以外は全件取得 + `paginate` で処理。
#[allow(
    clippy::type_complexity,
    reason = "ページネーション結果のタプルが自然な構造"
)]
fn fetch_page(
    reg: &mut NodeRegistry,
    path: &std::path::Path,
    sort: SortOrder,
    limit: Option<usize>,
    cursor: Option<&str>,
) -> Result<(Vec<EntryMeta>, Option<String>, usize, String), AppError> {
    let is_name_sort = matches!(sort, SortOrder::NameAsc | SortOrder::NameDesc);

    if is_name_sort && limit.is_some() {
        fetch_page_name_sort(reg, path, sort, limit.unwrap_or(0), cursor)
    } else {
        fetch_page_full(reg, path, sort, limit, cursor)
    }
}

/// name ソート + limit 指定時: `list_directory_page` で必要分だけ stat する最適化パス
#[allow(
    clippy::type_complexity,
    reason = "ページネーション結果のタプルが自然な構造"
)]
fn fetch_page_name_sort(
    reg: &mut NodeRegistry,
    path: &std::path::Path,
    sort: SortOrder,
    limit_val: usize,
    cursor: Option<&str>,
) -> Result<(Vec<EntryMeta>, Option<String>, usize, String), AppError> {
    // カーソルから node_id を抽出
    let cursor_node_id = cursor
        .map(|c| browse_cursor::decode_cursor(c, sort).map(|d| d.node_id))
        .transpose()?;

    let options = PageOptions {
        limit: limit_val + 1, // +1 で次ページ有無を判定
        cursor_node_id: cursor_node_id.as_deref(),
        reverse: sort == SortOrder::NameDesc,
    };

    let (all_entries, total) = reg.list_directory_page(path, &options)?;
    let has_next = all_entries.len() > limit_val;
    let page: Vec<EntryMeta> = all_entries.into_iter().take(limit_val).collect();

    let etag = compute_etag(&page);
    let next = if has_next {
        page.last()
            .map(|last| browse_cursor::encode_cursor(sort, last, &etag))
    } else {
        None
    };

    Ok((page, next, total, etag))
}

/// date ソート or limit なし: 全件取得してからページネーション
#[allow(
    clippy::type_complexity,
    reason = "ページネーション結果のタプルが自然な構造"
)]
fn fetch_page_full(
    reg: &mut NodeRegistry,
    path: &std::path::Path,
    sort: SortOrder,
    limit: Option<usize>,
    cursor: Option<&str>,
) -> Result<(Vec<EntryMeta>, Option<String>, usize, String), AppError> {
    let entries = reg.list_directory(path)?;
    let total = entries.len();

    if let Some(limit_val) = limit {
        let (page, next, _) = browse_cursor::paginate(entries, sort, Some(limit_val), cursor, "")?;
        let etag = compute_etag(&page);
        // etag 更新後にカーソルを再生成
        let next = if next.is_some() {
            page.last()
                .map(|last| browse_cursor::encode_cursor(sort, last, &etag))
        } else {
            None
        };
        Ok((page, next, total, etag))
    } else {
        // limit なし: ソートのみ
        let sorted = browse_cursor::sort_entries(entries, sort);
        let etag = compute_etag(&sorted);
        Ok((sorted, None, total, etag))
    }
}

/// `GET /api/browse/{node_id}/first-viewable`
///
/// ディレクトリ内の最初の閲覧対象を再帰的に探索する。
/// 優先順位: archive > pdf > image > directory (再帰降下)
/// 最大 10 レベルまで再帰。
pub(crate) async fn first_viewable(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
    Query(query): Query<FirstViewableQuery>,
) -> Result<Json<FirstViewableResponse>, AppError> {
    let registry = Arc::clone(&state.node_registry);
    let sort = query.sort;

    let result = tokio::task::spawn_blocking(move || {
        #[allow(
            clippy::expect_used,
            reason = "Mutex poison は致命的エラー、パニックが適切"
        )]
        let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");

        let max_depth = 10;
        let mut current_id = node_id;

        for _ in 0..max_depth {
            let path = reg.resolve(&current_id)?.to_path_buf();
            if !path.is_dir() {
                break;
            }

            let entries = reg.list_directory(&path)?;
            let sorted = browse_cursor::sort_entries(entries, sort);
            let viewable = select_first_viewable(&sorted);

            let Some(entry) = viewable else {
                return Ok(FirstViewableResponse {
                    entry: None,
                    parent_node_id: None,
                });
            };

            // archive, pdf, image は直接返す
            if matches!(
                entry.kind,
                EntryKind::Archive | EntryKind::Pdf | EntryKind::Image
            ) {
                return Ok(FirstViewableResponse {
                    entry: Some(entry.clone()),
                    parent_node_id: Some(current_id),
                });
            }

            // directory → 再帰降下
            current_id.clone_from(&entry.node_id);
        }

        Ok(FirstViewableResponse {
            entry: None,
            parent_node_id: None,
        })
    })
    .await
    .map_err(|e| AppError::path_security(format!("タスク実行エラー: {e}")))?;

    Ok(Json(result?))
}

/// ソート済みエントリから最初の閲覧対象を選ぶ
///
/// 優先順位: archive > pdf > image > directory (再帰降下用)
fn select_first_viewable(entries: &[EntryMeta]) -> Option<&EntryMeta> {
    for kind in [EntryKind::Archive, EntryKind::Pdf, EntryKind::Image] {
        if let Some(entry) = entries.iter().find(|e| e.kind == kind) {
            return Some(entry);
        }
    }
    // 閲覧対象なし → directory を探す (再帰降下用)
    entries.iter().find(|e| e.kind == EntryKind::Directory)
}

/// `GET /api/browse/{parent_node_id}/sibling`
///
/// 次または前の兄弟セット (directory/archive/pdf) を返す。
pub(crate) async fn find_sibling(
    State(state): State<Arc<AppState>>,
    Path(parent_node_id): Path<String>,
    Query(query): Query<SiblingQuery>,
) -> Result<Json<SiblingResponse>, AppError> {
    let registry = Arc::clone(&state.node_registry);
    let sort = query.sort;
    let current = query.current;
    let direction = query.direction;

    let result = tokio::task::spawn_blocking(move || {
        #[allow(
            clippy::expect_used,
            reason = "Mutex poison は致命的エラー、パニックが適切"
        )]
        let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");

        // 親ディレクトリのパスを解決
        let parent_path = reg.resolve(&parent_node_id)?.to_path_buf();
        if !parent_path.is_dir() {
            return Err(AppError::NotADirectory {
                path: parent_path.display().to_string(),
            });
        }

        // 全件取得してソート
        let entries = reg.list_directory(&parent_path)?;
        let sorted = browse_cursor::sort_entries(entries, sort);

        // 閲覧可能なエントリ (directory, archive, pdf) のみフィルタ
        let candidates: Vec<&EntryMeta> = sorted
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    EntryKind::Directory | EntryKind::Archive | EntryKind::Pdf
                )
            })
            .collect();

        // 現在のエントリを検索
        let current_idx = candidates.iter().position(|e| e.node_id == current);
        let Some(idx) = current_idx else {
            return Ok(SiblingResponse { entry: None });
        };

        // 方向に応じて隣接エントリを返す
        let sibling = match direction.as_str() {
            "next" => {
                if idx + 1 < candidates.len() {
                    Some(candidates[idx + 1].clone())
                } else {
                    None
                }
            }
            "prev" => {
                if idx > 0 {
                    Some(candidates[idx - 1].clone())
                } else {
                    None
                }
            }
            _ => None,
        };

        Ok(SiblingResponse { entry: sibling })
    })
    .await
    .map_err(|e| AppError::path_security(format!("タスク実行エラー: {e}")))?;

    Ok(Json(result?))
}

// --- テスト ---

#[cfg(test)]
mod tests {
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
    use crate::services::node_registry::NodeRegistry;
    use crate::services::path_security::PathSecurity;

    // --- テストヘルパー ---

    fn test_state(
        root: &std::path::Path,
        mount_names: HashMap<std::path::PathBuf, String>,
    ) -> Arc<AppState> {
        let settings = Settings::from_map(&HashMap::from([(
            "MOUNT_BASE_DIR".to_string(),
            root.to_string_lossy().to_string(),
        )]))
        .unwrap();
        let ps = Arc::new(PathSecurity::new(vec![root.to_path_buf()], false).unwrap());
        let registry = NodeRegistry::new(ps, 100_000, mount_names);
        let archive_service = Arc::new(crate::services::archive::ArchiveService::new(&settings));
        Arc::new(AppState {
            settings: Arc::new(settings),
            node_registry: Arc::new(Mutex::new(registry)),
            archive_service,
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
            preview_node_ids: None,
        }];
        assert_ne!(compute_etag(&entries_a), compute_etag(&entries_b));
    }
}
