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
use crate::services::dir_index::DirIndex;
use crate::services::extensions::{self, EntryKind};
use crate::services::models::{AncestorEntry, BrowseResponse, EntryMeta};
use crate::services::natural_sort::encode_sort_key;
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
    let entry_node_id = reg.register_resolved(&abs_resolved);

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
        browse_archive(
            &state,
            &resolved_path,
            &node_id,
            sort,
            limit,
            cursor.as_deref(),
        )
        .await?
    } else {
        // ディレクトリの通常ブラウズ (DirIndex 高速パス → フォールバック)
        let registry = Arc::clone(&state.node_registry);
        let dir_index = Arc::clone(&state.dir_index);
        let nid = node_id.clone();
        let is_dir_index_ready = state.dir_index.is_ready();
        let has_limit = limit.is_some();
        tokio::task::spawn_blocking(move || {
            #[allow(
                clippy::expect_used,
                reason = "Mutex poison は致命的エラー、パニックが適切"
            )]
            let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");

            // DirIndex 高速パス: ready かつ limit 指定時に試行
            if is_dir_index_ready && has_limit {
                let path = reg.resolve(&nid)?.to_path_buf();
                if path.is_dir() {
                    if let Some(result) = try_dir_index_browse(
                        &mut reg,
                        &dir_index,
                        &path,
                        &nid,
                        sort,
                        limit.unwrap_or(0),
                        cursor.as_deref(),
                    ) {
                        return Ok(result);
                    }
                }
            }

            // フォールバック: 従来の scandir パス
            browse_directory_blocking(&mut reg, &nid, sort, limit, cursor.as_deref())
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

/// `DirIndex` 高速パスでディレクトリ一覧を取得する
///
/// `DirIndex` が ready かつ mtime が一致する場合のみ `Some` を返す。
/// - `DirEntry` → `EntryMeta` への変換 (`node_id` 登録含む)
/// - ディレクトリエントリの `child_count` / `preview_node_ids` を `DirIndex` から取得
/// - カーソルは `DirIndex` 固有形式 (`sort_key` ベース)
#[allow(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    reason = "DirIndex → EntryMeta 変換ロジック、引数は browse パラメータの透過渡し"
)]
fn try_dir_index_browse(
    reg: &mut NodeRegistry,
    dir_index: &DirIndex,
    path: &std::path::Path,
    node_id: &str,
    sort: SortOrder,
    limit: usize,
    cursor: Option<&str>,
) -> Option<(BrowseResponse, String)> {
    // parent_path_key を計算 (mount_id/relative 形式)
    let parent_key = reg.compute_parent_path_key(path)?;

    // mtime ガード: ファイルシステムの mtime と DirIndex の mtime を比較
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

    let stored_mtime = dir_index.get_dir_mtime(&parent_key).ok().flatten()?;
    if fs_mtime_ns != stored_mtime {
        return None; // mtime 不一致 → DirIndex が古い
    }

    // ソート指定文字列
    let sort_str = match sort {
        SortOrder::NameAsc => "name-asc",
        SortOrder::NameDesc => "name-desc",
        SortOrder::DateAsc => "date-asc",
        SortOrder::DateDesc => "date-desc",
    };

    // DirIndex カーソルデコード:
    // browse_cursor 形式のカーソルから DirIndex 用の sort_key カーソルを抽出する
    let dir_index_cursor = cursor.and_then(|c| {
        let decoded = browse_cursor::decode_cursor(c, sort).ok()?;
        let entry_path = reg.resolve(&decoded.node_id).ok()?.to_path_buf();
        let name = entry_path.file_name()?.to_string_lossy().to_string();

        if matches!(sort, SortOrder::NameAsc | SortOrder::NameDesc) {
            // name 系ソート: カーソルの node_id から sort_key を構築
            let entry_sort_key = encode_sort_key(&name);
            let is_dir = entry_path.is_dir();
            let kind_flag = if is_dir { "0" } else { "1" };
            Some(format!("{kind_flag}\x00{entry_sort_key}"))
        } else {
            // date 系ソート: ファイルの mtime をナノ秒文字列で返す
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
            Some(ns.to_string())
        }
    });

    // +1 で次ページ有無を判定
    let query_limit = limit + 1;
    let entries = dir_index
        .query_page(
            &parent_key,
            sort_str,
            query_limit,
            dir_index_cursor.as_deref(),
        )
        .ok()?;

    let has_next = entries.len() > limit;
    let page_entries: Vec<_> = entries.into_iter().take(limit).collect();

    // total_count は DirIndex の child_count で取得
    let total_count = dir_index.child_count(&parent_key).ok()?;

    // DirEntry → EntryMeta 変換 (共通関数 + ディレクトリ固有の child_count/preview 補完)
    let root = reg
        .path_security()
        .find_root_for(path)
        .map(std::path::Path::to_path_buf)?;
    let mut entry_metas = Vec::with_capacity(page_entries.len());

    for de in &page_entries {
        let Some(mut meta) = dir_entry_to_entry_meta(de, &root, &parent_key, reg) else {
            continue;
        };

        // ディレクトリの child_count と preview_node_ids を DirIndex から補完
        if meta.kind == EntryKind::Directory {
            let child_key = format!("{parent_key}/{}", de.name);
            meta.child_count = dir_index.child_count(&child_key).ok();
            meta.preview_node_ids = dir_index
                .preview_entries(&child_key, 3)
                .ok()
                .map(|pvs| {
                    pvs.iter()
                        .filter_map(|pv| {
                            let pv_rel = parent_key_relative(&child_key);
                            let pv_abs = root.join(pv_rel).join(&pv.name);
                            let pv_resolved = std::fs::canonicalize(&pv_abs).ok()?;
                            Some(reg.register_resolved(&pv_resolved))
                        })
                        .collect::<Vec<_>>()
                })
                .filter(|v| !v.is_empty());
        }

        entry_metas.push(meta);
    }

    let etag = compute_etag(&entry_metas);

    // 次ページカーソル生成
    let next_cursor = if has_next {
        entry_metas
            .last()
            .map(|last| browse_cursor::encode_cursor(sort, last, &etag))
    } else {
        None
    };

    // パンくずリスト
    let parent_node_id = reg.get_parent_node_id(path);
    let ancestors = reg
        .get_ancestors(path)
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
        entries: entry_metas,
        next_cursor,
        total_count: Some(total_count),
    };

    Some((response, etag))
}

