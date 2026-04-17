//! マウント相対パス計算

use std::path::{Path, PathBuf};

/// 絶対パスからマウント相対パスを計算する
///
/// `mounts` の各ルートに対して `strip_prefix` を試み、
/// `mount_id` が空でなければ `"{mount_id}/{relative}"` 形式で返す
pub(super) fn compute_relative_path(path: &Path, mounts: &[(String, PathBuf)]) -> Option<String> {
    for (mount_id, root) in mounts {
        if let Ok(rel) = path.strip_prefix(root) {
            let rel_str = rel.to_string_lossy();
            if mount_id.is_empty() {
                return Some(rel_str.to_string());
            }
            return Some(format!("{mount_id}/{rel_str}"));
        }
    }
    None
}
