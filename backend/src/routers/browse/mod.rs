//! ディレクトリ閲覧 API
//!
//! - `GET /api/browse/{node_id}` — ディレクトリ一覧 (ページネーション + `ETag` + 304)
//! - `GET /api/browse/{node_id}/first-viewable` — 再帰的に最初の閲覧対象を探索
//! - `GET /api/browse/{parent_node_id}/sibling` — 次/前の兄弟セットを返す

mod archive;
mod fast_path;
mod first_viewable;
mod pagination;
mod sibling;
#[cfg(test)]
mod tests;

pub(crate) use first_viewable::first_viewable;
pub(crate) use sibling::{find_sibling, find_siblings};

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::services::browse_cursor::{MAX_LIMIT, SortOrder};
use crate::services::extensions::{self, EntryKind};
use crate::services::models::{AncestorEntry, BrowseResponse, EntryMeta};
use crate::services::node_registry::NodeRegistry;
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

/// `GET /api/browse/{parent_node_id}/siblings` のクエリパラメータ
#[derive(Debug, Deserialize)]
pub(crate) struct SiblingsQuery {
    /// 現在のエントリの `node_id`
    pub current: String,
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

/// sibling API のレスポンス (単方向)
#[derive(Debug, Serialize)]
pub(crate) struct SiblingResponse {
    pub entry: Option<EntryMeta>,
}

/// siblings API のレスポンス (prev + next を一括返却)
#[derive(Debug, Serialize)]
pub(crate) struct SiblingsResponse {
    pub prev: Option<EntryMeta>,
    pub next: Option<EntryMeta>,
}

// --- ETag 計算 ---

/// エントリ一覧から `ETag` (MD5 hex) を計算する
///
/// `node_id,name,kind,size_bytes,child_count,modified_at` を `|` 区切りで連結し、
/// MD5 ハッシュを生成する。Python 版と同一のロジック。
pub(super) fn compute_etag(entries: &[EntryMeta]) -> String {
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
    hex::encode(hasher.finalize())
}

// --- DirEntry → EntryMeta 変換ヘルパー ---

use crate::services::dir_index::DirEntry;

/// `parent_key` (`"{mount_id}/relative/path"`) から `mount_id` 以降の相対パスを取得する
pub(super) fn parent_key_relative(parent_key: &str) -> &str {
    parent_key
        .find('/')
        .map_or("", |i| parent_key[i..].trim_start_matches('/'))
}

/// `DirEntry` を `EntryMeta` に変換する (`node_id` 登録含む)
///
/// パスが存在しない場合は `None` を返す。
/// `child_count` / `preview_node_ids` はこの関数では設定しない (呼び出し側で必要に応じて補完)。
pub(super) fn dir_entry_to_entry_meta(
    de: &DirEntry,
    root: &std::path::Path,
    parent_key: &str,
    reg: &mut NodeRegistry,
) -> Option<EntryMeta> {
    let rel = parent_key_relative(parent_key);
    let abs_path = root.join(rel).join(&de.name);

    if !abs_path.exists() {
        return None;
    }
    let abs_resolved = std::fs::canonicalize(&abs_path).ok()?;
    let entry_node_id = reg.register_resolved(&abs_resolved).ok()?;

    let kind = if de.kind == "directory" {
        EntryKind::Directory
    } else {
        EntryKind::from_extension(&extensions::extract_extension(&de.name).to_ascii_lowercase())
    };

    let mime_type = if kind == EntryKind::Directory {
        None
    } else {
        let ext = extensions::extract_extension(&de.name).to_ascii_lowercase();
        extensions::mime_for_extension(&ext).map(String::from)
    };

    #[allow(clippy::cast_precision_loss, reason = "mtime_ns → f64 秒は十分な精度")]
    let modified_at = Some(de.mtime_ns as f64 / 1_000_000_000.0);

    Some(EntryMeta {
        node_id: entry_node_id,
        name: de.name.clone(),
        kind,
        size_bytes: de.size_bytes.map(|v| {
            #[allow(clippy::cast_sign_loss, reason = "size_bytes は非負")]
            let u = v as u64;
            u
        }),
        mime_type,
        child_count: None,
        modified_at,
        mtime_ns: None,
        preview_node_ids: None,
    })
}

// --- ハンドラ ---

/// `GET /api/browse/{node_id}`
///
/// ディレクトリ一覧を返す。`ETag` + 304 対応。
/// カーソルベースページネーション (limit + cursor + sort)。
#[allow(clippy::too_many_lines, reason = "DirIndex 高速パスの分岐で増加")]
pub(crate) async fn browse_directory(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
    Query(query): Query<BrowseQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    // limit のバリデーション (1..=500)
    if let Some(limit) = query.limit
        && (limit == 0 || limit > MAX_LIMIT)
    {
        tracing::warn!(limit, max = MAX_LIMIT, "不正な limit パラメータ");
        return Err(AppError::InvalidCursor(format!(
            "limit は 1 以上 {MAX_LIMIT} 以下で指定してください"
        )));
    }

    let sort = query.sort;
    let limit = query.limit;
    let cursor = query.cursor.clone();

    // Step 1: node_id 解決 + アーカイブ判定 (短ロック)
    let registry = Arc::clone(&state.node_registry);
    let nid = node_id.clone();
    let resolve_result = tokio::task::spawn_blocking(move || {
        #[allow(
            clippy::expect_used,
            reason = "Mutex poison は致命的エラー、パニックが適切"
        )]
        let reg = registry.lock().expect("NodeRegistry Mutex poisoned");

        // アーカイブエントリの browse は拒否
        if reg.is_archive_entry(&nid) {
            return Err(AppError::NotADirectory {
                path: format!("アーカイブエントリは browse 対象外です: {nid}"),
            });
        }
        let path = reg.resolve(&nid)?.to_path_buf();
        let is_archive = path.is_file() && extensions::is_archive_extension(&path);
        Ok((path, is_archive))
    })
    .await
    .map_err(|e| AppError::path_security(format!("タスク実行エラー: {e}")))?;
    let (resolved_path, is_archive) = resolve_result?;

    // Step 2: アーカイブの場合は browse_archive へ分岐
    let (response, etag) = if is_archive {
        archive::browse_archive(
            &state,
            &resolved_path,
            &node_id,
            sort,
            limit,
            cursor.as_deref(),
        )
        .await?
    } else {
        // ディレクトリの通常ブラウズ (DirIndex 高速パス → Two-Phase フォールバック)
        let registry = Arc::clone(&state.node_registry);
        let dir_index = Arc::clone(&state.dir_index);
        let nid = node_id.clone();
        let is_dir_index_ready = state.dir_index.is_ready();
        // 計測用の DirIndex 状態ラベル (cold/warm_indexing/warm_ready)
        let state_label = state.dir_index.state_label();
        tokio::task::spawn_blocking(move || {
            // Phase 0: 短時間ロックでパス解決 + PathSecurity 取得
            #[allow(
                clippy::expect_used,
                reason = "Mutex poison は致命的エラー、パニックが適切"
            )]
            let (path, path_security) = {
                let reg = registry.lock().expect("NodeRegistry Mutex poisoned");
                let path = reg.resolve(&nid)?.to_path_buf();
                let ps = reg.path_security_arc();
                (path, ps)
            };

            // DirIndex 高速パス: ready かつディレクトリのとき常に試行
            // limit = None (全件要求) にも対応 (SQLite `LIMIT -1`)
            // Phase 0 (短ロック) → Phase 1 (ロックなし) → Phase 2 (短ロック)
            if is_dir_index_ready
                && path.is_dir()
                && let Some(result) = fast_path::try_dir_index_browse_split(
                    &registry,
                    &dir_index,
                    &path,
                    &nid,
                    sort,
                    limit,
                    cursor.as_deref(),
                    state_label,
                )
            {
                return Ok(result);
            }

            // fallback 前に parent_key と dirty 世代を取得（self-healing 用）
            #[allow(
                clippy::expect_used,
                reason = "Mutex poison は致命的エラー、パニックが適切"
            )]
            let parent_key_for_heal = {
                let reg = registry.lock().expect("NodeRegistry Mutex poisoned");
                reg.compute_parent_path_key(&path)
            };
            // スキャン開始前の dirty 世代を記録
            // → スキャン完了後にこの世代と一致すれば、スキャン中の追加変更がなかったことを保証
            let dirty_generation = parent_key_for_heal.as_ref().and_then(|pk| {
                if dir_index.is_dir_dirty(pk) {
                    // dirty なら世代を再マークして取得（現在の世代を上書き）
                    Some(dir_index.mark_dir_dirty(pk))
                } else {
                    None
                }
            });

            // Two-Phase フォールバック: I/O をロック外で実行
            let result = browse_directory_blocking(
                &registry,
                &path_security,
                &path,
                &nid,
                sort,
                limit,
                cursor.as_deref(),
                state_label,
            );

            // fallback 成功後に DirIndex を自己修復 (mtime 更新 + dirty 解除)
            if result.is_ok()
                && let Some(ref pk) = parent_key_for_heal
            {
                // 現在の FS mtime で DirIndex を更新
                if let Some(mtime_ns) = std::fs::metadata(&path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| {
                        #[allow(
                            clippy::cast_possible_wrap,
                            reason = "UNIX タイムスタンプは i64 範囲内"
                        )]
                        let ns = d.as_nanos() as i64;
                        ns
                    })
                {
                    let _ = dir_index.set_dir_mtime(pk, mtime_ns);
                }
                // dirty 世代が一致する場合のみ dirty を解除
                // （スキャン中に FileWatcher が再 dirty 化した場合は世代が進んでいるので解除されない）
                if let Some(dg) = dirty_generation {
                    dir_index.clear_dir_dirty_if_match(pk, dg);
                }
            }

            result
        })
        .await
        .map_err(|e| AppError::path_security(format!("タスク実行エラー: {e}")))??
    };

    // `ETag` 比較 → 304 Not Modified
    if let Some(if_none_match) = headers.get("if-none-match")
        && let Ok(value) = if_none_match.to_str()
        && value == etag
    {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [
                ("etag", etag.as_str()),
                ("cache-control", "private, no-cache"),
            ],
        )
            .into_response());
    }

    // プリウォーム: サムネイルをバックグラウンドで事前生成
    state.thumbnail_warmer.warm(&response.entries, &state);

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

