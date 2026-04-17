use super::common::{make_args, setup};

// --- query_page sort/cursor ---

#[test]
fn query_pageでlimit_noneは全件取得する() {
    let (idx, _tmp) = setup();

    // 10 ファイルを投入
    let files: Vec<(&str, i64, i64)> = (0..10)
        .map(|i| {
            // 'static な文字列が必要なので Box::leak で吸収する
            let name: &'static str = Box::leak(format!("file{i:02}.jpg").into_boxed_str());
            (name, 100_i64, 1_000_000_i64 + i)
        })
        .collect();
    let args = make_args("/data", "/data", "m", vec![], files);
    idx.ingest_walk_entry(&args).unwrap();

    // limit = None で全件返る。has_next 判定用の +1 も発生しない。
    let entries = idx.query_page("m", "name-asc", None, None).unwrap();
    assert_eq!(entries.len(), 10);
    assert_eq!(entries[0].name, "file00.jpg");
    assert_eq!(entries[9].name, "file09.jpg");
}

#[test]
fn query_pageでname_ascソートが自然順で返る() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data",
        "/data",
        "m",
        vec![],
        vec![
            ("file10.jpg", 100, 1_000_000),
            ("file1.jpg", 100, 2_000_000),
            ("file2.jpg", 100, 3_000_000),
        ],
    );
    idx.ingest_walk_entry(&args).unwrap();

    let entries = idx.query_page("m", "name-asc", Some(100), None).unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["file1.jpg", "file2.jpg", "file10.jpg"]);
}

#[test]
fn query_pageでカーソルページネーションが動作する() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data",
        "/data",
        "m",
        vec![],
        vec![
            ("a.jpg", 100, 1_000_000),
            ("b.jpg", 100, 2_000_000),
            ("c.jpg", 100, 3_000_000),
        ],
    );
    idx.ingest_walk_entry(&args).unwrap();

    // 1 件目を取得
    let page1 = idx.query_page("m", "name-asc", Some(1), None).unwrap();
    assert_eq!(page1.len(), 1);
    assert_eq!(page1[0].name, "a.jpg");

    // カーソルを使って 2 件目を取得
    // kind_flag=1 (non-directory) + sort_key
    let cursor = format!("1\x00{}", page1[0].sort_key);
    let page2 = idx
        .query_page("m", "name-asc", Some(1), Some(&cursor))
        .unwrap();
    assert_eq!(page2.len(), 1);
    assert_eq!(page2[0].name, "b.jpg");

    // 3 件目
    let cursor2 = format!("1\x00{}", page2[0].sort_key);
    let page3 = idx
        .query_page("m", "name-asc", Some(1), Some(&cursor2))
        .unwrap();
    assert_eq!(page3.len(), 1);
    assert_eq!(page3[0].name, "c.jpg");

    // 4 件目は空
    let cursor3 = format!("1\x00{}", page3[0].sort_key);
    let page4 = idx
        .query_page("m", "name-asc", Some(1), Some(&cursor3))
        .unwrap();
    assert!(page4.is_empty());
}

#[test]
fn date_descソートとカーソル() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data",
        "/data",
        "m",
        vec![],
        vec![
            ("old.jpg", 100, 1_000_000),
            ("mid.jpg", 100, 2_000_000),
            ("new.jpg", 100, 3_000_000),
        ],
    );
    idx.ingest_walk_entry(&args).unwrap();

    // 新しい順
    let page1 = idx.query_page("m", "date-desc", Some(2), None).unwrap();
    assert_eq!(page1.len(), 2);
    assert_eq!(page1[0].name, "new.jpg");
    assert_eq!(page1[1].name, "mid.jpg");

    // カーソルで次ページ
    let cursor = page1[1].mtime_ns.to_string();
    let page2 = idx
        .query_page("m", "date-desc", Some(2), Some(&cursor))
        .unwrap();
    assert_eq!(page2.len(), 1);
    assert_eq!(page2[0].name, "old.jpg");
}

#[test]
fn query_pageのdate_descで同一mtimeはsort_key昇順() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data",
        "/data",
        "m",
        vec![],
        vec![
            ("beta.jpg", 100, 1_000_000),
            ("alpha.jpg", 200, 1_000_000), // 同じ mtime_ns
        ],
    );
    idx.ingest_walk_entry(&args).unwrap();

    let page = idx.query_page("m", "date-desc", Some(10), None).unwrap();
    assert_eq!(page[0].name, "alpha.jpg"); // sort_key 昇順: alpha < beta
    assert_eq!(page[1].name, "beta.jpg");
}

#[test]
fn query_pageのdate_descカーソルで同一mtimeのタプル比較() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data",
        "/data",
        "m",
        vec![],
        vec![
            ("a.jpg", 100, 2_000_000), // 新しい
            ("c.jpg", 100, 1_000_000), // 古い (同一 mtime)
            ("b.jpg", 100, 1_000_000), // 古い (同一 mtime)
        ],
    );
    idx.ingest_walk_entry(&args).unwrap();

    // 1ページ目: a.jpg + b.jpg (sort_key 昇順タイブレーカー)
    let page1 = idx.query_page("m", "date-desc", Some(2), None).unwrap();
    assert_eq!(page1[0].name, "a.jpg");
    assert_eq!(page1[1].name, "b.jpg");

    // カーソルで次ページ: c.jpg が残る
    let cursor = format!("{}\x00{}", page1[1].mtime_ns, page1[1].sort_key);
    let page2 = idx
        .query_page("m", "date-desc", Some(2), Some(&cursor))
        .unwrap();
    assert_eq!(page2.len(), 1);
    assert_eq!(page2[0].name, "c.jpg");
}

