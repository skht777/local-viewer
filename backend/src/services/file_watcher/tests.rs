//! `FileWatcher` サブモジュールの単体テスト

#![allow(
    non_snake_case,
    reason = "日本語テスト名で振る舞いを記述する規約 (07_testing.md)"
)]

use std::path::{Path, PathBuf};

use rstest::rstest;

use super::filter::is_hidden_under_mounts;
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

// --- enqueue (dirty 化の入口) ---

fn empty_pending() -> std::sync::Mutex<std::collections::HashMap<String, String>> {
    std::sync::Mutex::new(std::collections::HashMap::new())
}

#[rstest]
// 画像: FTS 対象外だが DirIndex の自己修復 (dirty 化) に必須
#[case("photo.jpg")]
#[case("cover.png")]
// FTS 対象 (従来から通過)
#[case("video.mp4")]
#[case("book.zip")]
// DirIndex は全エントリを保持するため対象外拡張子も通す
#[case("notes.txt")]
// 拡張子なし (ディレクトリ想定、従来から通過)
#[case("album")]
fn enqueueは隠しでないパスをすべてpendingに追加する(#[case] name: &str) {
    let pending = empty_pending();
    let path = format!("/data/pictures/album/{name}");
    super::filter::enqueue(&pending, Path::new(&path), "add", &pictures_mount());
    assert_eq!(
        pending.lock().unwrap().len(),
        1,
        "{name} は pending に追加されるべき"
    );
}

#[test]
fn enqueueは隠しファイルをpendingに追加しない() {
    let pending = empty_pending();
    super::filter::enqueue(
        &pending,
        Path::new("/data/pictures/.hidden.jpg"),
        "add",
        &pictures_mount(),
    );
    assert!(pending.lock().unwrap().is_empty());
}

// --- process_event (FTS 反映と dirty 化の分離) ---

/// `process_event` 用の最小フィクスチャ (実ファイルシステム + 一時 DB)
fn worker_fixture() -> (
    tempfile::TempDir,
    PathBuf,
    crate::services::path_security::PathSecurity,
    crate::services::indexer::Indexer,
    crate::services::dir_index::DirIndex,
    tempfile::NamedTempFile,
    tempfile::NamedTempFile,
) {
    let dir = tempfile::TempDir::new().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let ps = crate::services::path_security::PathSecurity::new(vec![root.clone()], false).unwrap();
    let indexer_db = tempfile::NamedTempFile::new().unwrap();
    let indexer = crate::services::indexer::Indexer::new(indexer_db.path().to_str().unwrap());
    indexer.init_db().unwrap();
    let dir_index_db = tempfile::NamedTempFile::new().unwrap();
    let dir_index =
        crate::services::dir_index::DirIndex::new(dir_index_db.path().to_str().unwrap());
    dir_index.init_db().unwrap();
    (dir, root, ps, indexer, dir_index, indexer_db, dir_index_db)
}

#[test]
fn 画像ファイルの追加イベントで親がdirty化されFTSには追加されない() {
    let (_dir, root, ps, indexer, dir_index, _db1, _db2) = worker_fixture();
    std::fs::create_dir(root.join("album")).unwrap();
    let img = root.join("album/photo.jpg");
    std::fs::write(&img, b"x").unwrap();
    let mounts = vec![("pics".to_string(), root.clone())];

    super::worker::process_event(
        &indexer,
        &ps,
        &dir_index,
        &mounts,
        &img.to_string_lossy(),
        "add",
    );

    assert!(
        dir_index.is_dir_dirty("pics/album"),
        "画像追加で親ディレクトリが dirty 化されるべき"
    );
    assert_eq!(
        indexer.entry_count().unwrap(),
        0,
        "画像は FTS インデックス対象外のまま"
    );
}

#[test]
fn 画像ファイルの削除イベントでも親がdirty化される() {
    let (_dir, root, ps, indexer, dir_index, _db1, _db2) = worker_fixture();
    std::fs::create_dir(root.join("album")).unwrap();
    // remove イベントでは対象パスは既に存在しない
    let img = root.join("album/photo.jpg");
    let mounts = vec![("pics".to_string(), root.clone())];

    super::worker::process_event(
        &indexer,
        &ps,
        &dir_index,
        &mounts,
        &img.to_string_lossy(),
        "remove",
    );

    assert!(
        dir_index.is_dir_dirty("pics/album"),
        "画像削除で親ディレクトリが dirty 化されるべき"
    );
}

