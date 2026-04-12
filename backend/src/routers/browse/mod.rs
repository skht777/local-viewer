//! ディレクトリ閲覧 API
//!
//! - `GET /api/browse/{node_id}` — ディレクトリ一覧 (ページネーション + `ETag` + 304)
//! - `GET /api/browse/{node_id}/first-viewable` — 再帰的に最初の閲覧対象を探索
//! - `GET /api/browse/{parent_node_id}/sibling` — 次/前の兄弟セットを返す

mod archive;
mod first_viewable;
mod sibling;
#[cfg(test)]
mod tests;

pub(crate) use first_viewable::first_viewable;
pub(crate) use sibling::find_sibling;

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::services::browse_cursor::{self, MAX_LIMIT, SortOrder};
use crate::services::dir_index::{DirChildInfo, DirIndex};
use crate::services::extensions::{self, EntryKind};
use crate::services::models::{AncestorEntry, BrowseResponse, EntryMeta};
use crate::services::natural_sort::encode_sort_key;
use crate::services::node_registry::{
    NodeRegistry, ScannedEntry, scan_entries, scan_entry_metas, stat_entries,
};
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

// --- DirEntry → EntryMeta 変換ヘルパー ---

use crate::services::dir_index::DirEntry;

/// `parent_key` (`"{mount_id}/relative/path"`) から `mount_id` 以降の相対パスを取得する
fn parent_key_relative(parent_key: &str) -> &str {
    parent_key
        .find('/')
        .map_or("", |i| parent_key[i..].trim_start_matches('/'))
}

/// `DirEntry` を `EntryMeta` に変換する (`node_id` 登録含む)
///
/// パスが存在しない場合は `None` を返す。
/// `child_count` / `preview_node_ids` はこの関数では設定しない (呼び出し側で必要に応じて補完)。
fn dir_entry_to_entry_meta(
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
    if let Some(limit) = query.limit {
        if limit == 0 || limit > MAX_LIMIT {
            tracing::warn!(limit, max = MAX_LIMIT, "不正な limit パラメータ");
            return Err(AppError::InvalidCursor(format!(
                "limit は 1 以上 {MAX_LIMIT} 以下で指定してください"
            )));
        }
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
            if is_dir_index_ready && path.is_dir() {
                if let Some(result) = try_dir_index_browse_split(
                    &registry,
                    &dir_index,
                    &path,
                    &nid,
                    sort,
                    limit,
                    cursor.as_deref(),
                    state_label,
                ) {
                    return Ok(result);
                }
            }

            // Two-Phase フォールバック: I/O をロック外で実行
            browse_directory_blocking(
                &registry,
                &path_security,
                &path,
                &nid,
                sort,
                limit,
                cursor.as_deref(),
                state_label,
            )
        })
        .await
        .map_err(|e| AppError::path_security(format!("タスク実行エラー: {e}")))??
    };

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
        fetch_page(registry, path_security, path, sort, limit, cursor)?;

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

