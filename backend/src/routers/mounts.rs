//! マウントポイント一覧 API
//!
//! `GET /api/mounts` — 全マウントポイントを返す

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::errors::AppError;
use crate::state::AppState;

#[derive(Serialize)]
struct MountEntryResponse {
    mount_id: String,
    name: String,
    node_id: String,
    child_count: Option<usize>,
}

#[derive(Serialize)]
pub(crate) struct MountListResponse {
    mounts: Vec<MountEntryResponse>,
}

/// `GET /api/mounts`
///
/// 全マウントルートを一覧で返す。
/// `mount_id` は現在 `node_id` と同値。
pub(crate) async fn list_mounts(
    State(state): State<Arc<AppState>>,
) -> Result<Json<MountListResponse>, AppError> {
    let registry = Arc::clone(&state.node_registry);

    let entries = tokio::task::spawn_blocking(move || {
        #[allow(
            clippy::expect_used,
            reason = "Mutex poison は致命的エラー、パニックが適切"
        )]
        let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");
        reg.list_mount_roots()
    })
    .await
    .map_err(|e| AppError::path_security(format!("タスク実行エラー: {e}")))?;

    let mounts = entries
        .into_iter()
        .map(|e| MountEntryResponse {
            mount_id: e.node_id.clone(),
            name: e.name,
            node_id: e.node_id,
            child_count: e.child_count,
        })
        .collect();

    Ok(Json(MountListResponse { mounts }))
}

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
    use crate::services::temp_file_cache::TempFileCache;
    use crate::services::thumbnail_service::ThumbnailService;
    use crate::services::thumbnail_warmer::ThumbnailWarmer;
    use crate::services::video_converter::VideoConverter;

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
        Arc::new(AppState {
            settings: Arc::new(settings),
            node_registry: Arc::new(Mutex::new(registry)),
            archive_service,
            temp_file_cache,
            thumbnail_service,
            video_converter,
            thumbnail_warmer,
            indexer,
            last_rebuild: tokio::sync::Mutex::new(None),
        })
    }

    fn app(state: Arc<AppState>) -> Router {
        Router::new()
            .route("/api/mounts", get(list_mounts))
            .with_state(state)
    }

    async fn get_json(app: Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let resp = app
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (status, json)
    }

    #[tokio::test]
    async fn マウントポイント一覧が正しく返る() {
        let dir = TempDir::new().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::write(root.join("file.txt"), "hello").unwrap();
        let state = test_state(&root, HashMap::new());
        let (status, json) = get_json(app(state), "/api/mounts").await;
        assert_eq!(status, StatusCode::OK);
        let mounts = json["mounts"].as_array().unwrap();
        assert_eq!(mounts.len(), 1);
    }

    #[tokio::test]
    async fn mount_idとnode_idが一致する() {
        let dir = TempDir::new().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let state = test_state(&root, HashMap::new());
        let (_, json) = get_json(app(state), "/api/mounts").await;
        let m = &json["mounts"][0];
        assert_eq!(m["mount_id"], m["node_id"]);
    }

    #[tokio::test]
    async fn mount_namesが反映される() {
        let dir = TempDir::new().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let mut names = HashMap::new();
        names.insert(root.clone(), "My Pictures".to_string());
        let state = test_state(&root, names);
        let (_, json) = get_json(app(state), "/api/mounts").await;
        assert_eq!(json["mounts"][0]["name"], "My Pictures");
    }

    #[tokio::test]
    async fn child_countが含まれる() {
        let dir = TempDir::new().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::write(root.join("a.jpg"), "img").unwrap();
        fs::write(root.join("b.png"), "img").unwrap();
        let state = test_state(&root, HashMap::new());
        let (_, json) = get_json(app(state), "/api/mounts").await;
        assert_eq!(json["mounts"][0]["child_count"], 2);
    }
}
