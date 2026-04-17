use super::common::{make_args, setup};

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
