#![allow(
    non_snake_case,
    reason = "日本語テスト名で PascalCase 残存を許容（規約 07_testing.md）"
)]

use super::common::{make_args, setup};
use crate::services::dir_index::DirIndexError;
use crate::services::dir_index::writes::{canonicalize_parent_in_tx, name_has_invalid_byte};

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

// --- canonicalize_parent_in_tx の per-parent cascade テスト ---

/// `canonicalize_parent_in_tx` を直接呼び出すヘルパ
///
/// 1 tx で実行し、戻り値の `CanonicalizeReport` を返す。テスト内で tx を開いて
/// `canonicalize_parent_in_tx` を呼び commit する。
fn run_canonicalize(
    idx: &crate::services::dir_index::DirIndex,
    mount_id: &str,
    parent_path: &str,
    dir_mtime_ns: i64,
    new_subdirs: &[(&str, i64)],
    new_files: &[(&str, i64, i64)],
) -> Result<crate::services::dir_index::writes::CanonicalizeReport, DirIndexError> {
    let bulk = idx.begin_bulk().unwrap();
    let tx = bulk.conn.unchecked_transaction()?;
    let subs: Vec<(String, i64)> = new_subdirs
        .iter()
        .map(|(n, m)| ((*n).to_owned(), *m))
        .collect();
    let files: Vec<(String, i64, i64)> = new_files
        .iter()
        .map(|(n, s, m)| ((*n).to_owned(), *s, *m))
        .collect();
    let report =
        canonicalize_parent_in_tx(&tx, mount_id, parent_path, dir_mtime_ns, &subs, &files)?;
    tx.commit()?;
    Ok(report)
}

#[test]
fn canonicalize_parentは削除されたファイル行を消す() {
    let (idx, _tmp) = setup();
    // 初期状態: a.jpg, b.jpg
    idx.ingest_walk_entry(&make_args(
        "/data/photos",
        "/data",
        MOUNT_A,
        vec![],
        vec![("a.jpg", 100, 1_000_000), ("b.jpg", 200, 2_000_000)],
    ))
    .unwrap();
    assert_eq!(idx.child_count(&format!("{MOUNT_A}/photos")).unwrap(), 2);

    // canonicalize: b.jpg のみ → a.jpg は削除される
    let parent = format!("{MOUNT_A}/photos");
    run_canonicalize(
        &idx,
        MOUNT_A,
        &parent,
        9_000_000,
        &[],
        &[("b.jpg", 200, 2_000_000)],
    )
    .unwrap();

    let entries = idx
        .query_page(&parent, "name-asc", Some(100), None)
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "b.jpg");
    // dir_mtime も更新される
    assert_eq!(idx.get_dir_mtime(&parent).unwrap(), Some(9_000_000));
}

#[test]
fn canonicalize_parentは新規ファイル行を投入する() {
    let (idx, _tmp) = setup();
    // 初期状態: a.jpg のみ
    idx.ingest_walk_entry(&make_args(
        "/data/photos",
        "/data",
        MOUNT_A,
        vec![],
        vec![("a.jpg", 100, 1_000_000)],
    ))
    .unwrap();

    let parent = format!("{MOUNT_A}/photos");
    run_canonicalize(
        &idx,
        MOUNT_A,
        &parent,
        9_000_000,
        &[],
        &[("a.jpg", 100, 1_000_000), ("c.jpg", 300, 3_000_000)],
    )
    .unwrap();

    let entries = idx
        .query_page(&parent, "name-asc", Some(100), None)
        .unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "a.jpg");
    assert_eq!(entries[1].name, "c.jpg");
}

