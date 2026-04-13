//! `DirIndex` 高速パス
//!
//! `DirIndex` が ready かつ mtime 一致時にファイルシステムスキャンをスキップする最適化経路。

use std::sync::Arc;

use crate::services::browse_cursor::{self, SortOrder};
use crate::services::dir_index::{DirChildInfo, DirEntry, DirIndex};
use crate::services::extensions::{self, EntryKind};
use crate::services::models::{AncestorEntry, BrowseResponse};
use crate::services::natural_sort::encode_sort_key;
use crate::services::node_registry::{NodeRegistry, ScannedEntry};

use super::{compute_etag, parent_key_relative};

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
pub(super) fn try_dir_index_browse_split(
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

    // FileWatcher が dirty 化したディレクトリは fallback にフォールバック
    if dir_index.is_dir_dirty(&parent_key) {
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
    entries: &[DirEntry],
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
