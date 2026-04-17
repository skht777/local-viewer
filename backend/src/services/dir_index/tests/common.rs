use crate::services::dir_index::DirIndex;
use crate::services::indexer::WalkCallbackArgs;

/// テスト用の一時 DB パスで `DirIndex` を生成する
pub(super) fn setup() -> (DirIndex, tempfile::NamedTempFile) {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let idx = DirIndex::new(tmp.path().to_str().unwrap());
    idx.init_db().unwrap();
    (idx, tmp)
}

/// テスト用の `WalkCallbackArgs` を生成する
pub(super) fn make_args(
    walk_path: &str,
    root_dir: &str,
    mount_id: &str,
    subdirs: Vec<(&str, i64)>,
    files: Vec<(&str, i64, i64)>,
) -> WalkCallbackArgs {
    WalkCallbackArgs {
        walk_entry_path: walk_path.to_owned(),
        root_dir: root_dir.to_owned(),
        mount_id: mount_id.to_owned(),
        dir_mtime_ns: 1_000_000_000,
        subdirs: subdirs
            .into_iter()
            .map(|(n, m)| (n.to_owned(), m))
            .collect(),
        files: files
            .into_iter()
            .map(|(n, s, m)| (n.to_owned(), s, m))
            .collect(),
    }
}

/// `batch_dir_info` テスト用のデータをセットアップする
pub(super) fn setup_batch_data(idx: &DirIndex) {
    // mount1/photos に 2 サブディレクトリ + 1 画像
    idx.ingest_walk_entry(&make_args(
        "/data/photos",
        "/data",
        "m",
        vec![("cats", 1_000_000), ("dogs", 2_000_000)],
        vec![("top.jpg", 100, 3_000_000)],
    ))
    .unwrap();
    // mount1/photos/cats に画像3枚
    idx.ingest_walk_entry(&make_args(
        "/data/photos/cats",
        "/data",
        "m",
        vec![],
        vec![
            ("a.jpg", 100, 1_000_000),
            ("b.png", 200, 2_000_000),
            ("c.gif", 300, 3_000_000),
        ],
    ))
    .unwrap();
    // mount1/photos/dogs に画像1枚 + テキスト1枚
    idx.ingest_walk_entry(&make_args(
        "/data/photos/dogs",
        "/data",
        "m",
        vec![],
        vec![("d.jpg", 400, 4_000_000), ("readme.txt", 50, 5_000_000)],
    ))
    .unwrap();
}
