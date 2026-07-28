//! マウント相対パス計算

use std::path::{Path, PathBuf};

/// 絶対パスからマウント相対パスを計算する
///
/// `mounts` の各ルートに対して `strip_prefix` を試み、
/// `"{mount_id}/{relative}"` 形式 (ルート自身は `mount_id` 単体) で返す。
/// `compute_parent_path_key` / `build_parent_path` と同一書式を維持すること
/// (不一致だと dirty 化キーが browse の照会に一致せず自己修復されない)
pub(super) fn compute_relative_path(path: &Path, mounts: &[(String, PathBuf)]) -> Option<String> {
    for (mount_id, root) in mounts {
        if let Ok(rel) = path.strip_prefix(root) {
            let rel_str = rel.to_string_lossy();
            if rel_str.is_empty() {
                return Some(mount_id.clone());
            }
            if mount_id.is_empty() {
                return Some(rel_str.to_string());
            }
            return Some(format!("{mount_id}/{rel_str}"));
        }
    }
    None
}
