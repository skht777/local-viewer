#![allow(
    non_snake_case,
    reason = "日本語テスト名で PascalCase 残存を許容（規約 07_testing.md）"
)]

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

#[test]
fn ingest_walk_entryはis_complete_falseをskipして既存行を保持する() {
    let (idx, _tmp) = setup();
    // 初期状態: dir1 配下に a.jpg, b.png
    {
        let mut bulk = idx.begin_bulk().unwrap();
        bulk.ingest_walk_entry(&make_args(
            "/data/dir1",
            "/data",
            "m",
            vec![],
            vec![("a.jpg", 100, 1_000_000), ("b.png", 200, 2_000_000)],
        ))
        .unwrap();
        bulk.flush().unwrap();
    }
    assert_eq!(idx.child_count("m/dir1").unwrap(), 2);

    // is_complete=false の args を投入 → DirIndex は無変更
    {
        let mut bulk = idx.begin_bulk().unwrap();
        let mut args = make_args(
            "/data/dir1",
            "/data",
            "m",
            vec![],
            vec![("a.jpg", 100, 1_000_000)], // 部分結果 (b.png 抜け)
        );
        args.is_complete = false;
        bulk.ingest_walk_entry(&args).unwrap();
        bulk.flush().unwrap();
    }
    // a.jpg + b.png 両方が保持される (cascade スキップ)
    assert_eq!(idx.child_count("m/dir1").unwrap(), 2);
}

#[test]
fn flushは累積保持せず2回目flushで前回parentをDELETEしない() {
    // codex 1回目 Critical 1 の回帰テスト:
    // 旧設計の visited_parents 累積を引きずると、2 回目 flush で前回 flush 済の
    // parent を再 DELETE して、別の visit で投入された行が消える可能性があった。
    let (idx, _tmp) = setup();
    let mut bulk = idx.begin_bulk().unwrap();

    // 1 回目: dir1 に a.jpg
    bulk.ingest_walk_entry(&make_args(
        "/data/dir1",
        "/data",
        "m",
        vec![],
        vec![("a.jpg", 100, 1_000_000)],
    ))
    .unwrap();
    bulk.flush().unwrap();
    assert_eq!(idx.child_count("m/dir1").unwrap(), 1);

    // 2 回目: dir2 のみ ingest (dir1 は触らない) → flush
    bulk.ingest_walk_entry(&make_args(
        "/data/dir2",
        "/data",
        "m",
        vec![],
        vec![("c.jpg", 300, 3_000_000)],
    ))
    .unwrap();
    bulk.flush().unwrap();

    // dir1 の a.jpg は累積 DELETE で消えていないこと
    assert_eq!(
        idx.child_count("m/dir1").unwrap(),
        1,
        "前回 flush 済の dir1 が 2 回目 flush で誤削除されている"
    );
    assert_eq!(idx.child_count("m/dir2").unwrap(), 1);
}

#[test]
fn ingest_walk_entryは同_parent_への重複ingestを最新で上書きする() {
    let (idx, _tmp) = setup();
    let mut bulk = idx.begin_bulk().unwrap();

    // 同 parent に 2 回 ingest (full snapshot API 仕様)
    bulk.ingest_walk_entry(&make_args(
        "/data/dir1",
        "/data",
        "m",
        vec![],
        vec![("old.jpg", 100, 1_000_000)],
    ))
    .unwrap();
    bulk.ingest_walk_entry(&make_args(
        "/data/dir1",
        "/data",
        "m",
        vec![],
        vec![("new.jpg", 200, 2_000_000)],
    ))
    .unwrap();
    bulk.flush().unwrap();

    // 最後の ingest が真として canonicalize される
    let entries = idx
        .query_page("m/dir1", "name-asc", Some(100), None)
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "new.jpg");
}
