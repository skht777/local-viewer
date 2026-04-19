use super::common::{make_args, setup};

// 16 桁 lowercase hex の mount_id 定数（mount_scope_range invariant を満たす）
const MOUNT_A: &str = "aaaaaaaaaaaaaaaa";
const MOUNT_B: &str = "bbbbbbbbbbbbbbbb";

#[test]
fn ingest_walk_entryでエントリが保存される() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data/photos",
        "/data",
        "mount1",
        vec![("subdir", 2_000_000_000)],
        vec![
            ("image1.jpg", 1024, 3_000_000_000),
            ("archive.zip", 2048, 4_000_000_000),
        ],
    );
    idx.ingest_walk_entry(&args).unwrap();

    // 3 エントリ (subdir + image1.jpg + archive.zip) が保存される
    let entries = idx
        .query_page("mount1/photos", "name-asc", Some(100), None)
        .unwrap();
    assert_eq!(entries.len(), 3);

    // ディレクトリが先頭
    assert_eq!(entries[0].name, "subdir");
    assert_eq!(entries[0].kind, "directory");

    // ファイルが後に続く (自然順)
    assert_eq!(entries[1].name, "archive.zip");
    assert_eq!(entries[1].kind, "archive");

    assert_eq!(entries[2].name, "image1.jpg");
    assert_eq!(entries[2].kind, "image");
}

#[test]
fn dir_mtimeの保存と取得() {
    let (idx, _tmp) = setup();

    // 未登録の場合 None
    assert_eq!(idx.get_dir_mtime("some/path").unwrap(), None);

    // 保存後は値が返る
    idx.set_dir_mtime("some/path", 12345).unwrap();
    assert_eq!(idx.get_dir_mtime("some/path").unwrap(), Some(12345));

    // 上書き
    idx.set_dir_mtime("some/path", 99999).unwrap();
    assert_eq!(idx.get_dir_mtime("some/path").unwrap(), Some(99999));
}

#[test]
fn ルートディレクトリのparent_pathがmount_idになる() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data",
        "/data",
        "myMount",
        vec![("sub", 1_000_000)],
        vec![("file.jpg", 100, 2_000_000)],
    );
    idx.ingest_walk_entry(&args).unwrap();

    // ルート直下は mount_id がそのまま parent_path
    let entries = idx
        .query_page("myMount", "name-asc", Some(100), None)
        .unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].parent_path, "myMount");
}

// --- delete_mount_entries の回帰テスト ---

/// `mount_a` + `mount_b` を登録して `DirIndex` に 2 マウントを準備する
fn setup_two_mounts(idx: &crate::services::dir_index::DirIndex) {
    // mount_a のルート
    idx.ingest_walk_entry(&make_args(
        "/data",
        "/data",
        MOUNT_A,
        vec![("sub_a", 1_000_000)],
        vec![("top_a.jpg", 100, 2_000_000)],
    ))
    .unwrap();
    // mount_a の子ディレクトリ
    idx.ingest_walk_entry(&make_args(
        "/data/sub_a",
        "/data",
        MOUNT_A,
        vec![],
        vec![("a.jpg", 10, 3_000_000)],
    ))
    .unwrap();
    // mount_b のルート
    idx.ingest_walk_entry(&make_args(
        "/data",
        "/data",
        MOUNT_B,
        vec![("sub_b", 4_000_000)],
        vec![("top_b.jpg", 200, 5_000_000)],
    ))
    .unwrap();
}

#[test]
fn delete_mount_entriesは自マウント配下のみ削除する() {
    let (idx, _tmp) = setup();
    setup_two_mounts(&idx);

    let removed = idx.delete_mount_entries(MOUNT_A).unwrap();
    assert!(removed >= 2, "mount_a の少なくとも 2 行が削除される");

    // mount_a は空
    let mount_a_root = idx
        .query_page(MOUNT_A, "name-asc", Some(100), None)
        .unwrap();
    assert!(mount_a_root.is_empty());
    let mount_a_sub = idx
        .query_page(&format!("{MOUNT_A}/sub_a"), "name-asc", Some(100), None)
        .unwrap();
    assert!(mount_a_sub.is_empty());

    // mount_b は無傷
    let b_root = idx
        .query_page(MOUNT_B, "name-asc", Some(100), None)
        .unwrap();
    assert_eq!(b_root.len(), 2);
}