// --- query_sibling ---

#[test]
fn 次の兄弟をkindフィルタ付きで取得できる() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data",
        "/data",
        "m1",
        vec![("a_dir", 1_000_000), ("c_dir", 3_000_000)],
        vec![("b_file.jpg", 100, 2_000_000_000)],
    );
    idx.ingest_walk_entry(&args).unwrap();

    // a_dir の次の directory は c_dir (b_file.jpg はスキップ)
    let next = idx
        .query_sibling("m1", "a_dir", true, "next", "name-asc", &["directory"])
        .unwrap();
    assert!(next.is_some());
    assert_eq!(next.unwrap().name, "c_dir");
}

#[test]
fn 前の兄弟をkindフィルタ付きで取得できる() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data",
        "/data",
        "m1",
        vec![("a_dir", 1_000_000), ("c_dir", 3_000_000)],
        vec![("b_file.jpg", 100, 2_000_000_000)],
    );
    idx.ingest_walk_entry(&args).unwrap();

    let prev = idx
        .query_sibling("m1", "c_dir", true, "prev", "name-asc", &["directory"])
        .unwrap();
    assert!(prev.is_some());
    assert_eq!(prev.unwrap().name, "a_dir");
}

#[test]
fn 該当なしでnoneを返す() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data",
        "/data",
        "m1",
        vec![("only_dir", 1_000_000)],
        vec![],
    );
    idx.ingest_walk_entry(&args).unwrap();

    let next = idx
        .query_sibling("m1", "only_dir", true, "next", "name-asc", &["directory"])
        .unwrap();
    assert!(next.is_none());
}

#[test]
fn query_siblingのname_ascでディレクトリからファイルへ次を取得() {
    let (idx, _tmp) = setup();

    // ディレクトリ (z_dir) + アーカイブ (a_archive.zip)
    // browse 順: z_dir (dir優先), a_archive.zip
    // sort_key のみの比較だと a < z なので z_dir → 次なし になるバグ
    let args = make_args(
        "/data",
        "/data",
        "m1",
        vec![("z_dir", 1_000_000)],
        vec![("a_archive.zip", 200, 2_000_000_000)],
    );
    idx.ingest_walk_entry(&args).unwrap();

    let kinds = &["directory", "archive", "pdf"];
    let next = idx
        .query_sibling("m1", "z_dir", true, "next", "name-asc", kinds)
        .unwrap();
    assert!(next.is_some(), "ディレクトリの次にアーカイブが来るはず");
    assert_eq!(next.unwrap().name, "a_archive.zip");
}

#[test]
fn query_siblingのname_descでsort_key降順の次を取得() {
    let (idx, _tmp) = setup();

    // name-desc 順: dir (dir優先), c_archive.zip, a_archive.zip
    let args = make_args(
        "/data",
        "/data",
        "m1",
        vec![("dir", 1_000_000)],
        vec![
            ("a_archive.zip", 100, 1_000_000_000),
            ("c_archive.zip", 100, 2_000_000_000),
        ],
    );
    idx.ingest_walk_entry(&args).unwrap();

    let kinds = &["directory", "archive", "pdf"];
    // dir の次は c_archive.zip (name-desc: ファイルは名前降順)
    let next = idx
        .query_sibling("m1", "dir", true, "next", "name-desc", kinds)
        .unwrap();
    assert!(next.is_some());
    assert_eq!(next.unwrap().name, "c_archive.zip");
}

#[test]
fn query_siblingのdate_descで同一mtimeのタイブレーカーが動作する() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data",
        "/data",
        "m1",
        vec![
            ("a_dir", 1_000_000),
            ("b_dir", 1_000_000), // 同一 mtime
        ],
        vec![],
    );
    idx.ingest_walk_entry(&args).unwrap();

    let kinds = &["directory", "archive", "pdf"];
    let next = idx
        .query_sibling("m1", "a_dir", true, "next", "date-desc", kinds)
        .unwrap();
    assert!(next.is_some());
    assert_eq!(next.unwrap().name, "b_dir");
}

#[test]
fn query_siblingでname逆引きにより大文字小文字衝突を回避() {
    let (idx, _tmp) = setup();

    // FILE2 と file2 は encode_sort_key で同じ sort_key になる
    // name 逆引きなら区別可能
    let args = make_args(
        "/data",
        "/data",
        "m1",
        vec![("FILE2", 2_000_000), ("file2", 1_000_000)],
        vec![],
    );
    idx.ingest_walk_entry(&args).unwrap();

    let kinds = &["directory", "archive", "pdf"];
    // date-desc 順: FILE2 (mtime 2M), file2 (mtime 1M)
    let next = idx
        .query_sibling("m1", "FILE2", true, "next", "date-desc", kinds)
        .unwrap();
    assert!(next.is_some());
    assert_eq!(next.unwrap().name, "file2");
}