/// `DirIndex` 高速パス (Three-Phase Lock Splitting)
///
/// `DirIndex` が ready かつ mtime が一致する場合のみ `Some` を返す。
///
/// - Phase 0 (短ロック) — `parent_key`, `root`, カーソル用パスを取得
/// - Phase 1 (ロックなし) — `DirIndex` クエリ + canonicalize + `ScannedEntry` 構築
/// - Phase 2 (短ロック) — `node_id` 登録 + パンくず
#[allow(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    reason = "Phase 0/1/2 の分割で行数が増加、引数は browse パラメータの透過渡し"
)]
fn try_dir_index_browse_split(
    registry: &Arc<std::sync::Mutex<NodeRegistry>>,
    dir_index: &DirIndex,
    path: &std::path::Path,
    node_id: &str,
    sort: SortOrder,
    limit: Option<usize>,
    cursor: Option<&str>,
    state_label: &'static str,
) -> Option<(BrowseResponse, String)> {
    // 計測 span: 高速パス (DirIndex + mtime ガード経路)
    let span = tracing::info_span!("browse", state = state_label, kind = "dir_index_fast");
    let _enter = span.enter();
    let started = std::time::Instant::now();

    // --- Phase 0 (短ロック): NodeRegistry から必要なキーを取得 ---
    #[allow(
        clippy::expect_used,
        reason = "Mutex poison は致命的エラー、パニックが適切"
    )]
    let (parent_key, root, cursor_entry_path, allow_symlinks) = {
        let reg = registry.lock().expect("NodeRegistry Mutex poisoned");
        let ps = reg.path_security();
        let parent_key = reg.compute_parent_path_key(path)?;
        let root = ps.find_root_for(path).map(std::path::Path::to_path_buf)?;
        let allow_symlinks = ps.is_allow_symlinks();
        let cursor_path = cursor.and_then(|c| {
            let decoded = browse_cursor::decode_cursor(c, sort).ok()?;
            reg.resolve(&decoded.node_id)
                .ok()
                .map(std::path::Path::to_path_buf)
        });
        (parent_key, root, cursor_path, allow_symlinks)
    }; // ロック解放

    // カーソル変換失敗時はフォールバック
    if cursor.is_some() && cursor_entry_path.is_none() {
        return None;
    }

    // --- Phase 1 (ロックなし): DirIndex クエリ + I/O ---

    // mtime ガード
    let fs_mtime_ns = std::fs::metadata(path)
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
        })?;

    let reader = dir_index.reader().ok()?;
    let stored_mtime = reader.get_dir_mtime(&parent_key).ok().flatten()?;
    if fs_mtime_ns != stored_mtime {
        return None;
    }

    let sort_str = match sort {
        SortOrder::NameAsc => "name-asc",
        SortOrder::NameDesc => "name-desc",
        SortOrder::DateAsc => "date-asc",
        SortOrder::DateDesc => "date-desc",
    };

    // DirIndex カーソルデコード
    let dir_index_cursor = cursor_entry_path.and_then(|entry_path| {
        let name = entry_path.file_name()?.to_string_lossy().into_owned();
        if matches!(sort, SortOrder::NameAsc | SortOrder::NameDesc) {
            let entry_sort_key = encode_sort_key(&name);
            let is_dir = entry_path.is_dir();
            let kind_flag = if is_dir { "0" } else { "1" };
            Some(format!("{kind_flag}\x00{entry_sort_key}"))
        } else {
            let mtime_ns = std::fs::metadata(&entry_path)
                .ok()?
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?;
            #[allow(
                clippy::cast_possible_wrap,
                reason = "UNIX タイムスタンプは i64 範囲内"
            )]
            let ns = mtime_ns.as_nanos() as i64;
            let entry_sort_key = encode_sort_key(&name);
            Some(format!("{ns}\x00{entry_sort_key}"))
        }
    });

    if cursor.is_some() && dir_index_cursor.is_none() {
        return None;
    }

    // limit = Some(n) は `n+1` 件要求して has_next 判定。
    // limit = None は SQLite LIMIT -1 にマップされ全件返る (has_next は常に false)。
    let query_limit = limit.map(|n| n.saturating_add(1));
    let entries = reader
        .query_page(
            &parent_key,
            sort_str,
            query_limit,
            dir_index_cursor.as_deref(),
        )
        .ok()?;

    let (has_next, page_entries): (bool, Vec<_>) = match limit {
        Some(n) => (entries.len() > n, entries.into_iter().take(n).collect()),
        None => (false, entries),
    };
    let total_count = reader.child_count(&parent_key).ok()?;

    // ディレクトリの child_key を収集してバッチ取得
    let dir_child_keys: Vec<String> = page_entries
        .iter()
        .filter(|de| de.kind == "directory")
        .map(|de| format!("{parent_key}/{}", de.name))
        .collect();
    let dir_child_key_refs: Vec<&str> = dir_child_keys.iter().map(String::as_str).collect();
    let dir_info = reader.batch_dir_info(&dir_child_key_refs, 3).ok()?;

    // DirEntry → ScannedEntry 変換 (ロック不要)
    let scanned =
        build_scanned_from_dir_index(&page_entries, &root, &parent_key, &dir_info, allow_symlinks);

    // --- Phase 2 (短ロック): node_id 登録 + パンくず ---
    #[allow(
        clippy::expect_used,
        reason = "Mutex poison は致命的エラー、パニックが適切"
    )]
    let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");

    let entry_metas = reg.register_scanned_entries(scanned).ok()?;

    let etag = compute_etag(&entry_metas);

    let next_cursor = if has_next {
        entry_metas
            .last()
            .map(|last| browse_cursor::encode_cursor(sort, last, &etag))
    } else {
        None
    };

    let parent_node_id = reg.get_parent_node_id(path);
    // path は resolve() 由来で canonicalize 済み
    let ancestors = reg
        .get_ancestors_from_resolved(path)
        .into_iter()
        .map(|(nid, name)| AncestorEntry { node_id: nid, name })
        .collect();

    let response = BrowseResponse {
        current_node_id: Some(node_id.to_string()),
        current_name: path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        parent_node_id,
        ancestors,
        entries: entry_metas,
        next_cursor,
        total_count: Some(total_count),
    };

    tracing::info!(
        entries = response.entries.len(),
        has_next,
        elapsed_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
        "browse dir_index_fast completed"
    );

    Some((response, etag))
}