#[test]
fn delete_mount_entriesはdir_metaも削除する() {
    let (idx, _tmp) = setup();
    setup_two_mounts(&idx);

    // 削除前: mount_a ルートの mtime が記録されている
    assert!(idx.get_dir_mtime(MOUNT_A).unwrap().is_some());

    idx.delete_mount_entries(MOUNT_A).unwrap();

    // 削除後: mount_a の mtime は消え、mount_b は残る
    assert!(idx.get_dir_mtime(MOUNT_A).unwrap().is_none());
    assert!(
        idx.get_dir_mtime(&format!("{MOUNT_A}/sub_a"))
            .unwrap()
            .is_none()
    );
    assert!(idx.get_dir_mtime(MOUNT_B).unwrap().is_some());
}

#[test]
fn delete_mount_entriesは無効mount_idでerrを返す() {
    let (idx, _tmp) = setup();
    // 空
    assert!(idx.delete_mount_entries("").is_err());
    // 15 桁
    assert!(idx.delete_mount_entries("aaaaaaaaaaaaaaa").is_err());
    // 非 hex (uppercase)
    assert!(idx.delete_mount_entries("AAAAAAAAAAAAAAAA").is_err());
    // 非 hex (記号)
    assert!(idx.delete_mount_entries("aaaa/../aaaaaaaa").is_err());
}

#[test]
fn delete_mount_entriesは該当なしで0を返し冪等() {
    let (idx, _tmp) = setup();
    // 登録なしで delete → 0 件、副作用なし
    let removed = idx.delete_mount_entries(MOUNT_A).unwrap();
    assert_eq!(removed, 0);

    // 2 度目も 0 件（冪等）
    let removed = idx.delete_mount_entries(MOUNT_A).unwrap();
    assert_eq!(removed, 0);
}

#[test]
fn delete_mount_entriesは子孫ディレクトリの全dir_metaを削除する() {
    let (idx, _tmp) = setup();
    // mount_a/sub_a/deeper のネスト階層まで dir_meta を積む
    idx.ingest_walk_entry(&make_args(
        "/data",
        "/data",
        MOUNT_A,
        vec![("sub_a", 1_000_000)],
        vec![],
    ))
    .unwrap();
    idx.ingest_walk_entry(&make_args(
        "/data/sub_a",
        "/data",
        MOUNT_A,
        vec![("deeper", 2_000_000)],
        vec![],
    ))
    .unwrap();
    idx.ingest_walk_entry(&make_args(
        "/data/sub_a/deeper",
        "/data",
        MOUNT_A,
        vec![],
        vec![("x.jpg", 10, 3_000_000)],
    ))
    .unwrap();

    // 3 階層すべての dir_meta が登録されている
    assert!(idx.get_dir_mtime(MOUNT_A).unwrap().is_some());
    assert!(
        idx.get_dir_mtime(&format!("{MOUNT_A}/sub_a"))
            .unwrap()
            .is_some()
    );
    assert!(
        idx.get_dir_mtime(&format!("{MOUNT_A}/sub_a/deeper"))
            .unwrap()
            .is_some()
    );

    idx.delete_mount_entries(MOUNT_A).unwrap();

    // 全階層の dir_meta が消去される（range scan が子孫まで届く）
    assert!(idx.get_dir_mtime(MOUNT_A).unwrap().is_none());
    assert!(
        idx.get_dir_mtime(&format!("{MOUNT_A}/sub_a"))
            .unwrap()
            .is_none()
    );
    assert!(
        idx.get_dir_mtime(&format!("{MOUNT_A}/sub_a/deeper"))
            .unwrap()
            .is_none()
    );
}