#[test]
fn canonicalize_parentは消えたサブディレクトリ配下を再帰削除する() {
    let (idx, _tmp) = setup();
    let parent = format!("{MOUNT_A}/photos");
    let sub = format!("{parent}/sub");
    let deeper = format!("{sub}/deeper");

    // 初期状態: photos/sub/deeper/x.jpg + photos/sub/y.jpg + photos/file.jpg
    idx.ingest_walk_entry(&make_args(
        "/data/photos",
        "/data",
        MOUNT_A,
        vec![("sub", 1_000_000)],
        vec![("file.jpg", 100, 5_000_000)],
    ))
    .unwrap();
    idx.ingest_walk_entry(&make_args(
        "/data/photos/sub",
        "/data",
        MOUNT_A,
        vec![("deeper", 2_000_000)],
        vec![("y.jpg", 200, 6_000_000)],
    ))
    .unwrap();
    idx.ingest_walk_entry(&make_args(
        "/data/photos/sub/deeper",
        "/data",
        MOUNT_A,
        vec![],
        vec![("x.jpg", 300, 7_000_000)],
    ))
    .unwrap();

    assert!(idx.get_dir_mtime(&sub).unwrap().is_some());
    assert!(idx.get_dir_mtime(&deeper).unwrap().is_some());

    // canonicalize: parent から sub/ が消えた → sub 自身 + sub/deeper + 配下全て削除
    let report = run_canonicalize(
        &idx,
        MOUNT_A,
        &parent,
        9_000_000,
        &[],
        &[("file.jpg", 100, 5_000_000)],
    )
    .unwrap();

    assert!(report.cascaded_dirs >= 1, "sub の cascade が記録される");

    // photos 配下は file.jpg のみ
    let entries = idx
        .query_page(&parent, "name-asc", Some(100), None)
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "file.jpg");

    // sub と deeper の dir_entries / dir_meta が完全に消える
    assert_eq!(idx.child_count(&sub).unwrap(), 0);
    assert_eq!(idx.child_count(&deeper).unwrap(), 0);
    assert!(idx.get_dir_mtime(&sub).unwrap().is_none());
    assert!(idx.get_dir_mtime(&deeper).unwrap().is_none());
}

#[test]
fn canonicalize_parentはsibling名干渉を起こさない() {
    // codex 2回目指摘の Critical 1 回帰テスト
    // `dir` を消すとき、以下の sibling は範囲外でなければならない:
    //   - `dir-old` ('-' = 0x2D < '/' = 0x2F)
    //   - `dir.old` ('.' = 0x2E < '/')
    //   - `dir foo` (' ' = 0x20 < '/')
    //   - `dir_other` ('_' = 0x5F > '0' = 0x30、上限から外れる)
    //   - `dir0sub` ('0' = 0x30 はちょうど hi なので半開で除外される)
    let (idx, _tmp) = setup();
    let parent = format!("{MOUNT_A}/photos");
    let siblings = [
        "dir",
        "dir-old",
        "dir.old",
        "dir foo",
        "dir_other",
        "dir0sub",
    ];

    // 各 sibling を photos 配下に登録 + 配下にファイルを 1 つずつ
    let subdirs: Vec<(&str, i64)> = siblings.iter().map(|n| (*n, 1_000_000)).collect();
    idx.ingest_walk_entry(&make_args(
        "/data/photos",
        "/data",
        MOUNT_A,
        subdirs,
        vec![],
    ))
    .unwrap();
    for sib in &siblings {
        idx.ingest_walk_entry(&make_args(
            &format!("/data/photos/{sib}"),
            "/data",
            MOUNT_A,
            vec![],
            vec![("inside.jpg", 100, 2_000_000)],
        ))
        .unwrap();
    }

    // canonicalize: photos 配下から `dir` を消す (他 sibling は残す)
    let new_subs: Vec<(&str, i64)> = siblings
        .iter()
        .filter(|n| **n != "dir")
        .map(|n| (*n, 1_000_000))
        .collect();
    run_canonicalize(&idx, MOUNT_A, &parent, 9_000_000, &new_subs, &[]).unwrap();

    // `dir` 自身と `dir/inside.jpg` のみが消えること
    assert_eq!(idx.child_count(&format!("{parent}/dir")).unwrap(), 0);
    assert!(
        idx.get_dir_mtime(&format!("{parent}/dir"))
            .unwrap()
            .is_none()
    );

    // 他の sibling とその配下は完全保持
    for sib in &siblings {
        if *sib == "dir" {
            continue;
        }
        let sib_path = format!("{parent}/{sib}");
        assert_eq!(
            idx.child_count(&sib_path).unwrap(),
            1,
            "sibling '{sib}' の配下が誤削除された"
        );
        assert!(
            idx.get_dir_mtime(&sib_path).unwrap().is_some(),
            "sibling '{sib}' の dir_meta が誤削除された"
        );
    }

    // photos 配下のディレクトリエントリも `dir` のみ消える
    let entries = idx
        .query_page(&parent, "name-asc", Some(100), None)
        .unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(!names.contains(&"dir"), "dir エントリ自身が残っている");
    for sib in &siblings {
        if *sib == "dir" {
            continue;
        }
        assert!(
            names.contains(sib),
            "sibling '{sib}' エントリが誤削除された"
        );
    }
}

#[test]
fn name_has_invalid_byteは特殊文字を検出する() {
    assert!(!name_has_invalid_byte("normal.jpg"));
    assert!(!name_has_invalid_byte("日本語.png"));
    assert!(!name_has_invalid_byte("with space.txt"));
    assert!(name_has_invalid_byte("bad/name"));
    assert!(name_has_invalid_byte("nul\0byte"));
    assert!(name_has_invalid_byte("back\\slash"));
}

