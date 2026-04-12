//! ディレクトリリスティング関連メソッド
//!
//! `NodeRegistry` のディレクトリ一覧取得・ページネーション・
//! マウントルート一覧を担当する。

use std::fs::Metadata;
use std::path::{Path, PathBuf};

use crate::errors::AppError;
use crate::services::extensions::{
    EntryKind, extract_extension, is_thumbnail_extension, mime_for_extension,
};
use crate::services::models::EntryMeta;
use crate::services::natural_sort::natural_sort_key;

use super::NodeRegistry;
use super::scan::{PageOptions, stat_entries};

impl NodeRegistry {
    // --- ディレクトリリスティング ---

    /// ディレクトリ内全エントリを `EntryMeta` として返す
    ///
    /// 3 フェーズ:
    /// 1. `read_dir` + `validate_child` + classify (stat なし)
    /// 2. stat 全エントリ (>200 件で rayon 並列)
    /// 3. `register_resolved` + `EntryMeta` 構築
    pub(crate) fn list_directory(&mut self, directory: &Path) -> Result<Vec<EntryMeta>, AppError> {
        // Phase 1: read_dir + classify
        let raw = self.scan_entries(directory)?;

        // Phase 2: stat (閾値超過で rayon 並列)
        let stated = stat_entries(&raw);

        // Phase 3: register + EntryMeta 構築
        self.build_entry_metas(directory, stated)
    }

    /// ページサイズ分のみ stat する最適化版 (name-sort 専用)
    pub(crate) fn list_directory_page(
        &mut self,
        directory: &Path,
        options: &PageOptions<'_>,
    ) -> Result<(Vec<EntryMeta>, usize), AppError> {
        // Phase 1: read_dir + classify
        let mut raw = self.scan_entries(directory)?;
        let total_count = raw.len();

        // ディレクトリ優先 + 自然順ソート
        raw.sort_by(|(a_path, a_kind, _), (b_path, b_kind, _)| {
            let a_is_dir = *a_kind == EntryKind::Directory;
            let b_is_dir = *b_kind == EntryKind::Directory;
            b_is_dir.cmp(&a_is_dir).then_with(|| {
                let a_name = a_path.file_name().unwrap_or_default().to_string_lossy();
                let b_name = b_path.file_name().unwrap_or_default().to_string_lossy();
                natural_sort_key(&a_name).cmp(&natural_sort_key(&b_name))
            })
        });

        // reverse (name-desc) 時はディレクトリ/ファイルグループ内で反転
        if options.reverse {
            // ディレクトリとファイルの境界を見つけて各グループ内で反転
            let dir_count = raw
                .iter()
                .filter(|(_, k, _)| *k == EntryKind::Directory)
                .count();
            raw[..dir_count].reverse();
            raw[dir_count..].reverse();
        }

        // カーソル位置を検索
        let start_idx = if let Some(cursor_id) = options.cursor_node_id {
            // cursor_node_id に対応するエントリを見つけ、その次から
            raw.iter()
                .position(|(path, _, _)| {
                    let key = path.to_string_lossy();
                    self.path_to_id
                        .get(key.as_ref())
                        .is_some_and(|id| id == cursor_id)
                })
                .map_or(0, |pos| pos + 1)
        } else {
            0
        };

        // ページ分だけスライスして stat
        let end_idx = (start_idx + options.limit).min(raw.len());
        let page_raw = &raw[start_idx..end_idx];

        // stat (ページ分のみ)
        let stated: Vec<_> = page_raw
            .iter()
            .map(|(p, k, _)| (p.clone(), *k, std::fs::metadata(p).ok()))
            .collect();

        let entries = self.build_entry_metas(directory, stated)?;
        Ok((entries, total_count))
    }

    /// 全マウントルートを `EntryMeta` として返す
    pub(crate) fn list_mount_roots(&mut self) -> Vec<EntryMeta> {
        let roots: Vec<PathBuf> = self.path_security.root_dirs().to_vec();
        roots
            .into_iter()
            .filter_map(|root| {
                let node_id = self.register_resolved(&root).ok()?;
                let name = self.mount_names.get(&root).cloned().unwrap_or_else(|| {
                    root.file_name().map_or_else(
                        || root.to_string_lossy().into_owned(),
                        |n| n.to_string_lossy().into_owned(),
                    )
                });
                let (child_count, preview_node_ids) = self.scan_child_meta(&root, 3);
                Some(EntryMeta {
                    node_id,
                    name,
                    kind: EntryKind::Directory,
                    size_bytes: None,
                    mime_type: None,
                    child_count: Some(child_count),
                    modified_at: None,
                    mtime_ns: None,
                    preview_node_ids,
                })
            })
            .collect()
    }