/// `browse_directory` のブロッキング処理本体 (Two-Phase Lock Splitting)
///
/// Phase 1: ロック外で filesystem I/O (scan + stat + canonicalize)
/// Phase 2: 短時間ロックで `HashMap` 登録 + パンくずリスト構築
#[allow(
    clippy::too_many_arguments,
    reason = "Two-Phase パターンで registry + path_security + path を分離して受け取る"
)]
fn browse_directory_blocking(
    registry: &std::sync::Mutex<NodeRegistry>,
    path_security: &crate::services::path_security::PathSecurity,
    path: &std::path::Path,
    node_id: &str,
    sort: SortOrder,
    limit: Option<usize>,
    cursor: Option<&str>,
    state_label: &'static str,
) -> Result<(BrowseResponse, String), AppError> {
    // 計測 span: フォールバック経路 (scan + stat + canonicalize)
    let span = tracing::info_span!("browse", state = state_label, kind = "fallback");
    let _enter = span.enter();
    let started = std::time::Instant::now();

    // ディレクトリかチェック
    if !path.is_dir() {
        return Err(AppError::NotADirectory {
            path: path.display().to_string(),
        });
    }

    // Phase 1 (ロック外): スキャン + ソート/ページネーション
    let (page_entries, next_cursor, total_count, etag) =
        pagination::fetch_page(registry, path_security, path, sort, limit, cursor)?;

    // Phase 2 (短時間ロック): パンくずリスト
    #[allow(
        clippy::expect_used,
        reason = "Mutex poison は致命的エラー、パニックが適切"
    )]
    let (parent_node_id, ancestors) = {
        let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");
        let pnid = reg.get_parent_node_id(path);
        let anc = reg
            .get_ancestors_from_resolved(path)
            .into_iter()
            .map(|(nid, name)| AncestorEntry { node_id: nid, name })
            .collect();
        (pnid, anc)
    };

    let response = BrowseResponse {
        current_node_id: Some(node_id.to_string()),
        current_name: path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
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

    tracing::info!(
        entries = response.entries.len(),
        elapsed_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
        "browse fallback completed"
    );

    Ok((response, etag))
}
