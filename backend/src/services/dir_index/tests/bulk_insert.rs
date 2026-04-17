use super::common::{make_args, setup};

#[test]
fn bulk_inserterでバッチ保存される() {
    let (idx, _tmp) = setup();
    let mut bulk = idx.begin_bulk().unwrap();

    let args1 = make_args(
        "/data/dir1",
        "/data",
        "m",
        vec![],
        vec![("a.jpg", 100, 1_000_000), ("b.png", 200, 2_000_000)],
    );
    let args2 = make_args(
        "/data/dir2",
        "/data",
        "m",
        vec![],
        vec![("c.jpg", 300, 3_000_000)],
    );

    bulk.ingest_walk_entry(&args1).unwrap();
    bulk.ingest_walk_entry(&args2).unwrap();
    bulk.flush().unwrap();

    // DirIndex 経由で確認
    assert_eq!(idx.entry_count().unwrap(), 3);
    assert_eq!(idx.child_count("m/dir1").unwrap(), 2);
    assert_eq!(idx.child_count("m/dir2").unwrap(), 1);
}
