//! `node_id` ↔ 実パス マッピング管理
//!
//! `node_id` は `HMAC-SHA256(secret, relative_path)` の先頭16文字 (hex)。
//! - 同じパスに対して常に同じ `node_id` を返す (冪等)
//! - secret により外部からの推測を防止
//! - クライアントに実パスを公開しない

use std::collections::{HashMap, VecDeque};
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hmac::{Hmac, Mac};
use rayon::prelude::*;
use sha2::Sha256;

use crate::errors::AppError;
use crate::services::extensions::{
    EntryKind, extract_extension, is_thumbnail_extension, mime_for_extension,
};
use crate::services::models::EntryMeta;
use crate::services::natural_sort::natural_sort_key;
use crate::services::path_security::PathSecurity;

type HmacSha256 = Hmac<Sha256>;

/// `node_id` ↔ 実パスのマッピングを管理する
///
/// - HMAC-SHA256 でパスから `node_id` を決定的に生成
/// - 双方向マッピングをメモリに保持
/// - `register()` は内部で `resolve()` を呼ぶが `validate()` は呼ばない
/// - `_generate_id()` 内の `find_root_for()` がルート外パスを拒否する最終防壁
pub(crate) struct NodeRegistry {
    path_security: Arc<PathSecurity>,
    secret: Vec<u8>,
    id_to_path: HashMap<String, PathBuf>,
    path_to_id: HashMap<String, String>,
    // 文字列比較用キャッシュ (root_str, root_prefix, root)
    root_entries: Vec<(String, String, PathBuf)>,
    // マウントポイント名マッピング
    mount_names: HashMap<PathBuf, String>,
    // `mount_id` → 解決済みルートパス (検索結果の `relative_path` 解決用)
    mount_id_map: HashMap<String, PathBuf>,
    // アーカイブエントリ用 LRU
    id_to_archive_entry: HashMap<String, (PathBuf, String)>,
    archive_entry_to_id: HashMap<String, String>,
    id_to_composite_key: HashMap<String, String>,
    archive_order: VecDeque<String>,
    archive_registry_max: usize,
}

impl NodeRegistry {
    /// 新規作成
    pub(crate) fn new(
        path_security: Arc<PathSecurity>,
        archive_registry_max_entries: usize,
        mount_names: HashMap<PathBuf, String>,
    ) -> Self {
        let root_entries = path_security.root_entries().to_vec();

        let secret = std::env::var("NODE_SECRET")
            .unwrap_or_else(|_| "local-viewer-default-secret".to_string())
            .into_bytes();

        Self {
            path_security,
            secret,
            id_to_path: HashMap::new(),
            path_to_id: HashMap::new(),
            root_entries,
            mount_names,
            mount_id_map: HashMap::new(),
            id_to_archive_entry: HashMap::new(),
            archive_entry_to_id: HashMap::new(),
            id_to_composite_key: HashMap::new(),
            archive_order: VecDeque::new(),
            archive_registry_max: archive_registry_max_entries,
        }
    }

    /// `mount_id` → ルートパスのマッピングを設定する (検索結果の `relative_path` 解決用)
    pub(crate) fn set_mount_id_map(&mut self, map: HashMap<String, PathBuf>) {
        self.mount_id_map = map;
    }

    /// `mount_id` → ルートパスのマッピングを参照する
    pub(crate) fn mount_id_map(&self) -> &HashMap<String, PathBuf> {
        &self.mount_id_map
    }

    /// パスの `parent_path_key` (`DirIndex` 用) を計算する
    ///
    /// `"{mount_id}/{relative}"` 形式。ルート直下の場合は `mount_id` のみ。
    /// どのマウントにも属さない場合は `None`。
    pub(crate) fn compute_parent_path_key(&self, dir_path: &Path) -> Option<String> {
        for (mount_id, root) in &self.mount_id_map {
            if let Ok(rel) = dir_path.strip_prefix(root) {
                let rel_str = rel.to_string_lossy();
                if rel_str.is_empty() {
                    return Some(mount_id.clone());
                }
                return Some(format!("{mount_id}/{rel_str}"));
            }
        }
        None
    }

    /// テスト用: secret を明示的に指定して作成
    #[cfg(test)]
    fn with_secret(
        path_security: Arc<PathSecurity>,
        secret: &[u8],
        mount_names: HashMap<PathBuf, String>,
    ) -> Self {
        let root_entries = path_security.root_entries().to_vec();

        Self {
            path_security,
            secret: secret.to_vec(),
            id_to_path: HashMap::new(),
            path_to_id: HashMap::new(),
            root_entries,
            mount_names,
            mount_id_map: HashMap::new(),
            id_to_archive_entry: HashMap::new(),
            archive_entry_to_id: HashMap::new(),
            id_to_composite_key: HashMap::new(),
            archive_order: VecDeque::new(),
            archive_registry_max: 100_000,
        }
    }