/// アーカイブファイルをディレクトリとして閲覧する
///
/// - `archive_service.list_entries()` でエントリ一覧取得 (ロック外)
/// - `NodeRegistry` にアーカイブエントリを登録 (短ロック)
/// - `BrowseResponse` を構築して返す
#[allow(
    clippy::too_many_lines,
    reason = "ページネーション追加で一時的に超過、将来分割予定"
)]
async fn browse_archive(
    state: &Arc<AppState>,
    archive_path: &std::path::Path,
    archive_node_id: &str,
    sort: SortOrder,
    limit: Option<usize>,
    cursor: Option<&str>,
) -> Result<(BrowseResponse, String), AppError> {
    // Step 1: アーカイブエントリ一覧を取得 (ロック外で I/O)
    let svc = Arc::clone(&state.archive_service);
    let path = archive_path.to_path_buf();
    let arc_entries = tokio::task::spawn_blocking(move || svc.list_entries(&path))
        .await
        .map_err(|e| AppError::path_security(format!("タスク実行エラー: {e}")))?
        .map_err(|e| match e {
            // zip/rar/7z ライブラリのエラーを InvalidArchive に正規化
            AppError::ArchiveSecurity(_) | AppError::ArchivePassword(_) => e,
            _ => AppError::InvalidArchive(e.to_string()),
        })?;

    // Step 2: NodeRegistry にエントリを登録して EntryMeta を構築 (短ロック)
    let registry = Arc::clone(&state.node_registry);
    let a_path = archive_path.to_path_buf();
    let a_nid = archive_node_id.to_string();
    let entries_clone = Arc::clone(&arc_entries);

    let (entry_metas, parent_node_id, ancestors) =
        tokio::task::spawn_blocking(move || -> Result<_, AppError> {
            #[allow(
                clippy::expect_used,
                reason = "Mutex poison は致命的エラー、パニックが適切"
            )]
            let mut reg = registry.lock().expect("NodeRegistry Mutex poisoned");

            let mut metas = Vec::with_capacity(entries_clone.len());
            for entry in entries_clone.iter() {
                // アーカイブエントリの node_id を登録
                let entry_node_id = reg.register_archive_entry(&a_path, &entry.name)?;

                // エントリ名からファイル名部分を取得 (パスの最後の要素)
                let display_name = entry
                    .name
                    .rsplit('/')
                    .next()
                    .unwrap_or(&entry.name)
                    .to_string();

                // 拡張子から kind と mime_type を判定
                let ext = extensions::extract_extension(&display_name).to_ascii_lowercase();
                let kind = EntryKind::from_extension(&ext);
                let mime_type = extensions::mime_for_extension(&ext).map(String::from);

                metas.push(EntryMeta {
                    node_id: entry_node_id,
                    name: display_name,
                    kind,
                    size_bytes: Some(entry.size_uncompressed),
                    mime_type,
                    child_count: None,
                    modified_at: None,
                    preview_node_ids: None,
                });
            }

            // パンくずリスト
            let parent_node_id = reg.get_parent_node_id(&a_path);
            let ancestors = reg
                .get_ancestors(&a_path)
                .into_iter()
                .map(|(nid, name)| AncestorEntry { node_id: nid, name })
                .collect::<Vec<_>>();

            Ok((metas, parent_node_id, ancestors))
        })
        .await
        .map_err(|e| AppError::path_security(format!("タスク実行エラー: {e}")))??;

    // ソート・ページネーション (fetch_page_full と同じパターン)
    let total = entry_metas.len();
    let (page_entries, next_cursor, etag) = if let Some(limit_val) = limit {
        let (page, next, _) =
            browse_cursor::paginate(entry_metas, sort, Some(limit_val), cursor, "")?;
        let etag = compute_etag(&page);
        let next = if next.is_some() {
            page.last()
                .map(|last| browse_cursor::encode_cursor(sort, last, &etag))
        } else {
            None
        };
        (page, next, etag)
    } else {
        let sorted = browse_cursor::sort_entries(entry_metas, sort);
        let etag = compute_etag(&sorted);
        (sorted, None, etag)
    };

    let archive_name = archive_path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().to_string());

    let response = BrowseResponse {
        current_node_id: Some(a_nid.clone()),
        current_name: archive_name,
        parent_node_id,
        ancestors,
        entries: page_entries,
        next_cursor,
        total_count: if limit.is_some() { Some(total) } else { None },
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
    use crate::services::dir_index::DirIndex;
    use crate::services::node_registry::NodeRegistry;
    use crate::services::path_security::PathSecurity;
    use crate::services::temp_file_cache::TempFileCache;
    use crate::services::thumbnail_service::ThumbnailService;
    use crate::services::thumbnail_warmer::ThumbnailWarmer;
    use crate::services::video_converter::VideoConverter;

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
        let dir_index_db = tempfile::NamedTempFile::new().unwrap();
        let dir_index = Arc::new(DirIndex::new(dir_index_db.path().to_str().unwrap()));
        dir_index.init_db().unwrap();
        Arc::new(AppState {
            settings: Arc::new(settings),
            node_registry: Arc::new(Mutex::new(registry)),
            archive_service,
            temp_file_cache,
            thumbnail_service,
            video_converter,
            thumbnail_warmer,
            indexer,
            dir_index,
            last_rebuild: tokio::sync::Mutex::new(None),
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

    // --- アーカイブ閲覧 ---

    fn create_archive_setup() -> (Router, Arc<AppState>, tempfile::TempDir) {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();

        // テスト用 ZIP ファイルを作成
        let zip_path = root.join("images.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file("img01.jpg", options).unwrap();
        writer.write_all(b"fake jpg").unwrap();
        writer.start_file("img02.png", options).unwrap();
        writer.write_all(b"fake png").unwrap();
        writer.finish().unwrap();

        let state = test_state(&root, HashMap::from([(root.clone(), "test".to_string())]));
        let app = Router::new()
            .route("/api/browse/{node_id}", get(browse_directory))
            .with_state(Arc::clone(&state));

        (app, state, dir)
    }

    #[tokio::test]
    async fn アーカイブファイルのnode_idでエントリ一覧を返す() {
        let (app, state, dir) = create_archive_setup();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let zip_node_id = register_node_id(&state, &root.join("images.zip"));

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/browse/{zip_node_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let resp: BrowseResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(resp.entries.len(), 2);
        assert_eq!(resp.current_name, "images.zip");
        // エントリ名はファイル名部分のみ
        let names: Vec<&str> = resp.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"img01.jpg"));
        assert!(names.contains(&"img02.png"));
    }

    #[tokio::test]
    async fn アーカイブ閲覧でlimit指定時にページネーションされる() {
        let (app, state, dir) = create_archive_setup();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let zip_node_id = register_node_id(&state, &root.join("images.zip"));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/browse/{zip_node_id}?limit=1"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let resp: BrowseResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(resp.entries.len(), 1, "limit=1 なので 1 件のみ");
        assert!(resp.next_cursor.is_some(), "次ページがあるはず");
        assert_eq!(resp.total_count, Some(2), "全エントリ数は 2");

        // next_cursor で2ページ目を取得
        let cursor = resp.next_cursor.unwrap();
        let response2 = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/browse/{zip_node_id}?limit=1&cursor={cursor}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body2 = axum::body::to_bytes(response2.into_body(), usize::MAX)
            .await
            .unwrap();
        let resp2: BrowseResponse = serde_json::from_slice(&body2).unwrap();
        assert_eq!(resp2.entries.len(), 1, "2ページ目も 1 件");
        assert!(resp2.next_cursor.is_none(), "最終ページ");

        // 重複なし
        assert_ne!(resp.entries[0].name, resp2.entries[0].name);
    }

    #[tokio::test]
    async fn アーカイブ閲覧でtotal_countが返される() {
        let (app, state, dir) = create_archive_setup();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let zip_node_id = register_node_id(&state, &root.join("images.zip"));

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/browse/{zip_node_id}?limit=10"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let resp: BrowseResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(resp.total_count, Some(2));
        assert!(resp.next_cursor.is_none(), "全件収まるので次ページなし");
    }

    #[tokio::test]
    async fn アーカイブのbrowse_responseにparent_node_idが設定される() {
        let (app, state, dir) = create_archive_setup();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let zip_node_id = register_node_id(&state, &root.join("images.zip"));

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/browse/{zip_node_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let resp: BrowseResponse = serde_json::from_slice(&body).unwrap();
        // parent_node_id はルートディレクトリの node_id
        assert!(resp.parent_node_id.is_some());
    }
}