#[test]
fn canonicalize_parentはnew_nameに不正文字を含むとErrを返す() {
    let (idx, _tmp) = setup();
    let parent = format!("{MOUNT_A}/photos");

    let err = run_canonicalize(
        &idx,
        MOUNT_A,
        &parent,
        1_000_000,
        &[("bad/dir", 1_000_000)],
        &[],
    )
    .unwrap_err();
    matches!(err, DirIndexError::Other(_));

    let err = run_canonicalize(
        &idx,
        MOUNT_A,
        &parent,
        1_000_000,
        &[],
        &[("nul\0name", 100, 1_000_000)],
    )
    .unwrap_err();
    matches!(err, DirIndexError::Other(_));

    let err = run_canonicalize(
        &idx,
        MOUNT_A,
        &parent,
        1_000_000,
        &[],
        &[("back\\slash.jpg", 100, 1_000_000)],
    )
    .unwrap_err();
    matches!(err, DirIndexError::Other(_));
}

#[test]
fn canonicalize_parentは永続層のold_dir名に不正文字が含まれていたらCorruptPersistentName_を返す() {
    let (idx, tmp) = setup();
    let parent = format!("{MOUNT_A}/photos");
    // 直接 INSERT で不正 name を流し込む (通常 API では絶対起きないが防御層検証用)
    {
        let conn = rusqlite::Connection::open(tmp.path()).unwrap();
        conn.execute(
            "INSERT INTO dir_entries (parent_path, name, kind, sort_key, size_bytes, mtime_ns) \
             VALUES (?1, ?2, 'directory', ?3, NULL, ?4)",
            rusqlite::params![&parent, "bad/dir", "bad/dir", 1_000_000_i64],
        )
        .unwrap();
    }

    let err = run_canonicalize(&idx, MOUNT_A, &parent, 9_000_000, &[], &[]).unwrap_err();
    match err {
        DirIndexError::CorruptPersistentName {
            mount_id,
            parent_path,
            name,
        } => {
            assert_eq!(mount_id, MOUNT_A);
            assert_eq!(parent_path, parent);
            assert_eq!(name, "bad/dir");
        }
        other => panic!("CorruptPersistentName を期待: {other:?}"),
    }

    // 既存行は維持される (リカバリは呼び出し側で実行する設計のため)
    let conn = rusqlite::Connection::open(tmp.path()).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM dir_entries WHERE parent_path = ?1",
            rusqlite::params![&parent],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "破損行は abort 時に維持される");
}

#[test]
fn recover_from_corrupt_persistent_nameは新tx_でmount全削除とfingerprintクリアを実行する() {
    use crate::services::dir_index::writes::recover_from_corrupt_persistent_name;
    use crate::services::indexer::Indexer;

    let (idx, _tmp_dir) = setup();
    // 別の tempfile で Indexer DB を用意
    let idx_db = tempfile::NamedTempFile::new().unwrap();
    let indexer = Indexer::new(idx_db.path().to_str().unwrap());
    indexer.init_db().unwrap();

    // mount_a + mount_b を populate
    idx.ingest_walk_entry(&make_args(
        "/data/foo",
        "/data",
        MOUNT_A,
        vec![],
        vec![("a.jpg", 100, 1_000_000)],
    ))
    .unwrap();
    idx.ingest_walk_entry(&make_args(
        "/data/bar",
        "/data",
        MOUNT_B,
        vec![],
        vec![("b.jpg", 200, 2_000_000)],
    ))
    .unwrap();
    indexer.save_mount_fingerprint(&[MOUNT_A, MOUNT_B]).unwrap();
    assert!(
        indexer
            .check_mount_fingerprint(&[MOUNT_A, MOUNT_B])
            .unwrap()
    );

    // recover: mount_a のみ全削除 + fingerprint クリア
    recover_from_corrupt_persistent_name(
        &idx,
        &indexer,
        MOUNT_A,
        &format!("{MOUNT_A}/foo"),
        "bad/name",
    )
    .unwrap();

    // mount_a の DirIndex 行が全て消える
    assert_eq!(idx.child_count(&format!("{MOUNT_A}/foo")).unwrap(), 0);
    // mount_b は無事
    assert_eq!(idx.child_count(&format!("{MOUNT_B}/bar")).unwrap(), 1);
    // fingerprint がクリアされ、次回起動が cold start に強制される
    assert!(
        !indexer
            .check_mount_fingerprint(&[MOUNT_A, MOUNT_B])
            .unwrap()
    );
}
