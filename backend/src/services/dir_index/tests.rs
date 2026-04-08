use super::*;

/// テスト用の一時 DB パスで `DirIndex` を生成する
fn setup() -> (DirIndex, tempfile::NamedTempFile) {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let idx = DirIndex::new(tmp.path().to_str().unwrap());
    idx.init_db().unwrap();
    (idx, tmp)
}

/// テスト用の `WalkCallbackArgs` を生成する
fn make_args(
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

#[test]
fn init_dbでスキーマが作成される() {
    let (idx, _tmp) = setup();
    let count = idx.entry_count().unwrap();
    assert_eq!(count, 0);
}

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
        .query_page("mount1/photos", "name-asc", 100, None)
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

    let entries = idx.query_page("m", "name-asc", 100, None).unwrap();
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
    let page1 = idx.query_page("m", "name-asc", 1, None).unwrap();
    assert_eq!(page1.len(), 1);
    assert_eq!(page1[0].name, "a.jpg");

    // カーソルを使って 2 件目を取得
    // kind_flag=1 (non-directory) + sort_key
    let cursor = format!("1\x00{}", page1[0].sort_key);
    let page2 = idx.query_page("m", "name-asc", 1, Some(&cursor)).unwrap();
    assert_eq!(page2.len(), 1);
    assert_eq!(page2[0].name, "b.jpg");

    // 3 件目
    let cursor2 = format!("1\x00{}", page2[0].sort_key);
    let page3 = idx.query_page("m", "name-asc", 1, Some(&cursor2)).unwrap();
    assert_eq!(page3.len(), 1);
    assert_eq!(page3[0].name, "c.jpg");

    // 4 件目は空
    let cursor3 = format!("1\x00{}", page3[0].sort_key);
    let page4 = idx.query_page("m", "name-asc", 1, Some(&cursor3)).unwrap();
    assert!(page4.is_empty());
}

#[test]
fn child_countが正しく返る() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data",
        "/data",
        "m",
        vec![("sub1", 1_000_000)],
        vec![("a.jpg", 100, 2_000_000), ("b.png", 200, 3_000_000)],
    );
    idx.ingest_walk_entry(&args).unwrap();

    assert_eq!(idx.child_count("m").unwrap(), 3);
    assert_eq!(idx.child_count("nonexistent").unwrap(), 0);
}

#[test]
fn preview_entriesが画像とアーカイブを返す() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data",
        "/data",
        "m",
        vec![("subdir", 1_000_000)],
        vec![
            ("photo.jpg", 100, 2_000_000),
            ("readme.txt", 50, 3_000_000),
            ("comic.zip", 500, 4_000_000),
            ("movie.mp4", 1000, 5_000_000),
        ],
    );
    idx.ingest_walk_entry(&args).unwrap();

    let previews = idx.preview_entries("m", 10).unwrap();
    let kinds: Vec<&str> = previews.iter().map(|e| e.kind.as_str()).collect();
    // directory と other (txt) は含まれない
    assert!(!kinds.contains(&"directory"));
    assert!(!kinds.contains(&"other"));
    assert_eq!(previews.len(), 3); // photo.jpg, comic.zip, movie.mp4
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
fn is_full_scan_doneのフラグ管理() {
    let (idx, _tmp) = setup();

    assert!(!idx.is_full_scan_done().unwrap());
    idx.mark_full_scan_done().unwrap();
    assert!(idx.is_full_scan_done().unwrap());
}

#[test]
fn mark_readyとmark_warm_startの状態遷移() {
    let idx = DirIndex::new(":memory:");

    // 初期状態
    assert!(!idx.is_ready());
    assert!(!idx.is_stale());

    // ウォームスタート
    idx.mark_warm_start();
    assert!(idx.is_ready());
    assert!(idx.is_stale());

    // 準備完了
    idx.mark_ready();
    assert!(idx.is_ready());
    assert!(!idx.is_stale());
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
    let page1 = idx.query_page("m", "date-desc", 2, None).unwrap();
    assert_eq!(page1.len(), 2);
    assert_eq!(page1[0].name, "new.jpg");
    assert_eq!(page1[1].name, "mid.jpg");

    // カーソルで次ページ
    let cursor = page1[1].mtime_ns.to_string();
    let page2 = idx.query_page("m", "date-desc", 2, Some(&cursor)).unwrap();
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

    let page = idx.query_page("m", "date-desc", 10, None).unwrap();
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
    let page1 = idx.query_page("m", "date-desc", 2, None).unwrap();
    assert_eq!(page1[0].name, "a.jpg");
    assert_eq!(page1[1].name, "b.jpg");

    // カーソルで次ページ: c.jpg が残る
    let cursor = format!("{}\x00{}", page1[1].mtime_ns, page1[1].sort_key);
    let page2 = idx.query_page("m", "date-desc", 2, Some(&cursor)).unwrap();
    assert_eq!(page2.len(), 1);
    assert_eq!(page2[0].name, "c.jpg");
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
    let entries = idx.query_page("myMount", "name-asc", 100, None).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].parent_path, "myMount");
}

// --- first_entry_by_kind ---

#[test]
fn first_entry_by_kindがarchiveを優先して返す() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data/photos",
        "/data",
        "m1",
        vec![],
        vec![
            ("image1.jpg", 100, 1_000_000_000),
            ("archive.zip", 200, 2_000_000_000),
            ("doc.pdf", 300, 3_000_000_000),
        ],
    );
    idx.ingest_walk_entry(&args).unwrap();

    // archive が最初に見つかる
    let entry = idx.first_entry_by_kind("m1/photos", "archive").unwrap();
    assert!(entry.is_some());
    assert_eq!(entry.unwrap().name, "archive.zip");
}

#[test]
fn first_entry_by_kindで該当なしはnoneを返す() {
    let (idx, _tmp) = setup();

    let args = make_args(
        "/data/photos",
        "/data",
        "m1",
        vec![],
        vec![("image1.jpg", 100, 1_000_000_000)],
    );
    idx.ingest_walk_entry(&args).unwrap();

    let entry = idx.first_entry_by_kind("m1/photos", "archive").unwrap();
    assert!(entry.is_none());
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
    let reader_page = reader.query_page(parent, "name-asc", 100, None).unwrap();
    let direct_page = idx.query_page(parent, "name-asc", 100, None).unwrap();
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

/// `batch_dir_info` テスト用のデータをセットアップする
fn setup_batch_data(idx: &DirIndex) {
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