    // --- 内部ヘルパー ---

    /// `read_dir` + `validate_child` + classify (stat なし)
    fn scan_entries(&self, directory: &Path) -> Result<Vec<(PathBuf, EntryKind, bool)>, AppError> {
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

            // validate_child でセキュリティチェック (symlink 拒否等)
            if self
                .path_security
                .validate_child(&path, is_symlink)
                .is_err()
            {
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

    /// ディレクトリの子エントリ数 + プレビュー画像 `node_id` (最大 `preview_limit` 件) を取得
    fn scan_child_meta(
        &mut self,
        directory: &Path,
        preview_limit: usize,
    ) -> (usize, Option<Vec<String>>) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return (0, None);
        };

        let mut count = 0usize;
        let mut previews: Vec<String> = Vec::new();

        for entry in entries {
            let Ok(entry) = entry else { continue };
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            // ディレクトリはカウントするがプレビュー対象外
            if ft.is_dir() {
                count += 1;
                continue;
            }
            count += 1;

            // プレビュー収集
            if previews.len() < preview_limit {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                let ext = extract_extension(&name);
                if is_thumbnail_extension(ext) {
                    let path = entry.path();
                    if self
                        .path_security
                        .validate_child(&path, ft.is_symlink())
                        .is_ok()
                    {
                        // symlink 無効時は canonicalize 不要
                        // (validate_child 検証済み + register_resolved 内 find_root_for が最終防壁)
                        let resolved = if self.path_security.is_allow_symlinks() {
                            std::fs::canonicalize(&path).unwrap_or(path)
                        } else {
                            path
                        };
                        if let Ok(id) = self.register_resolved(&resolved) {
                            previews.push(id);
                        }
                    }
                }
            }
        }

        let preview_ids = if previews.is_empty() {
            None
        } else {
            Some(previews)
        };
        (count, preview_ids)
    }

    /// stat 済みエントリから `EntryMeta` を構築する
    #[allow(
        clippy::unnecessary_wraps,
        reason = "Phase 6b で DirIndex 連携時にエラーを返す"
    )]
    fn build_entry_metas(
        &mut self,
        parent: &Path,
        stated: Vec<(PathBuf, EntryKind, Option<Metadata>)>,
    ) -> Result<Vec<EntryMeta>, AppError> {
        let mut entries = Vec::with_capacity(stated.len());

        for (path, kind, meta) in stated {
            // symlink 無効時は canonicalize 不要
            // (scan_entries 内 validate_child 検証済み、canonical 親 + エントリ名で安全)
            let resolved = if self.path_security.is_allow_symlinks() {
                std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone())
            } else {
                path.clone()
            };
            let node_id = self.register_resolved(&resolved)?;
            let name = path
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());

            let (size_bytes, modified_at) = meta.as_ref().map_or((None, None), |m| {
                let size = if kind == EntryKind::Directory {
                    None
                } else {
                    Some(m.len())
                };
                let mtime = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs_f64());
                (size, mtime)
            });

            let mime_type = if kind == EntryKind::Directory {
                None
            } else {
                let lower = name.to_lowercase();
                let ext = extract_extension(&lower);
                mime_for_extension(ext).map(String::from)
            };

            // ディレクトリの child_count と preview_node_ids
            let (child_count, preview_node_ids) = if kind == EntryKind::Directory {
                let (cc, pids) = self.scan_child_meta(&path, 3);
                (Some(cc), pids)
            } else {
                (None, None)
            };

            entries.push(EntryMeta {
                node_id,
                name,
                kind,
                size_bytes,
                mime_type,
                child_count,
                modified_at,
                mtime_ns: None,
                preview_node_ids,
            });
        }

        let _ = parent; // 将来の DirIndex 連携用パラメータ
        Ok(entries)
    }
}
