use super::common::{make_args, setup, setup_batch_data};

#[test]
fn readerで取得したセッションが既存メソッドと同じ結果を返す() {
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

    let parent = "mount1/photos";

    // reader セッションで全クエリを同一接続で実行
    let reader = idx.reader().unwrap();

    // query_page
    let reader_page = reader
        .query_page(parent, "name-asc", Some(100), None)
        .unwrap();
    let direct_page = idx.query_page(parent, "name-asc", Some(100), None).unwrap();
    assert_eq!(reader_page.len(), direct_page.len());
    for (r, d) in reader_page.iter().zip(direct_page.iter()) {
        assert_eq!(r.name, d.name);
        assert_eq!(r.kind, d.kind);
    }

    // child_count
    assert_eq!(
        reader.child_count(parent).unwrap(),
        idx.child_count(parent).unwrap(),
    );

    // preview_entries
    let reader_previews = reader.preview_entries(parent, 3).unwrap();
    let direct_previews = idx.preview_entries(parent, 3).unwrap();
    assert_eq!(reader_previews.len(), direct_previews.len());
    for (r, d) in reader_previews.iter().zip(direct_previews.iter()) {
        assert_eq!(r.name, d.name);
    }

    // get_dir_mtime
    assert_eq!(
        reader.get_dir_mtime(parent).unwrap(),
        idx.get_dir_mtime(parent).unwrap(),
    );

    // entry_count
    assert_eq!(reader.entry_count().unwrap(), idx.entry_count().unwrap(),);
}

#[test]
fn batch_dir_infoが複数ディレクトリのchild_countを一括取得する() {
    let (idx, _tmp) = setup();
    setup_batch_data(&idx);

    let reader = idx.reader().unwrap();
    let keys = &["m/photos/cats", "m/photos/dogs"];
    let info = reader.batch_dir_info(keys, 3).unwrap();

    assert_eq!(info.len(), 2);
    assert_eq!(info["m/photos/cats"].count, 3); // a.jpg, b.png, c.gif
    assert_eq!(info["m/photos/dogs"].count, 2); // d.jpg, readme.txt

    // 個別クエリと一致することを検証
    assert_eq!(
        info["m/photos/cats"].count,
        reader.child_count("m/photos/cats").unwrap(),
    );
    assert_eq!(
        info["m/photos/dogs"].count,
        reader.child_count("m/photos/dogs").unwrap(),
    );
}

#[test]
fn batch_dir_infoが存在しないparent_keyを含む場合空エントリを返す() {
    let (idx, _tmp) = setup();
    setup_batch_data(&idx);

    let reader = idx.reader().unwrap();
    let keys = &["m/photos/cats", "m/nonexistent"];
    let info = reader.batch_dir_info(keys, 3).unwrap();

    // cats は存在、nonexistent は結果に含まれない
    assert!(info.contains_key("m/photos/cats"));
    assert!(!info.contains_key("m/nonexistent"));
}

#[test]
fn batch_dir_infoがpreview_limitに従いプレビューを制限する() {
    let (idx, _tmp) = setup();
    setup_batch_data(&idx);

    let reader = idx.reader().unwrap();
    let keys = &["m/photos/cats"];

    // limit=2: cats には画像3枚あるが2枚まで
    let info = reader.batch_dir_info(keys, 2).unwrap();
    assert_eq!(info["m/photos/cats"].previews.len(), 2);

    // limit=10: 全3枚返る
    let info_all = reader.batch_dir_info(keys, 10).unwrap();
    assert_eq!(info_all["m/photos/cats"].previews.len(), 3);

    // 個別クエリと一致
    let direct = reader.preview_entries("m/photos/cats", 2).unwrap();
    assert_eq!(info["m/photos/cats"].previews.len(), direct.len());
    for (batch, single) in info["m/photos/cats"].previews.iter().zip(direct.iter()) {
        assert_eq!(batch.name, single.name);
    }
}

#[test]
fn batch_dir_infoが空スライスで空hash_mapを返す() {
    let (idx, _tmp) = setup();
    let reader = idx.reader().unwrap();
    let info = reader.batch_dir_info(&[], 3).unwrap();
    assert!(info.is_empty());
}