/// `DirIndex` の `DirEntry` + バッチ情報から `ScannedEntry` を構築する (ロック不要)
///
/// mtime ガード通過済みのため、DirIndex のエントリはファイルシステムと一致している。
/// `exists()` / `canonicalize()` をスキップしてパスをそのまま使用する。
/// symlink 有効時のみ `canonicalize` で正規化する。
fn build_scanned_from_dir_index(
    entries: &[crate::services::dir_index::DirEntry],
    root: &std::path::Path,
    parent_key: &str,
    dir_info: &std::collections::HashMap<String, DirChildInfo>,
    allow_symlinks: bool,
) -> Vec<ScannedEntry> {
    entries
        .iter()
        .filter_map(|de| {
            let rel = parent_key_relative(parent_key);
            let abs_path = root.join(rel).join(&de.name);
            // mtime ガード通過済み: エントリ構成はスキャン時点から不変
            // symlink 有効時のみ canonicalize で正規化
            let resolved = if allow_symlinks {
                std::fs::canonicalize(&abs_path).ok()?
            } else {
                abs_path
            };

            let kind = if de.kind == "directory" {
                EntryKind::Directory
            } else {
                EntryKind::from_extension(
                    &extensions::extract_extension(&de.name).to_ascii_lowercase(),
                )
            };

            let mime_type = if kind == EntryKind::Directory {
                None
            } else {
                let ext = extensions::extract_extension(&de.name).to_ascii_lowercase();
                extensions::mime_for_extension(&ext).map(String::from)
            };

            #[allow(clippy::cast_precision_loss, reason = "mtime_ns → f64 秒は十分な精度")]
            let modified_at = Some(de.mtime_ns as f64 / 1_000_000_000.0);

            #[allow(clippy::cast_sign_loss, reason = "size_bytes は非負")]
            let size_bytes = de.size_bytes.map(|v| v as u64);

            let (child_count, preview_paths) = if kind == EntryKind::Directory {
                let child_key = format!("{parent_key}/{}", de.name);
                let info = dir_info.get(&child_key);
                let count = info.map_or(0, |i| i.count);
                let previews = info.and_then(|i| {
                    let paths: Vec<std::path::PathBuf> = i
                        .previews
                        .iter()
                        .filter_map(|pv| {
                            let pv_rel = parent_key_relative(&child_key);
                            let pv_abs = root.join(pv_rel).join(&pv.name);
                            if allow_symlinks {
                                std::fs::canonicalize(&pv_abs).ok()
                            } else {
                                Some(pv_abs)
                            }
                        })
                        .collect();
                    if paths.is_empty() { None } else { Some(paths) }
                });
                (Some(count), previews)
            } else {
                (None, None)
            };

            // DirEntry.mtime_ns は i64。負値（UNIX_EPOCH 前）は 0 扱い。
            // try_from で clippy::cast_sign_loss を回避する。
            let mtime_ns = Some(u128::try_from(de.mtime_ns).unwrap_or(0));

            Some(ScannedEntry {
                path: resolved,
                kind,
                name: de.name.clone(),
                size_bytes,
                modified_at,
                mtime_ns,
                mime_type,
                child_count,
                preview_paths,
            })
        })
        .collect()
}

/// ディレクトリエントリを取得し、ソート/ページネーションを適用する (Two-Phase)
///
/// Phase 1: ロック外で scan + stat + `scan_entry_metas`
/// Phase 2: 短時間ロックで `register_scanned_entries`
#[allow(
    clippy::type_complexity,
    reason = "ページネーション結果のタプルが自然な構造"
)]
fn fetch_page(
    registry: &std::sync::Mutex<NodeRegistry>,
    path_security: &crate::services::path_security::PathSecurity,
    path: &std::path::Path,
    sort: SortOrder,
    limit: Option<usize>,
    cursor: Option<&str>,
) -> Result<(Vec<EntryMeta>, Option<String>, usize, String), AppError> {
    let is_name_sort = matches!(sort, SortOrder::NameAsc | SortOrder::NameDesc);

    if is_name_sort && limit.is_some() {
        fetch_page_name_sort(
            registry,
            path_security,
            path,
            sort,
            limit.unwrap_or(0),
            cursor,
        )
    } else {
        fetch_page_full(registry, path_security, path, sort, limit, cursor)
    }
}

