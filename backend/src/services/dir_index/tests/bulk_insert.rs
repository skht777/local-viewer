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

// --- warm-start シナリオ統合テスト ---
// 「サーバ停止中にファイル削除 → 再起動」を模倣した end-to-end シナリオ。
// BulkInserter 経由の通常 production path で削除反映を検証する。

#[test]
fn warm_start_削除ファイル反映_a_jpgを削除して再ingestするとb_jpgのみ残る() {
    // 元バグの主目的シナリオ:
    // 1. cold scan で a.jpg b.jpg 登録
    // 2. ユーザが a.jpg を削除（停止中）
    // 3. 再起動 → incremental_scan が親 dir mtime 変化を検出 → walker が再走査
    //    → callback で BulkInserter に「現在の子集合 = b.jpg のみ」を渡す
    // 4. canonicalize で a.jpg 行は削除される
    let (idx, _tmp) = setup();

    // Step 1: cold scan
    {
        let mut bulk = idx.begin_bulk().unwrap();
        bulk.ingest_walk_entry(&make_args(
            "/data/photos",
            "/data",
            "m",
            vec![],
            vec![("a.jpg", 100, 1_000_000), ("b.jpg", 200, 2_000_000)],
        ))
        .unwrap();
        bulk.flush().unwrap();
    }
    assert_eq!(idx.child_count("m/photos").unwrap(), 2);

    // Step 2: 「a.jpg 削除後の incremental_scan」を模倣 → b.jpg のみ ingest
    {
        let mut bulk = idx.begin_bulk().unwrap();
        bulk.ingest_walk_entry(&make_args(
            "/data/photos",
            "/data",
            "m",
            vec![],
            vec![("b.jpg", 200, 2_000_000)],
        ))
        .unwrap();
        bulk.flush().unwrap();
    }

    // Step 3: 検証 - b.jpg のみが残ること (a.jpg は cascade で削除)
    let entries = idx
        .query_page("m/photos", "name-asc", Some(100), None)
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "b.jpg");
}

#[test]
fn warm_start_サブツリー削除反映_subディレクトリ消失で配下が完全消去される() {
    // 「停止中に rm -rf sub/」シナリオ:
    // 1. cold scan で photos/sub/x.jpg, photos/sub/y.jpg, photos/file.jpg を登録
    // 2. sub/ が削除される
    // 3. 再起動 → walker が photos のみ visit (sub は存在しない)
    //    callback で BulkInserter に「photos の現在の子 = file.jpg のみ」を渡す
    // 4. canonicalize で sub/ 自身と sub/ 配下全行が cascade 削除
    let (idx, _tmp) = setup();

    // Step 1: 初期状態を構築 (3 階層)
    {
        let mut bulk = idx.begin_bulk().unwrap();
        bulk.ingest_walk_entry(&make_args(
            "/data/photos",
            "/data",
            "m",
            vec![("sub", 1_000_000)],
            vec![("file.jpg", 100, 5_000_000)],
        ))
        .unwrap();
        bulk.ingest_walk_entry(&make_args(
            "/data/photos/sub",
            "/data",
            "m",
            vec![],
            vec![("x.jpg", 200, 6_000_000), ("y.jpg", 300, 7_000_000)],
        ))
        .unwrap();
        bulk.flush().unwrap();
    }
    assert_eq!(idx.child_count("m/photos").unwrap(), 2); // sub + file.jpg
    assert_eq!(idx.child_count("m/photos/sub").unwrap(), 2); // x.jpg + y.jpg

    // Step 2: 再起動模倣 - photos のみ visit (sub は walker に拾われない)
    {
        let mut bulk = idx.begin_bulk().unwrap();
        bulk.ingest_walk_entry(&make_args(
            "/data/photos",
            "/data",
            "m",
            vec![],
            vec![("file.jpg", 100, 5_000_000)],
        ))
        .unwrap();
        bulk.flush().unwrap();
    }

    // Step 3: 検証 - sub 配下が完全消去
    assert_eq!(idx.child_count("m/photos").unwrap(), 1);
    assert_eq!(idx.child_count("m/photos/sub").unwrap(), 0);
    assert!(idx.get_dir_mtime("m/photos/sub").unwrap().is_none());
}

#[test]
fn warm_start_未変更リーフが誤削除されない() {
    // codex 1回目 Critical 2 回帰防止:
    // 旧設計の mount-wide GC は warm incremental の枝刈り parent を seen から
    // 取りこぼし、未変更リーフ配下を誤削除した。per-parent cascade は visit
    // された parent のみ正本化、未 visit parent は触らない。
    let (idx, _tmp) = setup();

    // 初期状態: photos/cold/ (未変更リーフ) + photos/hot/
    {
        let mut bulk = idx.begin_bulk().unwrap();
        bulk.ingest_walk_entry(&make_args(
            "/data/photos",
            "/data",
            "m",
            vec![("cold", 1_000_000), ("hot", 2_000_000)],
            vec![],
        ))
        .unwrap();
        bulk.ingest_walk_entry(&make_args(
            "/data/photos/cold",
            "/data",
            "m",
            vec![],
            vec![("preserved.jpg", 100, 1_000_000)],
        ))
        .unwrap();
        bulk.ingest_walk_entry(&make_args(
            "/data/photos/hot",
            "/data",
            "m",
            vec![],
            vec![("old.jpg", 200, 2_000_000)],
        ))
        .unwrap();
        bulk.flush().unwrap();
    }

    // hot のみ変更があった → walker は photos と hot のみ visit (cold は枝刈り)
    {
        let mut bulk = idx.begin_bulk().unwrap();
        bulk.ingest_walk_entry(&make_args(
            "/data/photos",
            "/data",
            "m",
            vec![("cold", 1_000_000), ("hot", 9_000_000)], // hot の mtime 更新
            vec![],
        ))
        .unwrap();
        bulk.ingest_walk_entry(&make_args(
            "/data/photos/hot",
            "/data",
            "m",
            vec![],
            vec![("new.jpg", 300, 9_000_000)],
        ))
        .unwrap();
        // cold は visit されない (枝刈り) - BulkInserter には渡されない
        bulk.flush().unwrap();
    }

    // hot の中身は更新される
    let hot_entries = idx
        .query_page("m/photos/hot", "name-asc", Some(100), None)
        .unwrap();
    assert_eq!(hot_entries.len(), 1);
    assert_eq!(hot_entries[0].name, "new.jpg");

    // cold は visit されていないため preserved.jpg が完全保持
    let cold_entries = idx
        .query_page("m/photos/cold", "name-asc", Some(100), None)
        .unwrap();
    assert_eq!(cold_entries.len(), 1, "未変更リーフ配下が誤削除された");
    assert_eq!(cold_entries[0].name, "preserved.jpg");
}