#[test]
fn 動画ファイルの追加イベントで親のdirty化とFTS追加が両方行われる() {
    let (_dir, root, ps, indexer, dir_index, _db1, _db2) = worker_fixture();
    std::fs::create_dir(root.join("movies")).unwrap();
    let video = root.join("movies/clip.mp4");
    std::fs::write(&video, b"x").unwrap();
    let mounts = vec![("vids".to_string(), root.clone())];

    super::worker::process_event(
        &indexer,
        &ps,
        &dir_index,
        &mounts,
        &video.to_string_lossy(),
        "add",
    );

    assert!(dir_index.is_dir_dirty("vids/movies"));
    assert_eq!(
        indexer.entry_count().unwrap(),
        1,
        "動画は FTS インデックスに追加されるべき"
    );
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

// --- pending 溢れの整合回復 ---

#[test]
fn pending溢れの整合回復で全ディレクトリがdirty化されpendingがdrainされる() {
    let (_dir, root, _ps, _indexer, dir_index, _db1, _db2) = worker_fixture();
    let _ = root;
    // DirIndex に 1 ディレクトリ分のデータを投入
    dir_index
        .ingest_walk_entry(&crate::services::indexer::WalkCallbackArgs {
            walk_entry_path: "/data/album".to_string(),
            root_dir: "/data".to_string(),
            mount_id: "pics".to_string(),
            dir_mtime_ns: 1,
            subdirs: vec![],
            files: vec![("a.jpg".to_string(), 100, 1)],
            is_complete: true,
        })
        .unwrap();

    let pending = empty_pending();
    pending
        .lock()
        .unwrap()
        .insert("/data/album/a.jpg".to_string(), "add".to_string());

    super::worker::recover_from_pending_overflow(&dir_index, &pending);

    assert!(
        dir_index.is_dir_dirty("pics/album"),
        "整合回復で既知ディレクトリが dirty 化されるべき"
    );
    assert!(
        pending.lock().unwrap().is_empty(),
        "pending は drain されるべき"
    );
}

// --- start / stop と is_running の整合 ---

fn build_watcher(mounts: Vec<(String, PathBuf)>, root: &Path) -> super::FileWatcher {
    use std::sync::Arc;

    use crate::services::dir_index::DirIndex;
    use crate::services::indexer::Indexer;
    use crate::services::path_security::PathSecurity;
    use crate::services::rebuild_guard::RebuildGuard;

    let ps = Arc::new(PathSecurity::new(vec![root.to_path_buf()], false).unwrap());
    super::FileWatcher::new(
        Arc::new(Indexer::new(":memory:")),
        ps,
        Arc::new(DirIndex::new(":memory:")),
        mounts,
        Arc::new(RebuildGuard::new()),
    )
}

#[tokio::test]
async fn startで監視が開始されstopで停止する() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let fw = build_watcher(vec![("m".to_string(), root.clone())], &root);

    assert!(!fw.is_running(), "起動前は is_running が false");
    fw.start().unwrap();
    assert!(fw.is_running(), "起動後は is_running が true");
    fw.stop();
    assert!(!fw.is_running(), "停止後は is_running が false");
}

#[tokio::test]
async fn watch失敗時はis_runningがfalseのまま() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::TempDir::new().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let denied = root.join("denied");
    std::fs::create_dir(&denied).unwrap();
    std::fs::set_permissions(&denied, std::fs::Permissions::from_mode(0o000)).unwrap();
    // root 実行等で権限制限が効かない環境ではスキップ
    if std::fs::read_dir(&denied).is_ok() {
        std::fs::set_permissions(&denied, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }

    let fw = build_watcher(vec![("m".to_string(), denied.clone())], &root);
    let result = fw.start();

    // TempDir 削除が失敗しないよう後始末
    std::fs::set_permissions(&denied, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert!(
        result.is_err(),
        "読み取り権限のないディレクトリでは start が失敗するはず"
    );
    assert!(
        !fw.is_running(),
        "start 失敗時に is_running が立っていてはならない (稼働中と誤認され誰も気付けない)"
    );
}