/// name ソート + limit 指定時: ページ分だけ stat する最適化パス (Two-Phase)
#[allow(
    clippy::type_complexity,
    reason = "ページネーション結果のタプルが自然な構造"
)]
fn fetch_page_name_sort(
    registry: &std::sync::Mutex<NodeRegistry>,
    path_security: &crate::services::path_security::PathSecurity,
    path: &std::path::Path,
    sort: SortOrder,
    limit_val: usize,
    cursor: Option<&str>,
) -> Result<(Vec<EntryMeta>, Option<String>, usize, String), AppError> {
    // カーソルから node_id を抽出
    let cursor_node_id = cursor
        .map(|c| browse_cursor::decode_cursor(c, sort).map(|d| d.node_id))
        .transpose()?;

    // Phase 1 (ロック外): scan + sort + page slice + stat + build ScannedEntry
    let mut raw = scan_entries(path_security, path)?;
    let total_count = raw.len();
    let reverse = sort == SortOrder::NameDesc;

    // ディレクトリ優先 + 自然順ソート
    use crate::services::natural_sort::natural_sort_key;
    raw.sort_by(|(a_path, a_kind, _), (b_path, b_kind, _)| {
        let a_is_dir = *a_kind == EntryKind::Directory;
        let b_is_dir = *b_kind == EntryKind::Directory;
        b_is_dir.cmp(&a_is_dir).then_with(|| {
            let a_name = a_path.file_name().unwrap_or_default().to_string_lossy();
            let b_name = b_path.file_name().unwrap_or_default().to_string_lossy();
            natural_sort_key(&a_name).cmp(&natural_sort_key(&b_name))
        })
    });

    if reverse {
        let dir_count = raw
            .iter()
            .filter(|(_, k, _)| *k == EntryKind::Directory)
            .count();
        raw[..dir_count].reverse();
        raw[dir_count..].reverse();
    }

    // カーソル位置を検索 (短時間ロック: path_to_id 参照)
    let start_idx = if let Some(ref cursor_id) = cursor_node_id {
        #[allow(
            clippy::expect_used,
            reason = "Mutex poison は致命的エラー、パニックが適切"
        )]
        let reg = registry.lock().expect("NodeRegistry Mutex poisoned");
        raw.iter()
            .position(|(p, _, _)| {
                let key = p.to_string_lossy();
                reg.path_to_id_get(key.as_ref())
                    .is_some_and(|id| id == *cursor_id)
            })
            .map_or(0, |pos| pos + 1)
    } else {
        0
    };

    let fetch_limit = limit_val + 1; // +1 で次ページ有無を判定
    let end_idx = (start_idx + fetch_limit).min(raw.len());
    let page_raw = &raw[start_idx..end_idx];

    // ページ分だけ stat + scan_entry_metas
    let stated: Vec<_> = page_raw
        .iter()
        .map(|(p, k, _)| (p.clone(), *k, std::fs::metadata(p).ok()))
        .collect();
    let scanned = scan_entry_metas(path_security, stated, 3);

    // Phase 2 (短時間ロック): register
    #[allow(
        clippy::expect_used,
        reason = "Mutex poison は致命的エラー、パニックが適切"
    )]
    let all_entries = {
        let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");
        reg.register_scanned_entries(scanned)?
    };

    let has_next = all_entries.len() > limit_val;
    let page: Vec<EntryMeta> = all_entries.into_iter().take(limit_val).collect();

    let etag = compute_etag(&page);
    let next = if has_next {
        page.last()
            .map(|last| browse_cursor::encode_cursor(sort, last, &etag))
    } else {
        None
    };

    Ok((page, next, total_count, etag))
}

/// date ソート or limit なし: 全件取得してからページネーション (Two-Phase)
#[allow(
    clippy::type_complexity,
    reason = "ページネーション結果のタプルが自然な構造"
)]
fn fetch_page_full(
    registry: &std::sync::Mutex<NodeRegistry>,
    path_security: &crate::services::path_security::PathSecurity,
    path: &std::path::Path,
    sort: SortOrder,
    limit: Option<usize>,
    cursor: Option<&str>,
) -> Result<(Vec<EntryMeta>, Option<String>, usize, String), AppError> {
    // Phase 1 (ロック外): scan + stat + build ScannedEntry
    let raw = scan_entries(path_security, path)?;
    let stated = stat_entries(&raw);
    let scanned = scan_entry_metas(path_security, stated, 3);

    // Phase 2 (短時間ロック): register
    #[allow(
        clippy::expect_used,
        reason = "Mutex poison は致命的エラー、パニックが適切"
    )]
    let entries = {
        let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");
        reg.register_scanned_entries(scanned)?
    };

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
