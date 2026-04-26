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
use crate::services::dir_index::DirIndex;
use crate::services::extensions::{extract_extension, mime_for_extension};
use crate::services::indexer::{Indexer, SearchHit, SearchOrder, SearchParams};
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
    /// 更新日時（Unix epoch 秒、`mtime_ns` を 1e9 で割った値）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<i64>,
    /// MIME タイプ（ファイルのみ、directory/archive は `None`）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// ディレクトリ/アーカイブの子要素数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_count: Option<u32>,
    /// ディレクトリ/アーカイブのプレビュー画像 `node_id` 一覧（最大 4 件）
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub preview_node_ids: Vec<String>,
    /// 結果の親 path key（後段 enrich 用、レスポンスには出さない）
    #[serde(skip)]
    pub parent_path_key: Option<String>,
}

/// `resolve_search_results` の引数コンテナ
pub(crate) struct ResolveContext<'a> {
    pub indexer: &'a Indexer,
    pub dir_index: &'a DirIndex,
    pub registry: &'a Arc<Mutex<NodeRegistry>>,
    pub mount_id_map: &'a HashMap<String, PathBuf>,
    pub query: &'a str,
    pub kind: Option<&'a str>,
    pub limit: usize,
    pub offset: usize,
    pub scope_prefix: Option<&'a str>,
    pub order: SearchOrder,
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
                order: ctx.order,
            })
            .map_err(|e| AppError::path_security(format!("検索エラー: {e}")))?;

        let hit_count = hits.len();

        for hit in hits {
            if let Some(resolved) = build_response(hit, ctx) {
                results.push(resolved);
                // `limit + 1` 件集まったら終了 (`has_more` 判定用)
                if results.len() > ctx.limit {
                    break;
                }
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

    // ディレクトリ/アーカイブ結果を `batch_dir_info` で 1 回呼び出して child_count + preview を埋める
    enrich_directory_results(&mut results, ctx)?;

    Ok((results, has_more))
}

/// 1 件の `SearchHit` を解決して `SearchResultResponse` に変換する
fn build_response(hit: SearchHit, ctx: &ResolveContext<'_>) -> Option<SearchResultResponse> {
    let (mount_root, actual_relative) = resolve_mount_path(&hit.relative_path, ctx.mount_id_map)?;
    let abs_path = mount_root.join(&actual_relative);

    let (node_id, parent_node_id) = {
        #[allow(
            clippy::expect_used,
            reason = "Mutex poison は致命的エラー、パニックが適切"
        )]
        let mut reg = ctx.registry.lock().expect("NodeRegistry Mutex poisoned");
        if reg.path_security().validate_existing(&abs_path).is_err() {
            return None;
        }
        let node_id = reg.register_resolved(&abs_path).ok()?;
        let parent = reg.get_parent_node_id(&abs_path);
        (node_id, parent)
    };

    let modified_at = (hit.mtime_ns > 0).then_some(hit.mtime_ns / 1_000_000_000);
    let is_container = hit.kind == "directory" || hit.kind == "archive";
    let mime_type = if is_container {
        None
    } else {
        let ext = extract_extension(&hit.name).to_lowercase();
        mime_for_extension(&ext).map(str::to_owned)
    };
    let parent_path_key = if is_container {
        Some(hit.relative_path.clone())
    } else {
        None
    };

    Some(SearchResultResponse {
        node_id,
        parent_node_id,
        name: hit.name,
        kind: hit.kind,
        relative_path: actual_relative,
        size_bytes: hit.size_bytes,
        modified_at,
        mime_type,
        child_count: None,
        preview_node_ids: Vec::new(),
        parent_path_key,
    })
}

/// ディレクトリ/アーカイブ結果に `child_count` + `preview_node_ids` をマージする
///
/// - `DirIndexReader::batch_dir_info` を 1 回だけ呼んで N+1 を回避
/// - プレビューエントリの `node_id` は `NodeRegistry::register_resolved` で生成
fn enrich_directory_results(
    results: &mut [SearchResultResponse],
    ctx: &ResolveContext<'_>,
) -> Result<(), AppError> {
    // parent_path_key を集める（重複排除）
    let mut keys: Vec<String> = results
        .iter()
        .filter_map(|r| r.parent_path_key.clone())
        .collect();
    keys.sort();
    keys.dedup();
    if keys.is_empty() {
        return Ok(());
    }

    // batch_dir_info は &[&str] を要求するので参照ベクタを作る
    let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();

    let reader = ctx
        .dir_index
        .reader()
        .map_err(|e| AppError::path_security(format!("DirIndex reader 取得失敗: {e}")))?;
    // 最大 4 件のプレビュー
    let info_map = reader
        .batch_dir_info(&key_refs, 4)
        .map_err(|e| AppError::path_security(format!("batch_dir_info 失敗: {e}")))?;

    for r in results.iter_mut() {
        let Some(key) = r.parent_path_key.as_deref() else {
            continue;
        };
        let Some(info) = info_map.get(key) else {
            continue;
        };
        r.child_count = Some(u32::try_from(info.count).unwrap_or(u32::MAX));

        // プレビュー DirEntry を絶対パスに復元 → register_resolved
        let mut preview_ids = Vec::with_capacity(info.previews.len());
        for prev in &info.previews {
            let Some((mount_root, actual_relative_dir)) =
                resolve_mount_path(&prev.parent_path, ctx.mount_id_map)
            else {
                continue;
            };
            let preview_abs = if actual_relative_dir.is_empty() {
                mount_root.join(&prev.name)
            } else {
                mount_root.join(&actual_relative_dir).join(&prev.name)
            };

            #[allow(
                clippy::expect_used,
                reason = "Mutex poison は致命的エラー、パニックが適切"
            )]
            let mut reg = ctx.registry.lock().expect("NodeRegistry Mutex poisoned");
            if reg.path_security().validate_existing(&preview_abs).is_err() {
                continue;
            }
            if let Ok(node_id) = reg.register_resolved(&preview_abs) {
                preview_ids.push(node_id);
            }
        }
        r.preview_node_ids = preview_ids;
    }

    Ok(())
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
