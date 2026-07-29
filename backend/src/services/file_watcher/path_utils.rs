//! マウント相対パス計算

use std::path::{Path, PathBuf};

/// 絶対パスからマウント相対パスを計算する
///
/// `mounts` の各ルートに対して `strip_prefix` を試み、
/// `"{mount_id}/{relative}"` 形式 (ルート自身は `mount_id` 単体) で返す。
/// `compute_parent_path_key` / `build_parent_path` と同一書式・同一選択規則を
/// 維持すること (不一致だと dirty 化キーが browse の照会に一致せず自己修復されない)。
///
/// ネストしたマウント (`/a` と `/a/b`) では**最長一致**、同一深さの衝突は
/// `mount_id` 昇順で決定的に選ぶ (`NodeRegistry::compute_parent_path_key` と同一)。
pub(super) fn compute_relative_path(path: &Path, mounts: &[(String, PathBuf)]) -> Option<String> {
    let mut best: Option<(&String, &Path)> = None;
    let mut best_root_len = 0usize;
    for (mount_id, root) in mounts {
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let root_len = root.as_os_str().len();
        let is_better = best.is_none_or(|(best_id, _)| {
            root_len > best_root_len || (root_len == best_root_len && mount_id < best_id)
        });
        if is_better {
            best_root_len = root_len;
            best = Some((mount_id, rel));
        }
    }
    let (mount_id, rel) = best?;
    let rel_str = rel.to_string_lossy();
    if rel_str.is_empty() {
        return Some(mount_id.clone());
    }
    if mount_id.is_empty() {
        return Some(rel_str.to_string());
    }
    Some(format!("{mount_id}/{rel_str}"))
}

#[cfg(test)]
#[allow(
    non_snake_case,
    reason = "日本語テスト名で振る舞いを記述する規約 (07_testing.md)"
)]
mod tests {
    use super::*;

    #[test]
    fn compute_relative_pathはネストマウントで最長一致を選ぶ() {
        // /data と /data/inner を両方マウントすると、mounts の並び順 (HashMap 由来) 次第で
        // dirty 化キーが変わり browse の compute_parent_path_key と食い違っていた
        let outer_first = vec![
            ("outer".to_string(), PathBuf::from("/data")),
            ("inner".to_string(), PathBuf::from("/data/inner")),
        ];
        let inner_first = vec![
            ("inner".to_string(), PathBuf::from("/data/inner")),
            ("outer".to_string(), PathBuf::from("/data")),
        ];
        let target = Path::new("/data/inner/album");

        assert_eq!(
            compute_relative_path(target, &outer_first).as_deref(),
            Some("inner/album")
        );
        assert_eq!(
            compute_relative_path(target, &inner_first).as_deref(),
            Some("inner/album"),
            "並び順に依らず最長一致のマウントを選ぶべき"
        );
    }

    #[test]
    fn compute_relative_pathは同一rootの衝突をmount_id昇順で解決する() {
        let mounts = vec![
            ("zzzz".to_string(), PathBuf::from("/data")),
            ("aaaa".to_string(), PathBuf::from("/data")),
        ];
        assert_eq!(
            compute_relative_path(Path::new("/data/album"), &mounts).as_deref(),
            Some("aaaa/album")
        );
    }

    #[test]
    fn compute_relative_pathはルート自身でmount_idを返す() {
        let mounts = vec![("m1".to_string(), PathBuf::from("/data"))];
        assert_eq!(
            compute_relative_path(Path::new("/data"), &mounts).as_deref(),
            Some("m1")
        );
    }
}
