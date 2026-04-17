//! `GET /api/search` ハンドラ
//!
//! 入力バリデーション → scope 解決 → ブロッキング内で検索 + 解決を実行。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};

use crate::errors::AppError;
use crate::services::search::resolve_search_results;
use crate::services::search::resolver::ResolveContext;
use crate::services::search::scope::resolve_scope_prefix;
use crate::state::AppState;

use super::{
    DEFAULT_LIMIT, MAX_LIMIT, MAX_QUERY_LENGTH, MIN_QUERY_LENGTH, SearchQuery, SearchResponse,
    VALID_KINDS,
};

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
    let scope_prefix = resolve_scope_prefix(&state.node_registry, query.scope.as_deref())?;

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
        let ctx = ResolveContext {
            indexer: &indexer,
            registry: &registry,
            mount_id_map: &mount_id_map,
            query: &q_clone,
            kind: kind.as_deref(),
            limit,
            offset,
            scope_prefix: scope_prefix.as_deref(),
        };
        resolve_search_results(&ctx)
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
