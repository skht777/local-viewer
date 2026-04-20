//! `FileWatcher` サブモジュールの単体テスト

#![allow(
    non_snake_case,
    reason = "日本語テスト名で振る舞いを記述する規約 (07_testing.md)"
)]

use std::path::{Path, PathBuf};

use rstest::rstest;

use super::filter::{is_hidden_under_mounts, is_indexable_extension};
use super::path_utils::compute_relative_path;

// --- is_hidden_under_mounts ---

fn pictures_mount() -> Vec<(String, PathBuf)> {
    vec![("pictures".to_string(), PathBuf::from("/data/pictures"))]
}

#[rstest]
// マウント直下のファイル名が . 始まり (既存ケース)
#[case("/data/pictures/.hidden", true)]
#[case("/data/pictures/.gitignore", true)]
// 通常ケース
#[case("/data/pictures/visible.txt", false)]
#[case("/data/pictures/dir/file.zip", false)]
// 親 hidden + 子は通常名 (本改修で拾うケース)
#[case("/data/pictures/.hidden/foo.mp4", true)]
// 中間 hidden (より深いネスト)
#[case("/data/pictures/dir/.hidden/sub/foo.mp4", true)]
fn 隠しファイルのフィルタリングが正しく動作する(
    #[case] path: &str,
    #[case] expected: bool,
) {
    assert_eq!(
        is_hidden_under_mounts(Path::new(path), &pictures_mount()),
        expected,
    );
}

#[test]
fn マウント外パスは安全側で隠し扱いにする() {
    // FileWatcher は本来マウント配下のみ監視するため、マウント外は fail-safe で hidden
    assert!(is_hidden_under_mounts(
        Path::new("/other/path/file.txt"),
        &pictures_mount(),
    ));
}

#[test]
fn マウントルート自身がドット始まりでも配下は通常パスなら通す() {
    // parallel_walk::scan_one の BFS 起点が skip_hidden 対象外なのと一致させる
    let mounts = vec![("archive".to_string(), PathBuf::from("/data/.archive"))];
    assert!(!is_hidden_under_mounts(
        Path::new("/data/.archive/album/pic.jpg"),
        &mounts,
    ));
    // 配下に . 始まりがあれば hidden
    assert!(is_hidden_under_mounts(
        Path::new("/data/.archive/.secret/pic.jpg"),
        &mounts,
    ));
}

// --- compute_relative_path ---

#[test]
fn compute_relative_pathが正しくパスを解決する() {
    let mounts = vec![
        ("pictures".to_string(), PathBuf::from("/data/pictures")),
        ("videos".to_string(), PathBuf::from("/data/videos")),
    ];

    // マウント内のパス → mount_id/relative 形式
    assert_eq!(
        compute_relative_path(Path::new("/data/pictures/album/photo.jpg"), &mounts),
        Some("pictures/album/photo.jpg".to_string()),
    );

    // 別のマウント
    assert_eq!(
        compute_relative_path(Path::new("/data/videos/movie.mp4"), &mounts),
        Some("videos/movie.mp4".to_string()),
    );

    // マウント外のパス → None
    assert_eq!(
        compute_relative_path(Path::new("/other/path/file.txt"), &mounts),
        None,
    );
}

#[test]
fn compute_relative_pathが空mount_idで正しく動作する() {
    let mounts = vec![(String::new(), PathBuf::from("/data"))];

    assert_eq!(
        compute_relative_path(Path::new("/data/subdir/file.zip"), &mounts),
        Some("subdir/file.zip".to_string()),
    );
}

// --- is_indexable_extension ---

#[rstest]
#[case(".mp4", true)]
#[case(".mkv", true)]
#[case(".zip", true)]
#[case(".rar", true)]
#[case(".7z", true)]
#[case(".cbz", true)]
#[case(".pdf", true)]
#[case(".jpg", false)]
#[case(".png", false)]
#[case(".txt", false)]
#[case(".exe", false)]
#[case("", false)]
fn is_indexable_extensionが正しく判定する(#[case] ext: &str, #[case] expected: bool) {
    assert_eq!(is_indexable_extension(ext), expected);
}

// --- AppState.file_watcher slot の所有権 (Phase D0) ---

/// `FileWatcher` を `Arc<Mutex<Option<FileWatcher>>>` slot に保存できること
///
/// 旧実装は `std::mem::forget(file_watcher)` で leak していたが、Phase D0 以降は
/// `AppState` の slot に保持される。`take()` → `stop()` → `replace()` の
/// ライフサイクル操作（hot reload 用）が成立することを最小単位で確認する。
#[test]
fn FileWatcherはslotにtakeとreplaceで出し入れできる() {
    use std::sync::{Arc, Mutex};

    use crate::services::dir_index::DirIndex;
    use crate::services::indexer::Indexer;
    use crate::services::path_security::PathSecurity;
    use crate::services::rebuild_guard::RebuildGuard;

    // 最小依存: 未起動の FileWatcher を 2 つ作って slot に出し入れする
    let dir = tempfile::TempDir::new().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let ps = Arc::new(PathSecurity::new(vec![root.clone()], false).unwrap());
    let indexer = Arc::new(Indexer::new(":memory:"));
    let dir_index = Arc::new(DirIndex::new(":memory:"));
    let rebuild_guard = Arc::new(RebuildGuard::new());

    let fw1 = super::FileWatcher::new(
        Arc::clone(&indexer),
        Arc::clone(&ps),
        Arc::clone(&dir_index),
        vec![("deadbeefcafe0001".to_string(), root.clone())],
        Arc::clone(&rebuild_guard),
    );

    let slot: Arc<Mutex<Option<super::FileWatcher>>> = Arc::new(Mutex::new(None));
    // 保存 → 取り出し → 差し替え
    slot.lock().unwrap().replace(fw1);
    assert!(slot.lock().unwrap().is_some());

    let taken = slot.lock().unwrap().take();
    assert!(taken.is_some());
    assert!(slot.lock().unwrap().is_none());

    let fw2 = super::FileWatcher::new(
        indexer,
        ps,
        dir_index,
        vec![("deadbeefcafe0002".to_string(), root)],
        rebuild_guard,
    );
    slot.lock().unwrap().replace(fw2);
    assert!(slot.lock().unwrap().is_some());
}
