//! `DirIndex` 高速パス
//!
//! `DirIndex` が ready かつ mtime 一致時にファイルシステムスキャンをスキップする最適化経路。

use std::sync::Arc;

use crate::services::browse_cursor::{self, SortOrder};
use crate::services::dir_index::{DirChildInfo, DirEntry, DirIndex};
use crate::services::extensions::{self, EntryKind};
use crate::services::models::{AncestorEntry, BrowseResponse};
use crate::services::node_registry::{NodeRegistry, ScannedEntry, scan_child_meta};
use crate::services::path_security::PathSecurity;

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

    // ユーザー入力 (cursor) のデコードはロック取得前に行う
    // (デコード処理の欠陥が NodeRegistry Mutex の poison に波及しないよう隔離する)
    let cursor_node_id = cursor.and_then(|c| {
        browse_cursor::decode_cursor(c, sort)
            .ok()
            .map(|d| d.node_id)
    });

    // --- Phase 0 (短ロック): NodeRegistry から必要なキーを取得 ---
    #[allow(
        clippy::expect_used,
        reason = "Mutex poison は致命的エラー、パニックが適切"
    )]
    let (parent_key, root, cursor_entry_path, allow_symlinks, path_security) = {
        let reg = registry.lock().expect("NodeRegistry Mutex poisoned");
        let ps = reg.path_security_arc();
        let parent_key = reg.compute_parent_path_key(path)?;
        let root = ps.find_root_for(path)?;
        let allow_symlinks = ps.is_allow_symlinks();
        let cursor_path = cursor_node_id
            .as_deref()
            .and_then(|id| reg.resolve(id).ok().map(std::path::Path::to_path_buf));
        (parent_key, root, cursor_path, allow_symlinks, ps)
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

    let sort_str = sort.as_wire_str();

    // DirIndex カーソルデコード
    //
    // kind / mtime_ns はファイルシステムではなく DirIndex の行から取る。
    // 比較対象 (SQL の dir_entries) とカーソル値のソースが食い違うと、
    // 子ファイルの in-place 更新等で FS と DB の mtime が乖離した瞬間に
    // date ソートのページ境界でエントリの欠落・重複が発生する。
    // DirIndex に該当行が無い場合はカーソル位置を決められないため fallback に落とす。
    let dir_index_cursor = cursor_entry_path.and_then(|entry_path| {
        let name = entry_path.file_name()?.to_string_lossy().into_owned();
        let de = reader.entry_by_name(&parent_key, &name).ok()??;
        if matches!(sort, SortOrder::NameAsc | SortOrder::NameDesc) {
            // カーソルは "{kind_flag}\x00{name}" 形式 (sort_key はクエリ側で name から導出)
            let kind_flag = if de.kind == "directory" { "0" } else { "1" };
            Some(format!("{kind_flag}\x00{name}"))
        } else {
            // カーソルは "{mtime_ns}\x00{name}" 形式 (sort_key はクエリ側で name から導出)
            Some(format!("{}\x00{name}", de.mtime_ns))
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
    // dirty な子ディレクトリは scan_child_meta で FS から再スキャンする
    let mut dirty_child_rescanned = 0usize;
    let scanned = build_scanned_from_dir_index(
        &page_entries,
        &root,
        &parent_key,
        &dir_info,
        allow_symlinks,
        dir_index,
        path_security.as_ref(),
        &mut dirty_child_rescanned,
    );

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
        dirty_child_rescanned,
        elapsed_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
        "browse dir_index_fast completed"
    );

    Some((response, etag))
}

/// `DirIndex` 由来のエントリ名が安全か検証する (lexical validation)
///
/// 永続層 (`SQLite`) から復元した name を `root.join(...)` する前に、`Path::components()`
/// が `Component::Normal` 1 つだけで構成されることを確認する。`..` / `.` / `/abs` /
/// `\\` / `""` 等は reject。NUL バイトも明示的に reject (`Path::components` は
/// NUL を Normal 内の文字として扱うため)。`09_security.md` の必須要件
/// 「永続層・キャッシュ復元パスの `join` 前検証必須」に対応。
fn safe_db_name(name: &str) -> bool {
    if name.as_bytes().contains(&0) {
        return false;
    }
    let mut comps = std::path::Path::new(name).components();
    matches!(
        (comps.next(), comps.next()),
        (Some(std::path::Component::Normal(_)), None)
    )
}

/// `DirIndex` の `DirEntry` + バッチ情報から `ScannedEntry` を構築する (ロック不要)
///
/// mtime ガード通過済みのため、DirIndex のエントリはファイルシステムと一致している。
/// `exists()` / `canonicalize()` をスキップしてパスをそのまま使用する。
/// symlink 有効時のみ `canonicalize` で正規化する。
///
/// `de.name` / `pv.name` は `safe_db_name` で lexical validation し、不正名は捨てる
/// （永続層の DB 破損 / cfg 改竄に対する fail-closed 防壁）。
///
/// 子ディレクトリが `dir_index.is_dir_dirty` で dirty 化されている場合、
/// `dir_info` の preview/count は stale 可能性があるため `scan_child_meta` で
/// FS から即時再スキャンして上書きする（FileWatcher 連動の自己修復）。
/// `dirty_child_rescanned` には再スキャン件数を加算する。
#[allow(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    reason = "DirEntry → ScannedEntry 変換 + 不正名 reject + preview 構築 + dirty 子再スキャンを 1 関数に集約"
)]
fn build_scanned_from_dir_index(
    entries: &[DirEntry],
    root: &std::path::Path,
    parent_key: &str,
    dir_info: &std::collections::HashMap<String, DirChildInfo>,
    allow_symlinks: bool,
    dir_index: &DirIndex,
    path_security: &PathSecurity,
    dirty_child_rescanned: &mut usize,
) -> Vec<ScannedEntry> {
    entries
        .iter()
        .filter_map(|de| {
            // 永続層由来 name の lexical validation (root.join 前)
            if !safe_db_name(&de.name) {
                tracing::warn!(
                    parent_key,
                    name = %de.name,
                    "DirIndex 由来の不正な name を reject"
                );
                return None;
            }

            let rel = parent_key_relative(parent_key);
            let abs_path = root.join(rel).join(&de.name);
            // mtime ガード通過済み: エントリ構成はスキャン時点から不変
            // symlink 有効時のみ canonicalize で正規化
            let resolved = if allow_symlinks {
                std::fs::canonicalize(&abs_path).ok()?
            } else {
                abs_path.clone()
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
                if dir_index.is_dir_dirty(&child_key) {
                    // dirty 子: FS から再スキャンして DirIndex の stale を回避
                    *dirty_child_rescanned += 1;
                    let cm = scan_child_meta(path_security, &abs_path, 3, allow_symlinks);
                    let previews = if cm.preview_paths.is_empty() {
                        None
                    } else {
                        Some(cm.preview_paths)
                    };
                    (Some(cm.count), previews)
                } else {
                    let info = dir_info.get(&child_key);
                    let count = info.map_or(0, |i| i.count);
                    let previews = info.and_then(|i| {
                        let paths: Vec<std::path::PathBuf> = i
                            .previews
                            .iter()
                            .filter_map(|pv| {
                                // preview name も lexical validation で fail-closed
                                if !safe_db_name(&pv.name) {
                                    tracing::warn!(
                                        child_key = %child_key,
                                        name = %pv.name,
                                        "DirIndex 由来の preview name を reject"
                                    );
                                    return None;
                                }
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
                }
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

#[cfg(test)]
mod safe_db_name_tests {
    use super::safe_db_name;

    #[test]
    fn 通常のファイル名は通過する() {
        assert!(safe_db_name("photo.jpg"));
        assert!(safe_db_name("a"));
        assert!(safe_db_name("ふぁいる.png"));
    }

    #[test]
    fn 空文字列は除外される() {
        assert!(!safe_db_name(""));
    }

    #[test]
    fn dot_と_dotdot_は除外される() {
        assert!(!safe_db_name("."));
        assert!(!safe_db_name(".."));
    }

    #[test]
    fn 区切り含み_絶対パスは除外される() {
        assert!(!safe_db_name("a/b"));
        assert!(!safe_db_name("/abs"));
        assert!(!safe_db_name("/"));
    }

    #[test]
    fn nul_バイト含みは除外される() {
        assert!(!safe_db_name("a\0b"));
    }
}
