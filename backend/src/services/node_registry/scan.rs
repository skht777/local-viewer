//! ファイルシステムスキャンとメタデータ抽出

use std::fs::Metadata;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::errors::AppError;
use crate::services::extensions::{
    EntryKind, extract_extension, is_thumbnail_extension, mime_for_extension,
};
use crate::services::path_security::PathSecurity;

pub(crate) struct PageOptions<'a> {
    pub limit: usize,
    pub cursor_node_id: Option<&'a str>,
    pub reverse: bool,
}

// --- Two-Phase Lock Splitting 用データ構造と free functions ---

/// Phase 1（ロック外）で収集したスキャン結果
pub(crate) struct ScannedEntry {
    pub path: PathBuf,
    pub kind: EntryKind,
    pub name: String,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<f64>,
    /// 真値の mtime (ns 精度)。warmer / thumbnail cache key 用。
    /// `modified_at: f64` はサブミリ秒以下の精度が欠落するため、
    /// cache key 生成には必ずこちらを使う。
    pub mtime_ns: Option<u128>,
    pub mime_type: Option<String>,
    pub child_count: Option<usize>,
    /// ディレクトリのプレビュー画像パス (canonicalize 済み)
    pub preview_paths: Option<Vec<PathBuf>>,
}

/// ディレクトリ子要素のスキャン結果 (ロック外)
pub(crate) struct ScannedChildMeta {
    pub count: usize,
    pub preview_paths: Vec<PathBuf>,
}

/// ディレクトリ内エントリ一覧を取得する (ロック不要)
///
/// `read_dir` + `validate_child` + classify。stat は行わない。
pub(crate) fn scan_entries(
    path_security: &PathSecurity,
    directory: &Path,
) -> Result<Vec<(PathBuf, EntryKind, bool)>, AppError> {
    let entries = std::fs::read_dir(directory).map_err(|e| AppError::FileNotFound {
        path: format!("{}: {e}", directory.display()),
    })?;

    let mut result = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let is_symlink = file_type.is_symlink();

        if path_security.validate_child(&path, is_symlink).is_err() {
            continue;
        }

        let kind = if file_type.is_dir() || (is_symlink && path.is_dir()) {
            EntryKind::Directory
        } else {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let ext = extract_extension(&name);
            EntryKind::from_extension(ext)
        };

        result.push((path, kind, is_symlink));
    }
    Ok(result)
}

/// ディレクトリの子エントリ数 + プレビュー画像パスを取得する (ロック不要)
///
/// `register` は行わず、canonicalize 済みパスのみ返す。
pub(crate) fn scan_child_meta(
    path_security: &PathSecurity,
    directory: &Path,
    preview_limit: usize,
    allow_symlinks: bool,
) -> ScannedChildMeta {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return ScannedChildMeta {
            count: 0,
            preview_paths: vec![],
        };
    };

    let mut count = 0usize;
    let mut preview_paths = Vec::new();

    for entry in entries {
        let Ok(entry) = entry else { continue };
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            count += 1;
            continue;
        }
        count += 1;

        if preview_paths.len() < preview_limit {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let ext = extract_extension(&name);
            if is_thumbnail_extension(ext) {
                let path = entry.path();
                if path_security.validate_child(&path, ft.is_symlink()).is_ok() {
                    // symlink 無効時は canonicalize 不要
                    let resolved = if allow_symlinks {
                        std::fs::canonicalize(&path).unwrap_or(path)
                    } else {
                        path
                    };
                    preview_paths.push(resolved);
                }
            }
        }
    }

    ScannedChildMeta {
        count,
        preview_paths,
    }
}

/// stat 済みエントリから `ScannedEntry` を構築する (ロック不要)
///
/// canonicalize + child scan を実行。`register` は行わない。
/// 完了時に `scan_entry_metas` の経過時間と canonicalize / `scan_child_meta`
/// 呼び出し回数を info ログとして出力する (Phase 4 計測基盤)。
pub(crate) fn scan_entry_metas(
    path_security: &PathSecurity,
    stated: Vec<(PathBuf, EntryKind, Option<Metadata>)>,
    preview_limit: usize,
) -> Vec<ScannedEntry> {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let allow_symlinks = path_security.is_allow_symlinks();
    let scan_started = std::time::Instant::now();
    let canonicalize_count = AtomicUsize::new(0);
    let scan_child_count = AtomicUsize::new(0);
    let input_count = stated.len();

    let result: Vec<ScannedEntry> = stated
        .into_par_iter()
        .map(|(path, kind, meta)| {
            // symlink 無効時は canonicalize 不要 (validate_child で拒否済み、
            // read_dir の子は canonical 親 + エントリ名なので canonical)
            let resolved = if allow_symlinks {
                canonicalize_count.fetch_add(1, Ordering::Relaxed);
                std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone())
            } else {
                path.clone()
            };
            let name = path
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());

            // modified_at (f64 秒, フロント表示用) と mtime_ns (u128 ns 精度,
            // サムネイル cache key 用) を同じ Duration から併せて取得する。
            // f64 はサブミリ秒以下の精度が欠けるため、cache key には必ず ns 値を使う。
            let (size_bytes, modified_at, mtime_ns) =
                meta.as_ref().map_or((None, None, None), |m| {
                    let size = if kind == EntryKind::Directory {
                        None
                    } else {
                        Some(m.len())
                    };
                    let duration = m
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
                    let mtime = duration.map(|d| d.as_secs_f64());
                    let mtime_ns = duration.map(|d| d.as_nanos());
                    (size, mtime, mtime_ns)
                });

            let mime_type = if kind == EntryKind::Directory {
                None
            } else {
                let lower = name.to_lowercase();
                let ext = extract_extension(&lower);
                mime_for_extension(ext).map(String::from)
            };

            let (child_count, preview_paths) = if kind == EntryKind::Directory {
                scan_child_count.fetch_add(1, Ordering::Relaxed);
                let cm = scan_child_meta(path_security, &path, preview_limit, allow_symlinks);
                (Some(cm.count), Some(cm.preview_paths))
            } else {
                (None, None)
            };

            ScannedEntry {
                path: resolved,
                kind,
                name,
                size_bytes,
                mime_type,
                child_count,
                modified_at,
                mtime_ns,
                preview_paths,
            }
        })
        .collect();

    tracing::info!(
        input = input_count,
        output = result.len(),
        canonicalize = canonicalize_count.load(Ordering::Relaxed),
        scan_child_dirs = scan_child_count.load(Ordering::Relaxed),
        allow_symlinks,
        elapsed_us = u64::try_from(scan_started.elapsed().as_micros()).unwrap_or(u64::MAX),
        "scan_entry_metas completed"
    );

    result
}

/// 200 件超で rayon 並列 stat
const PARALLEL_STAT_THRESHOLD: usize = 200;

pub(crate) fn stat_entries(
    raw: &[(PathBuf, EntryKind, bool)],
) -> Vec<(PathBuf, EntryKind, Option<Metadata>)> {
    if raw.len() > PARALLEL_STAT_THRESHOLD {
        raw.par_iter()
            .map(|(p, k, _)| (p.clone(), *k, std::fs::metadata(p).ok()))
            .collect()
    } else {
        raw.iter()
            .map(|(p, k, _)| (p.clone(), *k, std::fs::metadata(p).ok()))
            .collect()
    }
}
