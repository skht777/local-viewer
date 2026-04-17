//! 検索結果を `NodeRegistry` 経由で `node_id` に解決する (ブロッキング処理)
//!
//! - `Indexer` の検索結果 (`{mount_id}/{relative_path}`) を絶対パスに復元
//! - `PathSecurity::validate_existing` で存在確認（削除済みはスキップ）
//! - `NodeRegistry::register_resolved` で `node_id` 生成
//! - 削除によって件数不足になった場合は DB オフセットを進めて最大 5 回までリトライ

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::errors::AppError;
use crate::services::indexer::{Indexer, SearchParams};
use crate::services::node_registry::NodeRegistry;

/// 検索結果解決の最大リトライ回数
/// (削除済みファイルのスキップにより件数不足になった場合のリトライ)
const MAX_RESOLVE_ITERATIONS: usize = 5;

/// 検索結果の 1 件 (API レスポンスにそのまま載せる)
#[derive(Debug, Serialize)]
pub(crate) struct SearchResultResponse {
    pub node_id: String,
    pub parent_node_id: Option<String>,
    pub name: String,
    pub kind: String,
    pub relative_path: String,
    pub size_bytes: Option<i64>,
}

/// `resolve_search_results` の引数コンテナ
pub(crate) struct ResolveContext<'a> {
    pub indexer: &'a Indexer,
    pub registry: &'a Arc<Mutex<NodeRegistry>>,
    pub mount_id_map: &'a HashMap<String, PathBuf>,
    pub query: &'a str,
    pub kind: Option<&'a str>,
    pub limit: usize,
    pub offset: usize,
    pub scope_prefix: Option<&'a str>,
}

/// 検索結果を解決する
pub(crate) fn resolve_search_results(
    ctx: &ResolveContext<'_>,
) -> Result<(Vec<SearchResultResponse>, bool), AppError> {
    let mut results = Vec::new();
    let mut db_offset = ctx.offset;
    // DB から多めに取得して削除済みファイルのスキップに備える
    let collect_limit = (ctx.limit + 1) * 2;

    for _ in 0..MAX_RESOLVE_ITERATIONS {
        let (hits, _) = ctx
            .indexer
            .search(&SearchParams {
                query: ctx.query,
                kind: ctx.kind,
                limit: collect_limit,
                offset: db_offset,
                scope_prefix: ctx.scope_prefix,
            })
            .map_err(|e| AppError::path_security(format!("検索エラー: {e}")))?;

        let hit_count = hits.len();

        for hit in hits {
            // `relative_path` から `mount_id` と実際の相対パスを分解
            // format: "{mount_id}/{actual_relative_path}"
            let Some((mount_root, actual_relative)) =
                resolve_mount_path(&hit.relative_path, ctx.mount_id_map)
            else {
                continue;
            };

            let abs_path = mount_root.join(&actual_relative);

            // `PathSecurity` で存在確認 + `NodeRegistry` で `node_id` 生成 (短ロック)
            #[allow(
                clippy::expect_used,
                reason = "Mutex poison は致命的エラー、パニックが適切"
            )]
            let mut reg = ctx.registry.lock().expect("NodeRegistry Mutex poisoned");

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

            // `limit + 1` 件集まったら終了 (`has_more` 判定用)
            if results.len() > ctx.limit {
                break;
            }
        }

        // 十分な件数が集まった場合
        if results.len() > ctx.limit {
            break;
        }

        // DB にこれ以上の結果がない場合
        if hit_count < collect_limit {
            break;
        }

        // リトライ: DB オフセットを進めて再取得
        db_offset += collect_limit;
    }

    let has_more = results.len() > ctx.limit;
    results.truncate(ctx.limit);

    Ok((results, has_more))
}

/// `relative_path` から `mount_id` とマウントルート + 実際の相対パスを解決する
///
/// `relative_path` の format: `{mount_id}/{actual_relative_path}`
/// `mount_id` が `mount_id_map` に存在しない場合は `None` を返す
pub(crate) fn resolve_mount_path(
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
}