    pub(crate) fn path_security(&self) -> &PathSecurity {
        &self.path_security
    }

    /// ロック外で `PathSecurity` を使うための `Arc` クローン取得
    pub(crate) fn path_security_arc(&self) -> Arc<PathSecurity> {
        Arc::clone(&self.path_security)
    }

    /// パスから決定的な `node_id` を生成する (内部用)
    ///
    /// `HMAC-SHA256(secret, "{root}::{relative_path}")` の先頭16文字。
    /// ルートパスを入力に含め、異なるマウントの同名ファイルの衝突を回避。
    fn generate_id(&self, path: &Path) -> Result<String, AppError> {
        let root = self.path_security.find_root_for(path).ok_or_else(|| {
            AppError::path_security(format!(
                "パスがどのルートにも属しません: {}",
                path.display()
            ))
        })?;
        let relative = path
            .strip_prefix(root)
            .map_err(|_| AppError::path_security("相対パスの取得に失敗"))?;
        let hmac_input = format!(
            "{root}::{relative}",
            root = root.display(),
            relative = relative.display()
        );
        Ok(self.hmac_hex(&hmac_input))
    }

    /// パスを登録し、`node_id` を返す
    ///
    /// 既に登録済みならキャッシュから返す。
    /// 外部からの呼び出し用。`resolve()` で正規化する (fail-closed)。
    pub(crate) fn register(&mut self, path: &Path) -> Result<String, AppError> {
        let resolved = std::fs::canonicalize(path).map_err(|_| {
            AppError::path_security(format!("パスの解決に失敗: {}", path.display()))
        })?;
        let key = resolved.to_string_lossy().into_owned();
        if let Some(id) = self.path_to_id.get(&key) {
            return Ok(id.clone());
        }

        let node_id = self.generate_id(&resolved)?;
        self.id_to_path.insert(node_id.clone(), resolved);
        self.path_to_id.insert(key, node_id.clone());
        Ok(node_id)
    }

    /// 検証済み・正規化済みパスを登録する (内部用 fast-path)
    ///
    /// `validate` / `validate_child` 済みのパスのみ渡すこと。
    /// `resolve()` と `relative_to()` をスキップして高速化。
    /// `find_root_for()` でルート外パスを拒否する (TOCTOU 対策)。
    pub(crate) fn register_resolved(&mut self, resolved: &Path) -> Result<String, AppError> {
        let key = resolved.to_string_lossy().into_owned();
        if let Some(id) = self.path_to_id.get(&key) {
            return Ok(id.clone());
        }

        // root ガード: ルート外パスを拒否
        let mut root_str = "";
        let mut rel = "";
        for (rs, rp, _) in &self.root_entries {
            if key == *rs {
                root_str = rs;
                rel = "";
                break;
            }
            if key.starts_with(rp.as_str()) {
                root_str = rs;
                rel = &key[rp.len()..];
                break;
            }
        }
        if root_str.is_empty() {
            return Err(AppError::path_security(format!(
                "パスがどのルートにも属しません: {}",
                resolved.display()
            )));
        }

        let hmac_input = format!("{root_str}::{rel}");
        let node_id = self.hmac_hex(&hmac_input);
        self.id_to_path
            .insert(node_id.clone(), resolved.to_path_buf());
        self.path_to_id.insert(key, node_id.clone());
        Ok(node_id)
    }

    /// `node_id` から実パスを返す
    pub(crate) fn resolve(&self, node_id: &str) -> Result<&Path, AppError> {
        self.id_to_path
            .get(node_id)
            .map(PathBuf::as_path)
            .ok_or_else(|| AppError::node_not_found(node_id))
    }

    /// パスの親ディレクトリの `node_id` を返す
    ///
    /// ルートディレクトリ自体の場合のみ `None` を返す。
    pub(crate) fn get_parent_node_id(&mut self, path: &Path) -> Option<String> {
        let resolved = std::fs::canonicalize(path).ok()?;
        let roots = self.path_security.root_dirs();
        if roots.contains(&resolved) {
            return None;
        }
        let parent = resolved.parent()?;
        self.path_security.validate(parent).ok()?;
        self.register(parent).ok()
    }

    /// パスの祖先エントリを返す (マウントルートから親まで)
    ///
    /// パンくずリスト表示用。現在のディレクトリ自体は含まない。
    pub(crate) fn get_ancestors(&mut self, path: &Path) -> Vec<(String, String)> {
        let Ok(resolved) = std::fs::canonicalize(path) else {
            return vec![];
        };
        let Some(root) = self.path_security.find_root_for(&resolved) else {
            return vec![];
        };
        let root = root.to_path_buf();
        if resolved == root {
            return vec![];
        }

        let mut ancestors: Vec<(String, String)> = Vec::new();
        for ancestor in resolved.ancestors().skip(1) {
            if ancestor == root {
                break;
            }
            let Ok(node_id) = self.register_resolved(ancestor) else {
                continue;
            };
            let name = ancestor.file_name().map_or_else(
                || ancestor.to_string_lossy().into_owned(),
                |n| n.to_string_lossy().into_owned(),
            );
            ancestors.push((node_id, name));
        }

        // マウントルート自体を追加
        let Ok(root_node_id) = self.register_resolved(&root) else {
            return vec![];
        };
        let root_name = self.mount_names.get(&root).cloned().unwrap_or_else(|| {
            root.file_name().map_or_else(
                || root.to_string_lossy().into_owned(),
                |n| n.to_string_lossy().into_owned(),
            )
        });
        ancestors.push((root_node_id, root_name));

        ancestors.reverse();
        ancestors
    }

    // --- アーカイブエントリ対応 ---

    /// アーカイブエントリを登録し `node_id` を返す
    ///
    /// HMAC 入力: `"arc::{root}::{archive_relative}::{entry_name}"`
    /// LRU 方式で上限超過時は最も古い登録を削除。
    pub(crate) fn register_archive_entry(
        &mut self,
        archive_path: &Path,
        entry_name: &str,
    ) -> Result<String, AppError> {
        let composite_key = format!(
            "arc::{archive_path}::{entry_name}",
            archive_path = archive_path.display()
        );
        if let Some(id) = self.archive_entry_to_id.get(&composite_key) {
            // LRU: move to end
            let id_clone = id.clone();
            if let Some(pos) = self.archive_order.iter().position(|x| *x == id_clone) {
                self.archive_order.remove(pos);
            }
            self.archive_order.push_back(id_clone.clone());
            return Ok(id_clone);
        }

        // HMAC でアーカイブ相対パスとエントリ名から node_id を生成
        let resolved = std::fs::canonicalize(archive_path).map_err(|_| {
            AppError::path_security(format!(
                "アーカイブパスの解決に失敗: {}",
                archive_path.display()
            ))
        })?;
        let root = self
            .path_security
            .find_root_for(&resolved)
            .ok_or_else(|| AppError::path_security("アーカイブがどのルートにも属しません"))?;
        let rel = resolved
            .strip_prefix(root)
            .map_err(|_| AppError::path_security("相対パスの取得に失敗"))?;
        let hmac_input = format!(
            "arc::{root}::{rel}::{entry_name}",
            root = root.display(),
            rel = rel.display()
        );
        let node_id = self.hmac_hex(&hmac_input);

        // LRU 上限管理
        while self.id_to_archive_entry.len() >= self.archive_registry_max {
            if let Some(evicted_id) = self.archive_order.pop_front() {
                self.id_to_archive_entry.remove(&evicted_id);
                if let Some(evicted_key) = self.id_to_composite_key.remove(&evicted_id) {
                    self.archive_entry_to_id.remove(&evicted_key);
                }
            } else {
                break;
            }
        }

        self.id_to_archive_entry
            .insert(node_id.clone(), (resolved, entry_name.to_string()));
        self.archive_entry_to_id
            .insert(composite_key.clone(), node_id.clone());
        self.id_to_composite_key
            .insert(node_id.clone(), composite_key);
        self.archive_order.push_back(node_id.clone());
        Ok(node_id)
    }

    /// `node_id` がアーカイブエントリなら `(archive_path, entry_name)` を返す
    pub(crate) fn resolve_archive_entry(&mut self, node_id: &str) -> Option<(PathBuf, String)> {
        let result = self.id_to_archive_entry.get(node_id)?.clone();
        // LRU: move to end
        if let Some(pos) = self.archive_order.iter().position(|x| x == node_id) {
            self.archive_order.remove(pos);
        }
        self.archive_order.push_back(node_id.to_owned());
        Some(result)
    }

    /// `node_id` がアーカイブエントリかどうか
    pub(crate) fn is_archive_entry(&self, node_id: &str) -> bool {
        self.id_to_archive_entry.contains_key(node_id)
    }

    /// HMAC-SHA256 の先頭 16 hex 文字を返す
    fn hmac_hex(&self, input: &str) -> String {
        #[allow(
            clippy::expect_used,
            reason = "HMAC-SHA256 は任意長の鍵を受け付ける (Sha256 には鍵長制限なし)"
        )]
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).expect("HMAC は任意長の鍵を受け付ける");
        mac.update(input.as_bytes());
        let result = mac.finalize().into_bytes();
        let mut h = hex::encode(result);
        h.truncate(16);
        h
    }

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
                        let resolved = std::fs::canonicalize(&path).unwrap_or(path);
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
            let resolved = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
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
                preview_node_ids,
            });
        }

        let _ = parent; // 将来の DirIndex 連携用パラメータ
        Ok(entries)
    }

    // --- Two-Phase Lock Splitting: Phase 2 メソッド ---

    /// Phase 1 で収集した `ScannedEntry` を登録し `EntryMeta` に変換する (短時間ロック内)
    ///
    /// filesystem I/O は一切行わない。純粋な `HashMap` 操作のみ。
    pub(crate) fn register_scanned_entries(
        &mut self,
        scanned: Vec<ScannedEntry>,
    ) -> Result<Vec<EntryMeta>, AppError> {
        let mut entries = Vec::with_capacity(scanned.len());
        for se in scanned {
            let node_id = self.register_resolved(&se.path)?;

            // プレビューパスの登録
            let preview_node_ids = se.preview_paths.and_then(|paths| {
                let ids: Vec<String> = paths
                    .iter()
                    .filter_map(|p| self.register_resolved(p).ok())
                    .collect();
                if ids.is_empty() { None } else { Some(ids) }
            });

            entries.push(EntryMeta {
                node_id,
                name: se.name,
                kind: se.kind,
                size_bytes: se.size_bytes,
                mime_type: se.mime_type,
                child_count: se.child_count,
                modified_at: se.modified_at,
                preview_node_ids,
            });
        }
        Ok(entries)
    }

    /// canonicalize 済みパスから祖先を取得する (canonicalize をスキップ)
    ///
    /// `find_root_for` で root を特定し、`register_resolved` で登録。
    pub(crate) fn get_ancestors_from_resolved(&mut self, resolved: &Path) -> Vec<(String, String)> {
        let Some(root) = self.path_security.find_root_for(resolved) else {
            return vec![];
        };
        let root = root.to_path_buf();
        if *resolved == root {
            return vec![];
        }

        let mut ancestors: Vec<(String, String)> = Vec::new();
        for ancestor in resolved.ancestors().skip(1) {
            if ancestor == root {
                break;
            }
            let Ok(node_id) = self.register_resolved(ancestor) else {
                continue;
            };
            let name = ancestor.file_name().map_or_else(
                || ancestor.to_string_lossy().into_owned(),
                |n| n.to_string_lossy().into_owned(),
            );
            ancestors.push((node_id, name));
        }

        // マウントルート自体を追加
        let Ok(root_node_id) = self.register_resolved(&root) else {
            return vec![];
        };
        let root_name = self.mount_names.get(&root).cloned().unwrap_or_else(|| {
            root.file_name().map_or_else(
                || root.to_string_lossy().into_owned(),
                |n| n.to_string_lossy().into_owned(),
            )
        });
        ancestors.push((root_node_id, root_name));

        ancestors.reverse();
        ancestors
    }
}

/// ページングオプション
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
                    let resolved = std::fs::canonicalize(&path).unwrap_or(path);
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
pub(crate) fn scan_entry_metas(
    path_security: &PathSecurity,
    stated: Vec<(PathBuf, EntryKind, Option<Metadata>)>,
    preview_limit: usize,
) -> Vec<ScannedEntry> {
    stated
        .into_iter()
        .map(|(path, kind, meta)| {
            let resolved = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
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

            let (child_count, preview_paths) = if kind == EntryKind::Directory {
                let cm = scan_child_meta(path_security, &path, preview_limit);
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
                preview_paths,
            }
        })
        .collect()
}

/// 200 件超で rayon 並列 stat
const PARALLEL_STAT_THRESHOLD: usize = 200;

fn stat_entries(raw: &[(PathBuf, EntryKind, bool)]) -> Vec<(PathBuf, EntryKind, Option<Metadata>)> {
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use tempfile::TempDir;

    const TEST_SECRET: &[u8] = b"local-viewer-default-secret";

    struct TestEnv {
        #[allow(dead_code, reason = "TempDir のドロップでディレクトリを保持")]
        dir: TempDir,
        root: PathBuf,
    }

    impl TestEnv {
        fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let root = fs::canonicalize(dir.path()).unwrap();
            fs::write(root.join("file.txt"), "hello").unwrap();
            fs::create_dir_all(root.join("subdir")).unwrap();
            fs::write(root.join("subdir/nested.txt"), "nested").unwrap();
            Self { dir, root }
        }

        fn registry(&self) -> NodeRegistry {
            let ps = Arc::new(PathSecurity::new(vec![self.root.clone()], false).unwrap());
            NodeRegistry::with_secret(ps, TEST_SECRET, HashMap::new())
        }
    }

    // --- 基本 register / resolve ---

    #[test]
    fn パスを登録してnode_idを返す() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let id = reg.register(&env.root.join("file.txt")).unwrap();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn 同じパスに対して同じnode_idを返す() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let id1 = reg.register(&env.root.join("file.txt")).unwrap();
        let id2 = reg.register(&env.root.join("file.txt")).unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn 異なるパスに対して異なるnode_idを返す() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let id1 = reg.register(&env.root.join("file.txt")).unwrap();
        let id2 = reg.register(&env.root.join("subdir/nested.txt")).unwrap();
        assert_ne!(id1, id2);
    }

    #[test]
    fn node_idから元のパスを解決する() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let file_path = env.root.join("file.txt");
        let id = reg.register(&file_path).unwrap();
        let resolved = reg.resolve(&id).unwrap();
        assert_eq!(resolved, fs::canonicalize(&file_path).unwrap());
    }

    #[test]
    fn 未登録のnode_idでnot_foundエラー() {
        let env = TestEnv::new();
        let reg = env.registry();
        let err = reg.resolve("nonexistent").unwrap_err();
        assert!(err.to_string().contains("見つかりません"));
    }

    // --- register_resolved ---

    #[test]
    fn register_resolvedがregisterと同じnode_idを返す() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let resolved = fs::canonicalize(env.root.join("file.txt")).unwrap();
        let id1 = reg.register(&env.root.join("file.txt")).unwrap();
        let id2 = reg.register_resolved(&resolved).unwrap();
        assert_eq!(id1, id2);
    }

    // --- get_parent_node_id ---

    #[test]
    fn 親のnode_idが取得できる() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let parent_id = reg.get_parent_node_id(&env.root.join("subdir/nested.txt"));
        assert!(parent_id.is_some());
    }

    #[test]
    fn root_dirの親はnone() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let parent_id = reg.get_parent_node_id(&env.root);
        assert!(parent_id.is_none());
    }

    // --- get_ancestors ---

    #[test]
    fn ルートディレクトリのancestorsが空リストを返す() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let ancestors = reg.get_ancestors(&env.root);
        assert!(ancestors.is_empty());
    }

    #[test]
    fn ルート直下ディレクトリのancestorsがルートのみを含む() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let ancestors = reg.get_ancestors(&env.root.join("subdir"));
        assert_eq!(ancestors.len(), 1);
    }

    #[test]
    fn 深い階層のancestorsが全祖先を含む() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let ancestors = reg.get_ancestors(&env.root.join("subdir/nested.txt"));
        // ルート + subdir = 2 件
        assert_eq!(ancestors.len(), 2);
    }

    #[test]
    fn ancestorsの順序がルートから親へ正しい() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let ancestors = reg.get_ancestors(&env.root.join("subdir/nested.txt"));
        // 先頭がルート
        assert_eq!(
            ancestors[0].1,
            env.root.file_name().unwrap().to_string_lossy()
        );
        assert_eq!(ancestors[1].1, "subdir");
    }

    #[test]
    fn ancestorsのルートエントリ名がmount_namesを反映する() {
        let env = TestEnv::new();
        let ps = Arc::new(PathSecurity::new(vec![env.root.clone()], false).unwrap());
        let mut names = HashMap::new();
        let canonical_root = fs::canonicalize(&env.root).unwrap();
        names.insert(canonical_root, "My Pictures".to_string());
        let mut reg = NodeRegistry::with_secret(ps, TEST_SECRET, names);
        let ancestors = reg.get_ancestors(&env.root.join("subdir"));
        assert_eq!(ancestors[0].1, "My Pictures");
    }

    // --- アーカイブエントリ ---

    #[test]
    fn アーカイブエントリを登録してnode_idを返す() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        // アーカイブファイルを作成 (テスト用)
        let archive = env.root.join("test.zip");
        fs::write(&archive, "fake zip").unwrap();
        let id = reg.register_archive_entry(&archive, "page01.jpg").unwrap();
        assert_eq!(id.len(), 16);
    }

    #[test]
    fn 同じアーカイブエントリに対して同じnode_idを返す() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let archive = env.root.join("test.zip");
        fs::write(&archive, "fake zip").unwrap();
        let id1 = reg.register_archive_entry(&archive, "page01.jpg").unwrap();
        let id2 = reg.register_archive_entry(&archive, "page01.jpg").unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn アーカイブエントリのnode_idを解決できる() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let archive = env.root.join("test.zip");
        fs::write(&archive, "fake zip").unwrap();
        let id = reg.register_archive_entry(&archive, "page01.jpg").unwrap();
        let (path, entry) = reg.resolve_archive_entry(&id).unwrap();
        assert_eq!(path, fs::canonicalize(&archive).unwrap());
        assert_eq!(entry, "page01.jpg");
    }

    #[test]
    fn is_archive_entryが正しく判定する() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let archive = env.root.join("test.zip");
        fs::write(&archive, "fake zip").unwrap();
        let arc_id = reg.register_archive_entry(&archive, "p.jpg").unwrap();
        let file_id = reg.register(&env.root.join("file.txt")).unwrap();
        assert!(reg.is_archive_entry(&arc_id));
        assert!(!reg.is_archive_entry(&file_id));
    }

    #[test]
    fn アーカイブエントリ上限超過で最古エントリがevictされる() {
        let env = TestEnv::new();
        let ps = Arc::new(PathSecurity::new(vec![env.root.clone()], false).unwrap());
        let mut reg = NodeRegistry::with_secret(ps, TEST_SECRET, HashMap::new());
        reg.archive_registry_max = 2;

        let archive = env.root.join("test.zip");
        fs::write(&archive, "fake zip").unwrap();

        let id1 = reg.register_archive_entry(&archive, "p1.jpg").unwrap();
        let _id2 = reg.register_archive_entry(&archive, "p2.jpg").unwrap();
        // 3 番目の登録で id1 が evict される
        let _id3 = reg.register_archive_entry(&archive, "p3.jpg").unwrap();

        assert!(!reg.is_archive_entry(&id1));
    }

    // --- list_directory ---

    struct ListTestEnv {
        #[allow(dead_code, reason = "TempDir のドロップでディレクトリを保持")]
        dir: TempDir,
        root: PathBuf,
    }

    impl ListTestEnv {
        fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let root = fs::canonicalize(dir.path()).unwrap();
            // ファイル
            fs::write(root.join("image1.jpg"), "jpg").unwrap();
            fs::write(root.join("image2.png"), "png").unwrap();
            fs::write(root.join("video.mp4"), "mp4").unwrap();
            fs::write(root.join("doc.pdf"), "pdf").unwrap();
            fs::write(root.join("readme.txt"), "txt").unwrap();
            // サブディレクトリ (画像入り)
            fs::create_dir_all(root.join("subdir")).unwrap();
            fs::write(root.join("subdir/inner.jpg"), "inner").unwrap();
            fs::write(root.join("subdir/inner2.png"), "inner2").unwrap();
            // 空ディレクトリ
            fs::create_dir_all(root.join("empty")).unwrap();
            Self { dir, root }
        }

        fn registry(&self) -> NodeRegistry {
            let ps = Arc::new(PathSecurity::new(vec![self.root.clone()], false).unwrap());
            NodeRegistry::with_secret(ps, TEST_SECRET, HashMap::new())
        }
    }

    #[test]
    fn list_directoryが全エントリを返す() {
        let env = ListTestEnv::new();
        let mut reg = env.registry();
        let entries = reg.list_directory(&env.root).unwrap();
        // image1.jpg, image2.png, video.mp4, doc.pdf, readme.txt, subdir, empty = 7
        assert_eq!(entries.len(), 7);
    }

    #[test]
    fn list_directoryでファイルが正しくclassifyされる() {
        let env = ListTestEnv::new();
        let mut reg = env.registry();
        let entries = reg.list_directory(&env.root).unwrap();
        let image_count = entries
            .iter()
            .filter(|e| e.kind == EntryKind::Image)
            .count();
        assert_eq!(image_count, 2);
        let video_count = entries
            .iter()
            .filter(|e| e.kind == EntryKind::Video)
            .count();
        assert_eq!(video_count, 1);
        let dir_count = entries
            .iter()
            .filter(|e| e.kind == EntryKind::Directory)
            .count();
        assert_eq!(dir_count, 2);
    }

    #[test]
    fn ディレクトリのchild_countが正しい() {
        let env = ListTestEnv::new();
        let mut reg = env.registry();
        let entries = reg.list_directory(&env.root).unwrap();
        let subdir = entries.iter().find(|e| e.name == "subdir").unwrap();
        assert_eq!(subdir.child_count, Some(2)); // inner.jpg, inner2.png
    }

    #[test]
    fn preview_node_idsが画像を含む() {
        let env = ListTestEnv::new();
        let mut reg = env.registry();
        let entries = reg.list_directory(&env.root).unwrap();
        let subdir = entries.iter().find(|e| e.name == "subdir").unwrap();
        let previews = subdir.preview_node_ids.as_ref().unwrap();
        assert!(!previews.is_empty());
        assert!(previews.len() <= 3);
    }

    #[test]
    fn 空ディレクトリのpreview_node_idsがnone() {
        let env = ListTestEnv::new();
        let mut reg = env.registry();
        let entries = reg.list_directory(&env.root).unwrap();
        let empty = entries.iter().find(|e| e.name == "empty").unwrap();
        assert_eq!(empty.child_count, Some(0));
        assert!(empty.preview_node_ids.is_none());
    }

    #[test]
    fn modified_atがposix秒で設定される() {
        let env = ListTestEnv::new();
        let mut reg = env.registry();
        let entries = reg.list_directory(&env.root).unwrap();
        let file = entries.iter().find(|e| e.name == "image1.jpg").unwrap();
        assert!(file.modified_at.is_some());
        // 2020年以降の値であること (POSIX 秒)
        assert!(file.modified_at.unwrap() > 1_577_836_800.0);
    }

    #[test]
    fn 空ディレクトリのlist_directoryが空リストを返す() {
        let env = ListTestEnv::new();
        let mut reg = env.registry();
        let entries = reg.list_directory(&env.root.join("empty")).unwrap();
        assert!(entries.is_empty());
    }

    // --- list_directory_page ---

    #[test]
    fn list_directory_pageでlimit件分のみ返す() {
        let env = ListTestEnv::new();
        let mut reg = env.registry();
        let opts = PageOptions {
            limit: 3,
            cursor_node_id: None,
            reverse: false,
        };
        let (entries, total) = reg.list_directory_page(&env.root, &opts).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(total, 7);
    }

    #[test]
    fn list_directory_pageの合計件数が全エントリ数() {
        let env = ListTestEnv::new();
        let mut reg = env.registry();
        let opts = PageOptions {
            limit: 100,
            cursor_node_id: None,
            reverse: false,
        };
        let (entries, total) = reg.list_directory_page(&env.root, &opts).unwrap();
        assert_eq!(entries.len(), 7);
        assert_eq!(total, 7);
    }

    // --- list_mount_roots ---

    #[test]
    fn list_mount_rootsが全ルートを返す() {
        let env = ListTestEnv::new();
        let mut reg = env.registry();
        let roots = reg.list_mount_roots();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].kind, EntryKind::Directory);
    }

    #[test]
    fn list_mount_rootsのnameがmount_namesを反映() {
        let env = ListTestEnv::new();
        let ps = Arc::new(PathSecurity::new(vec![env.root.clone()], false).unwrap());
        let mut names = HashMap::new();
        names.insert(env.root.clone(), "My Pictures".to_string());
        let mut reg = NodeRegistry::with_secret(ps, TEST_SECRET, names);
        let roots = reg.list_mount_roots();
        assert_eq!(roots[0].name, "My Pictures");
    }

    #[test]
    fn list_mount_rootsにchild_countが含まれる() {
        let env = ListTestEnv::new();
        let mut reg = env.registry();
        let roots = reg.list_mount_roots();
        assert!(roots[0].child_count.is_some());
        assert_eq!(roots[0].child_count, Some(7));
    }

    // --- HMAC ゴールデンベクターテスト ---

    /// HMAC 入力文字列と期待される `node_id` を直接テストするヘルパー
    fn compute_hmac(input: &str) -> String {
        let mut mac =
            HmacSha256::new_from_slice(TEST_SECRET).expect("HMAC は任意長の鍵を受け付ける");
        mac.update(input.as_bytes());
        let result = mac.finalize().into_bytes();
        let mut h = hex::encode(result);
        h.truncate(16);
        h
    }

    #[test]
    fn hmac_通常ファイルのゴールデンベクター() {
        // Python で生成済みベクター (secret = b"local-viewer-default-secret")
        assert_eq!(
            compute_hmac("/mnt/data::photos/img001.jpg"),
            "cc420505916e01d4"
        );
        assert_eq!(compute_hmac("/mnt/data::"), "0b27cf020d2e8dff");
        assert_eq!(
            compute_hmac("/mnt/data::subdir/deep/file.png"),
            "ae53c4d5e3a72c78"
        );
        assert_eq!(
            compute_hmac("/mnt/data::日本語ファイル.jpg"),
            "b143bc31a26f1350"
        );
        assert_eq!(
            compute_hmac("/mnt/data::file with spaces.jpg"),
            "7acc8ddcceec554e"
        );
        assert_eq!(
            compute_hmac("/mnt/archive::zips/images.zip"),
            "299db1b9e7104f0e"
        );
    }

    #[test]
    fn hmac_アーカイブエントリのゴールデンベクター() {
        assert_eq!(
            compute_hmac("arc::/mnt/data::archive.zip::page01.jpg"),
            "fdc8fc764a07d9e9"
        );
        assert_eq!(
            compute_hmac("arc::/mnt/data::nested/comic.cbz::img/001.png"),
            "bb2c42b499b6d6f2"
        );
        assert_eq!(
            compute_hmac("arc::/mnt/data::test.zip::日本語/画像.jpg"),
            "27a6131445f16976"
        );
    }

    // --- register_resolved root ガード ---

    #[test]
    fn register_resolvedがルート外パスでエラーを返す() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let err = reg.register_resolved(Path::new("/nonexistent/path"));
        assert!(err.is_err());
    }

    // --- Two-Phase free functions ---

    #[test]
    fn scan_entriesがディレクトリ内エントリを返す() {
        let env = ListTestEnv::new();
        let ps = Arc::new(PathSecurity::new(vec![env.root.clone()], false).unwrap());
        let entries = scan_entries(&ps, &env.root).unwrap();
        assert_eq!(entries.len(), 7);
    }

    #[test]
    fn scan_child_metaが子エントリ数とプレビューパスを返す() {
        let env = ListTestEnv::new();
        let ps = Arc::new(PathSecurity::new(vec![env.root.clone()], false).unwrap());
        let cm = scan_child_meta(&ps, &env.root.join("subdir"), 3);
        assert_eq!(cm.count, 2); // inner.jpg, inner2.png
        assert!(!cm.preview_paths.is_empty());
        assert!(cm.preview_paths.len() <= 3);
    }

    #[test]
    fn scan_entry_metasがcanonicalize済みパスを持つ() {
        let env = ListTestEnv::new();
        let ps = Arc::new(PathSecurity::new(vec![env.root.clone()], false).unwrap());
        let raw = scan_entries(&ps, &env.root).unwrap();
        let stated = stat_entries(&raw);
        let scanned = scan_entry_metas(&ps, stated, 3);
        assert_eq!(scanned.len(), 7);
        // 全パスが絶対パス
        assert!(scanned.iter().all(|s| s.path.is_absolute()));
    }

    #[test]
    fn register_scanned_entriesがlist_directoryと同じ結果を返す() {
        let env = ListTestEnv::new();
        let ps = Arc::new(PathSecurity::new(vec![env.root.clone()], false).unwrap());

        // Two-Phase パス
        let raw = scan_entries(&ps, &env.root).unwrap();
        let stated = stat_entries(&raw);
        let scanned = scan_entry_metas(&ps, stated, 3);
        let mut reg1 = env.registry();
        let two_phase = reg1.register_scanned_entries(scanned).unwrap();

        // 既存パス
        let mut reg2 = env.registry();
        let legacy = reg2.list_directory(&env.root).unwrap();

        // エントリ数が一致
        assert_eq!(two_phase.len(), legacy.len());
        // 全 node_id が一致 (順序はファイルシステム依存なので名前でソート)
        let mut tp_ids: Vec<_> = two_phase
            .iter()
            .map(|e| (e.name.as_str(), e.node_id.as_str()))
            .collect();
        let mut lg_ids: Vec<_> = legacy
            .iter()
            .map(|e| (e.name.as_str(), e.node_id.as_str()))
            .collect();
        tp_ids.sort_by_key(|(n, _)| *n);
        lg_ids.sort_by_key(|(n, _)| *n);
        assert_eq!(tp_ids, lg_ids);
    }

    // --- get_ancestors_from_resolved ---

    #[test]
    fn get_ancestors_from_resolvedがget_ancestorsと同じ結果を返す() {
        let env = TestEnv::new();
        let mut reg1 = env.registry();
        let mut reg2 = env.registry();
        let subdir = fs::canonicalize(env.root.join("subdir/nested.txt")).unwrap();
        let anc1 = reg1.get_ancestors(&subdir);
        let anc2 = reg2.get_ancestors_from_resolved(&subdir);
        assert_eq!(anc1, anc2);
    }
}
