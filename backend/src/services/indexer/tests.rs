//! Indexer 統合テスト
//!
//! `indexer/mod.rs` から分離した `#[cfg(test)]` 一式。

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

use super::*;
use crate::services::path_security::PathSecurity;

/// テスト用の一時 DB パスでインデクサーを生成する
fn setup_indexer() -> (Indexer, tempfile::NamedTempFile) {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let indexer = Indexer::new(tmp.path().to_str().unwrap());
    indexer.init_db().unwrap();
    (indexer, tmp)
}

/// テスト用エントリを生成する
fn make_entry(relative_path: &str, name: &str, kind: &str) -> IndexEntry {
    IndexEntry {
        relative_path: relative_path.to_owned(),
        name: name.to_owned(),
        kind: kind.to_owned(),
        size_bytes: Some(1024),
        mtime_ns: 1_000_000_000,
    }
}

#[test]
fn init_dbでスキーマが作成される() {
    let (indexer, _tmp) = setup_indexer();
    let count = indexer.entry_count().unwrap();
    assert_eq!(count, 0);
}

#[test]
fn エントリの追加と検索ができる() {
    let (indexer, _tmp) = setup_indexer();

    let entry = make_entry("photos/sunset.jpg", "sunset.jpg", "image");
    indexer.add_entry(&entry).unwrap();

    let (hits, has_more) = indexer
        .search(&SearchParams {
            query: "sunset",
            kind: None,
            limit: 10,
            offset: 0,
            scope_prefix: None,
        })
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert!(!has_more);
    assert_eq!(hits[0].name, "sunset.jpg");
    assert_eq!(hits[0].relative_path, "photos/sunset.jpg");
    assert_eq!(hits[0].kind, "image");
}

#[test]
fn kind指定で検索をフィルタできる() {
    let (indexer, _tmp) = setup_indexer();

    indexer
        .add_entry(&make_entry("videos/clip.mp4", "clip.mp4", "video"))
        .unwrap();
    indexer
        .add_entry(&make_entry("docs/manual.pdf", "manual.pdf", "pdf"))
        .unwrap();

    // kind="video" で検索 — "clip" は 4 文字なので FTS5 パス
    let (hits, _) = indexer
        .search(&SearchParams {
            query: "clip",
            kind: Some("video"),
            limit: 10,
            offset: 0,
            scope_prefix: None,
        })
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "video");

    // kind="pdf" で同じクエリ — ヒットしない
    let (hits, _) = indexer
        .search(&SearchParams {
            query: "clip",
            kind: Some("pdf"),
            limit: 10,
            offset: 0,
            scope_prefix: None,
        })
        .unwrap();
    assert!(hits.is_empty());
}

#[test]
fn エントリの削除で検索から消える() {
    let (indexer, _tmp) = setup_indexer();

    let entry = make_entry("photos/beach.jpg", "beach.jpg", "image");
    indexer.add_entry(&entry).unwrap();

    // 削除前: 検索にヒットする
    let (hits, _) = indexer
        .search(&SearchParams {
            query: "beach",
            kind: None,
            limit: 10,
            offset: 0,
            scope_prefix: None,
        })
        .unwrap();
    assert_eq!(hits.len(), 1);

    // 削除
    indexer.remove_entry("photos/beach.jpg").unwrap();

    // 削除後: 検索にヒットしない
    let (hits, _) = indexer
        .search(&SearchParams {
            query: "beach",
            kind: None,
            limit: 10,
            offset: 0,
            scope_prefix: None,
        })
        .unwrap();
    assert!(hits.is_empty());
}

#[test]
fn 二文字クエリでlikeフォールバック() {
    let (indexer, _tmp) = setup_indexer();

    let entry = make_entry("tests/ab_test.mp4", "ab_test.mp4", "video");
    indexer.add_entry(&entry).unwrap();

    // "ab" は 2 文字 → LIKE フォールバック
    let (hits, _) = indexer
        .search(&SearchParams {
            query: "ab",
            kind: None,
            limit: 10,
            offset: 0,
            scope_prefix: None,
        })
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name, "ab_test.mp4");
}

#[test]
fn 日本語ファイル名の部分一致検索() {
    let (indexer, _tmp) = setup_indexer();

    let entry = make_entry("動画/テスト動画.mp4", "テスト動画.mp4", "video");
    indexer.add_entry(&entry).unwrap();

    // "テスト" は 3 文字 → FTS5 パス
    let (hits, _) = indexer
        .search(&SearchParams {
            query: "テスト",
            kind: None,
            limit: 10,
            offset: 0,
            scope_prefix: None,
        })
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name, "テスト動画.mp4");
}

#[test]
fn 日本語スペース区切りで複数トークンがand検索になる() {
    let (indexer, _tmp) = setup_indexer();

    indexer
        .add_entry(&make_entry(
            "動画/夏の旅行記録.mp4",
            "夏の旅行記録.mp4",
            "video",
        ))
        .unwrap();
    indexer
        .add_entry(&make_entry("動画/冬の山.mp4", "冬の山.mp4", "video"))
        .unwrap();
    indexer
        .add_entry(&make_entry("動画/旅行メモ.mp4", "旅行メモ.mp4", "video"))
        .unwrap();

    // 「夏 旅行」→ 両方を含むファイルのみヒット (AND)
    let (hits, _) = indexer
        .search(&SearchParams {
            query: "夏 旅行",
            kind: None,
            limit: 10,
            offset: 0,
            scope_prefix: None,
        })
        .unwrap();
    assert_eq!(
        hits.len(),
        1,
        "hits={:?}",
        hits.iter().map(|h| &h.name).collect::<Vec<_>>()
    );
    assert_eq!(hits[0].name, "夏の旅行記録.mp4");
}

#[test]
fn 三文字以上と二文字日本語の混在トークンがandになる() {
    let (indexer, _tmp) = setup_indexer();

    indexer
        .add_entry(&make_entry(
            "作品/テスト画像集.zip",
            "テスト画像集.zip",
            "archive",
        ))
        .unwrap();
    indexer
        .add_entry(&make_entry("作品/テスト.zip", "テスト.zip", "archive"))
        .unwrap();
    indexer
        .add_entry(&make_entry(
            "素材/画像まとめ.zip",
            "画像まとめ.zip",
            "archive",
        ))
        .unwrap();

    // 「テスト 画像」→ 「テスト」(3文字, FTS) AND 「画像」(2文字, LIKE)
    let (hits, _) = indexer
        .search(&SearchParams {
            query: "テスト 画像",
            kind: None,
            limit: 10,
            offset: 0,
            scope_prefix: None,
        })
        .unwrap();
    assert_eq!(
        hits.len(),
        1,
        "hits={:?}",
        hits.iter().map(|h| &h.name).collect::<Vec<_>>()
    );
    assert_eq!(hits[0].name, "テスト画像集.zip");
}

#[test]
fn 二文字日本語フォルダ名で検索できる() {
    let (indexer, _tmp) = setup_indexer();

    indexer
        .add_entry(&make_entry("mount/写真", "写真", "directory"))
        .unwrap();
    indexer
        .add_entry(&make_entry("mount/動画", "動画", "directory"))
        .unwrap();
    indexer
        .add_entry(&make_entry("mount/写真/beach.jpg", "beach.jpg", "image"))
        .unwrap();

    // 「写真」(2文字) で検索 → 「写真」ディレクトリが name 一致で 1 件ヒット
    let (hits, _) = indexer
        .search(&SearchParams {
            query: "写真",
            kind: Some("directory"),
            limit: 10,
            offset: 0,
            scope_prefix: None,
        })
        .unwrap();
    assert_eq!(
        hits.len(),
        1,
        "hits={:?}",
        hits.iter().map(|h| &h.name).collect::<Vec<_>>()
    );
    assert_eq!(hits[0].name, "写真");
    assert_eq!(hits[0].kind, "directory");
}

#[test]
fn 二文字日本語複数トークンがand検索になる() {
    let (indexer, _tmp) = setup_indexer();

    indexer
        .add_entry(&make_entry(
            "写真/風景の記録.zip",
            "風景の記録.zip",
            "archive",
        ))
        .unwrap();
    indexer
        .add_entry(&make_entry("写真/夜景.zip", "夜景.zip", "archive"))
        .unwrap();
    indexer
        .add_entry(&make_entry("動画/風景.mp4", "風景.mp4", "video"))
        .unwrap();

    // 「写真 風景」→ 両トークン2文字。LIKE AND 合流でヒット。
    // relative_path が「写真/...」で name に「風景」を含むものだけ。
    let (hits, _) = indexer
        .search(&SearchParams {
            query: "写真 風景",
            kind: None,
            limit: 10,
            offset: 0,
            scope_prefix: None,
        })
        .unwrap();
    assert_eq!(
        hits.len(),
        1,
        "hits={:?}",
        hits.iter().map(|h| &h.name).collect::<Vec<_>>()
    );
    assert_eq!(hits[0].relative_path, "写真/風景の記録.zip");
}

#[test]
fn 特殊文字入力でエラーにならない() {
    let (indexer, _tmp) = setup_indexer();

    // ダブルクォートやアスタリスクを含むクエリでエラーにならない
    let result = indexer.search(&SearchParams {
        query: "\"test*",
        kind: None,
        limit: 10,
        offset: 0,
        scope_prefix: None,
    });
    assert!(result.is_ok());
}

#[test]
fn mount_fingerprintの保存と検証() {
    let (indexer, _tmp) = setup_indexer();

    let ids = vec!["aaaaaaaaaaaaaaaa", "bbbbbbbbbbbbbbbb"];
    indexer.save_mount_fingerprint(&ids).unwrap();

    // 同じ ID リストで検証 → true
    assert!(indexer.check_mount_fingerprint(&ids).unwrap());

    // 異なる ID リストで検証 → false
    let different = vec!["mount_c"];
    assert!(!indexer.check_mount_fingerprint(&different).unwrap());

    // 順序を入れ替えても一致する (ソート済みフィンガープリント)
    let reversed = vec!["bbbbbbbbbbbbbbbb", "aaaaaaaaaaaaaaaa"];
    assert!(indexer.check_mount_fingerprint(&reversed).unwrap());
}

#[test]
fn clear_mount_fingerprintは保存済み値を削除する() {
    let (indexer, _tmp) = setup_indexer();

    let ids = vec!["aaaaaaaaaaaaaaaa", "bbbbbbbbbbbbbbbb"];
    indexer.save_mount_fingerprint(&ids).unwrap();
    assert!(indexer.check_mount_fingerprint(&ids).unwrap());

    indexer.clear_mount_fingerprint().unwrap();

    // クリア後は任意の ID リストに対して false
    assert!(!indexer.check_mount_fingerprint(&ids).unwrap());
    // load_stored_mount_ids も空 Vec を返す（次回起動は cold start に落ちる）
    assert!(indexer.load_stored_mount_ids().unwrap().is_empty());
}

#[test]
fn clear_mount_fingerprintは未保存時no_opで成功する() {
    let (indexer, _tmp) = setup_indexer();

    // 未保存状態でもエラーにならず no-op で成功
    indexer.clear_mount_fingerprint().unwrap();
    // 冪等性: 2 度目も成功
    indexer.clear_mount_fingerprint().unwrap();

    assert!(indexer.load_stored_mount_ids().unwrap().is_empty());
}

#[test]
fn mark_warm_startでis_readyとis_staleが設定される() {
    let (indexer, _tmp) = setup_indexer();

    // 初期状態: 両方 false
    assert!(!indexer.is_ready());
    assert!(!indexer.is_stale());

    indexer.mark_warm_start();

    assert!(indexer.is_ready());
    assert!(indexer.is_stale());
}

#[test]
fn has_moreがlimit超過時にtrueになる() {
    let (indexer, _tmp) = setup_indexer();

    // 3 件のエントリを追加
    for i in 0..3 {
        indexer
            .add_entry(&make_entry(
                &format!("photos/image_{i}.jpg"),
                &format!("image_{i}.jpg"),
                "image",
            ))
            .unwrap();
    }

    // limit=2 で検索 → has_more=true
    let (hits, has_more) = indexer
        .search(&SearchParams {
            query: "image",
            kind: None,
            limit: 2,
            offset: 0,
            scope_prefix: None,
        })
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert!(has_more);

    // limit=10 で検索 → has_more=false
    let (hits, has_more) = indexer
        .search(&SearchParams {
            query: "image",
            kind: None,
            limit: 10,
            offset: 0,
            scope_prefix: None,
        })
        .unwrap();
    assert_eq!(hits.len(), 3);
    assert!(!has_more);
}

// --- scope_prefix テスト ---

#[test]
fn scope_prefix付きでfts検索がプレフィックス一致のみ返す() {
    let (indexer, _tmp) = setup_indexer();

    indexer
        .add_entry(&make_entry(
            "mount1/photos/sunset.jpg",
            "sunset.jpg",
            "image",
        ))
        .unwrap();
    indexer
        .add_entry(&make_entry(
            "mount1/videos/sunset.mp4",
            "sunset.mp4",
            "video",
        ))
        .unwrap();
    indexer
        .add_entry(&make_entry(
            "mount2/photos/sunset.png",
            "sunset.png",
            "image",
        ))
        .unwrap();

    // scope_prefix="mount1/photos" → mount1/photos 配下のみ
    let (hits, _) = indexer
        .search(&SearchParams {
            query: "sunset",
            kind: None,
            limit: 10,
            offset: 0,
            scope_prefix: Some("mount1/photos"),
        })
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].relative_path, "mount1/photos/sunset.jpg");
}

#[test]
fn scope_prefix付きでlike検索がプレフィックス一致のみ返す() {
    let (indexer, _tmp) = setup_indexer();

    indexer
        .add_entry(&make_entry("mount1/ab_test.jpg", "ab_test.jpg", "image"))
        .unwrap();
    indexer
        .add_entry(&make_entry("mount2/ab_other.jpg", "ab_other.jpg", "image"))
        .unwrap();

    // "ab" は 2 文字 → LIKE フォールバック + scope_prefix
    let (hits, _) = indexer
        .search(&SearchParams {
            query: "ab",
            kind: None,
            limit: 10,
            offset: 0,
            scope_prefix: Some("mount1"),
        })
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].relative_path, "mount1/ab_test.jpg");
}

#[test]
fn scope_prefix内のlikeワイルドカードがエスケープされる() {
    let (indexer, _tmp) = setup_indexer();

    // ディレクトリ名に % と _ を含む
    indexer
        .add_entry(&make_entry("mount/dir_100%/test.jpg", "test.jpg", "image"))
        .unwrap();
    indexer
        .add_entry(&make_entry("mount/dir_200/test.jpg", "test.jpg", "image"))
        .unwrap();

    // scope_prefix に % を含む → エスケープされて literal match
    let (hits, _) = indexer
        .search(&SearchParams {
            query: "test",
            kind: None,
            limit: 10,
            offset: 0,
            scope_prefix: Some("mount/dir_100%"),
        })
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].relative_path, "mount/dir_100%/test.jpg");
}

#[test]
fn scope_prefixがnoneの場合はフィルタなしで全件返す() {
    let (indexer, _tmp) = setup_indexer();

    indexer
        .add_entry(&make_entry("mount1/photo.jpg", "photo.jpg", "image"))
        .unwrap();
    indexer
        .add_entry(&make_entry("mount2/photo.jpg", "photo.jpg", "image"))
        .unwrap();

    let (hits, _) = indexer
        .search(&SearchParams {
            query: "photo",
            kind: None,
            limit: 10,
            offset: 0,
            scope_prefix: None,
        })
        .unwrap();
    assert_eq!(hits.len(), 2);
}

// --- escape_like_pattern テスト ---

#[test]
fn likeパターンのワイルドカードがエスケープされる() {
    use super::helpers::escape_like_pattern;
    assert_eq!(escape_like_pattern("normal/path"), "normal/path");
    assert_eq!(escape_like_pattern("dir_100%"), "dir\\_100\\%");
    assert_eq!(escape_like_pattern("back\\slash"), "back\\\\slash");
}

// --- scan_directory / incremental_scan / rebuild テスト ---

/// テスト用ディレクトリ構造とインデクサーを生成する
///
/// root/
///   sub1/
///     movie.mp4  (5 bytes)
///   doc.pdf      (3 bytes)
///   image.jpg    (3 bytes)  -- インデックス対象外 (画像)
struct ScanTestEnv {
    #[allow(dead_code, reason = "TempDir のドロップでディレクトリを保持")]
    _dir: TempDir,
    root: PathBuf,
    indexer: Indexer,
    #[allow(dead_code, reason = "NamedTempFile のドロップで DB を保持")]
    _db_file: tempfile::NamedTempFile,
}

impl ScanTestEnv {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::create_dir_all(root.join("sub1")).unwrap();
        fs::write(root.join("sub1/movie.mp4"), b"video").unwrap();
        fs::write(root.join("doc.pdf"), b"pdf").unwrap();
        fs::write(root.join("image.jpg"), b"img").unwrap();

        let db_file = tempfile::NamedTempFile::new().unwrap();
        let indexer = Indexer::new(db_file.path().to_str().unwrap());
        indexer.init_db().unwrap();

        Self {
            _dir: dir,
            root,
            indexer,
            _db_file: db_file,
        }
    }

    fn path_security(&self) -> PathSecurity {
        PathSecurity::new(vec![self.root.clone()], false).unwrap()
    }
}

#[test]
fn scan_directoryでディレクトリを走査してインデックスに登録する() {
    let env = ScanTestEnv::new();
    let ps = env.path_security();

    let (count, report) = env
        .indexer
        .scan_directory(&env.root, &ps, "1111111111111111", 2, None, &|| false)
        .unwrap();

    // sub1 (directory) + movie.mp4 (video) + doc.pdf (pdf) = 3
    // image.jpg は画像なのでインデックス対象外
    assert_eq!(count, 3);
    assert_eq!(report.error_count(), 0);
    assert_eq!(env.indexer.entry_count().unwrap(), 3);

    // is_ready が true に設定される
    assert!(env.indexer.is_ready());
    assert!(!env.indexer.is_stale());
}

#[test]
fn incremental_scanで変更ファイルのみ更新する() {
    let env = ScanTestEnv::new();
    let ps = env.path_security();

    // 初回フルスキャン
    env.indexer
        .scan_directory(&env.root, &ps, "1111111111111111", 2, None, &|| false)
        .unwrap();
    assert_eq!(env.indexer.entry_count().unwrap(), 3);

    // ファイルを追加して sub1 の mtime を変える
    fs::write(env.root.join("sub1/extra.mp4"), b"extra").unwrap();

    let (added, updated, deleted) = env
        .indexer
        .incremental_scan(&env.root, &ps, "1111111111111111", 2, None, &|| false)
        .unwrap();

    // extra.mp4 が追加される
    assert!(added >= 1, "added={added}, 少なくとも1件追加されるべき");
    // 削除はない
    assert_eq!(deleted, 0);
    // 合計 4 件 (sub1 + movie.mp4 + doc.pdf + extra.mp4)
    assert_eq!(env.indexer.entry_count().unwrap(), 4);

    // is_ready が true に設定される
    assert!(env.indexer.is_ready());
    assert!(!env.indexer.is_stale());

    // updated は sub1 の mtime 変更で 0 以上 (ディレクトリ mtime が更新されていれば updated)
    let _ = updated; // コンパイラ警告抑制
}

#[test]
fn incremental_scanでネストされたディレクトリ内の新規ファイルを検出する() {
    // root/dir1/dir2/movie.mp4 を作成
    let dir = TempDir::new().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(root.join("dir1/dir2")).unwrap();
    fs::write(root.join("dir1/dir2/movie.mp4"), b"video").unwrap();

    let db_file = tempfile::NamedTempFile::new().unwrap();
    let indexer = Indexer::new(db_file.path().to_str().unwrap());
    indexer.init_db().unwrap();
    let ps = PathSecurity::new(vec![root.clone()], false).unwrap();

    // 初回フルスキャン: dir1 + dir2 + movie.mp4 = 3
    indexer
        .scan_directory(&root, &ps, "abcdef0123456789", 2, None, &|| false)
        .unwrap();
    assert_eq!(indexer.entry_count().unwrap(), 3);

    // dir1 の mtime を記録
    let dir1_mtime = fs::metadata(root.join("dir1")).unwrap().modified().unwrap();

    // dir2 にファイル追加 (dir2 の mtime は変わるが dir1 の mtime は変わらない)
    fs::write(root.join("dir1/dir2/new.mp4"), b"new video").unwrap();

    // dir1 の mtime を元に戻す (テスト環境の安全策)
    let times = std::fs::FileTimes::new().set_modified(dir1_mtime);
    std::fs::File::open(root.join("dir1"))
        .unwrap()
        .set_times(times)
        .unwrap();

    // dir1 の mtime が変わっていないことを確認
    let dir1_mtime_after = fs::metadata(root.join("dir1")).unwrap().modified().unwrap();
    assert_eq!(dir1_mtime, dir1_mtime_after);

    // incremental_scan で new.mp4 が検出されるべき
    let (added, _updated, deleted) = indexer
        .incremental_scan(&root, &ps, "abcdef0123456789", 2, None, &|| false)
        .unwrap();

    assert!(added >= 1, "new.mp4 が追加されるべき (added={added})");
    assert_eq!(deleted, 0, "削除はないはず");
    // dir1 + dir2 + movie.mp4 + new.mp4 = 4
    assert_eq!(indexer.entry_count().unwrap(), 4);
}

#[test]
fn rebuildでインデックスを再構築する() {
    let env = ScanTestEnv::new();
    let ps = env.path_security();

    // 初回スキャン
    env.indexer
        .scan_directory(&env.root, &ps, "1111111111111111", 2, None, &|| false)
        .unwrap();
    assert_eq!(env.indexer.entry_count().unwrap(), 3);

    // rebuild
    let count = env
        .indexer
        .rebuild(&env.root, &ps, "1111111111111111", &|| false)
        .unwrap();

    // 同じ件数で再構築される
    assert_eq!(count, 3);
    assert_eq!(env.indexer.entry_count().unwrap(), 3);

    // Indexer 内部で排他フラグは持たなくなったが ready フラグは維持される
    // （rebuild 全体排他は AppState.rebuild_guard に移動、Phase B）
    assert!(env.indexer.is_ready());
}

#[test]
fn list_entry_pathsは空テーブルで空vecを返す() {
    let (indexer, _tmp) = setup_indexer();
    let paths = indexer.list_entry_paths().unwrap();
    assert!(paths.is_empty());
}

#[test]
fn list_entry_pathsは登録済みrelative_pathを返す() {
    let (indexer, _tmp) = setup_indexer();
    indexer
        .add_entry(&make_entry(
            "mount_a/photos/sunset.jpg",
            "sunset.jpg",
            "image",
        ))
        .unwrap();
    indexer
        .add_entry(&make_entry("mount_a/videos/clip.mp4", "clip.mp4", "video"))
        .unwrap();
    indexer
        .add_entry(&make_entry("mount_b/docs/manual.pdf", "manual.pdf", "pdf"))
        .unwrap();

    let mut paths = indexer.list_entry_paths().unwrap();
    paths.sort();
    assert_eq!(
        paths,
        vec![
            "mount_a/photos/sunset.jpg".to_string(),
            "mount_a/videos/clip.mp4".to_string(),
            "mount_b/docs/manual.pdf".to_string(),
        ]
    );
}

// --- マルチマウント回帰テスト ---
//
// scan_directory / incremental_scan / rebuild を複数マウントで逐次実行したとき、
// 他マウントのエントリが削除されないことを保証する。
// `delete_unseen` / `Indexer::rebuild` の `DELETE` が mount スコープに限定される
// 修正を担保する回帰テスト。

/// 2 マウント構成のスキャン用フィクスチャ
struct MultiMountEnv {
    #[allow(dead_code, reason = "TempDir のドロップでディレクトリを保持")]
    _dir_a: TempDir,
    #[allow(dead_code, reason = "TempDir のドロップでディレクトリを保持")]
    _dir_b: TempDir,
    root_a: PathBuf,
    root_b: PathBuf,
    indexer: Indexer,
    #[allow(dead_code, reason = "NamedTempFile のドロップで DB を保持")]
    _db_file: tempfile::NamedTempFile,
}

impl MultiMountEnv {
    /// 2 マウント (`mount_a` / `mount_b`) を作成し、初期スキャンで両方を登録する
    fn new_with_initial_scan() -> Self {
        let dir_a = TempDir::new().unwrap();
        let dir_b = TempDir::new().unwrap();
        let root_a = fs::canonicalize(dir_a.path()).unwrap();
        let root_b = fs::canonicalize(dir_b.path()).unwrap();

        // mount_a: sub_a/ + movie_a.mp4 + doc_a.pdf
        fs::create_dir_all(root_a.join("sub_a")).unwrap();
        fs::write(root_a.join("sub_a/inner_a.mp4"), b"a-video").unwrap();
        fs::write(root_a.join("doc_a.pdf"), b"pdf-a").unwrap();
        // mount_b: sub_b/ + movie_b.mp4 + doc_b.pdf
        fs::create_dir_all(root_b.join("sub_b")).unwrap();
        fs::write(root_b.join("sub_b/inner_b.mp4"), b"b-video").unwrap();
        fs::write(root_b.join("doc_b.pdf"), b"pdf-b").unwrap();

        let db_file = tempfile::NamedTempFile::new().unwrap();
        let indexer = Indexer::new(db_file.path().to_str().unwrap());
        indexer.init_db().unwrap();

        let ps_a = PathSecurity::new(vec![root_a.clone()], false).unwrap();
        let ps_b = PathSecurity::new(vec![root_b.clone()], false).unwrap();
        indexer
            .scan_directory(&root_a, &ps_a, "aaaaaaaaaaaaaaaa", 2, None, &|| false)
            .unwrap();
        indexer
            .scan_directory(&root_b, &ps_b, "bbbbbbbbbbbbbbbb", 2, None, &|| false)
            .unwrap();

        Self {
            _dir_a: dir_a,
            _dir_b: dir_b,
            root_a,
            root_b,
            indexer,
            _db_file: db_file,
        }
    }

    fn ps_a(&self) -> PathSecurity {
        PathSecurity::new(vec![self.root_a.clone()], false).unwrap()
    }

    fn ps_b(&self) -> PathSecurity {
        PathSecurity::new(vec![self.root_b.clone()], false).unwrap()
    }

    /// `list_entry_paths()` のうち指定プレフィックスで始まる件数を返す
    fn count_with_prefix(&self, prefix: &str) -> usize {
        self.indexer
            .list_entry_paths()
            .unwrap()
            .into_iter()
            .filter(|p| p.starts_with(prefix))
            .count()
    }
}

#[test]
fn incremental_scanは他マウントのエントリを削除しない() {
    // 2 マウントを初期スキャンで登録 → mount_a 分 + mount_b 分の両方が entries にある
    let env = MultiMountEnv::new_with_initial_scan();
    let before_a = env.count_with_prefix("aaaaaaaaaaaaaaaa/");
    let before_b = env.count_with_prefix("bbbbbbbbbbbbbbbb/");
    assert!(
        before_a >= 3,
        "mount_a の初期件数が想定より少ない: {before_a}"
    );
    assert!(
        before_b >= 3,
        "mount_b の初期件数が想定より少ない: {before_b}"
    );

    // mount_a → mount_b の順で incremental_scan を逐次実行
    env.indexer
        .incremental_scan(
            &env.root_a,
            &env.ps_a(),
            "aaaaaaaaaaaaaaaa",
            2,
            None,
            &|| false,
        )
        .unwrap();
    env.indexer
        .incremental_scan(
            &env.root_b,
            &env.ps_b(),
            "bbbbbbbbbbbbbbbb",
            2,
            None,
            &|| false,
        )
        .unwrap();

    // 両マウントのエントリが残っていることを prefix 厳密検証
    let after_a = env.count_with_prefix("aaaaaaaaaaaaaaaa/");
    let after_b = env.count_with_prefix("bbbbbbbbbbbbbbbb/");
    assert_eq!(
        after_a, before_a,
        "mount_a のエントリが他マウントの incremental_scan で削除された"
    );
    assert_eq!(
        after_b, before_b,
        "mount_b のエントリが他マウントの incremental_scan で削除された"
    );
}

#[test]
fn rebuildは他マウントのエントリを削除しない() {
    let env = MultiMountEnv::new_with_initial_scan();
    let before_a = env.count_with_prefix("aaaaaaaaaaaaaaaa/");
    let before_b = env.count_with_prefix("bbbbbbbbbbbbbbbb/");
    assert!(before_a >= 3);
    assert!(before_b >= 3);

    // mount_a → mount_b の順で rebuild
    env.indexer
        .rebuild(&env.root_a, &env.ps_a(), "aaaaaaaaaaaaaaaa", &|| false)
        .unwrap();
    env.indexer
        .rebuild(&env.root_b, &env.ps_b(), "bbbbbbbbbbbbbbbb", &|| false)
        .unwrap();

    // 両マウントのエントリが残っていることを prefix 厳密検証
    let after_a = env.count_with_prefix("aaaaaaaaaaaaaaaa/");
    let after_b = env.count_with_prefix("bbbbbbbbbbbbbbbb/");
    assert_eq!(
        after_a, before_a,
        "mount_a のエントリが他マウントの rebuild で削除された"
    );
    assert_eq!(
        after_b, before_b,
        "mount_b のエントリが他マウントの rebuild で削除された"
    );
}

#[test]
fn incremental_scanで自マウントの削除済みファイルだけを消す() {
    let env = MultiMountEnv::new_with_initial_scan();
    let before_a = env.count_with_prefix("aaaaaaaaaaaaaaaa/");
    let before_b = env.count_with_prefix("bbbbbbbbbbbbbbbb/");

    // mount_a 配下の doc_a.pdf を物理削除
    fs::remove_file(env.root_a.join("doc_a.pdf")).unwrap();
    // mount_a の incremental_scan で削除が反映されるはず
    env.indexer
        .incremental_scan(
            &env.root_a,
            &env.ps_a(),
            "aaaaaaaaaaaaaaaa",
            2,
            None,
            &|| false,
        )
        .unwrap();

    let after_a = env.count_with_prefix("aaaaaaaaaaaaaaaa/");
    let after_b = env.count_with_prefix("bbbbbbbbbbbbbbbb/");
    assert_eq!(
        after_a,
        before_a - 1,
        "mount_a の削除済みファイルがインデックスから消されていない"
    );
    assert_eq!(
        after_b, before_b,
        "mount_b のエントリが mount_a の incremental_scan で影響を受けた"
    );
}

/// サブディレクトリの read permission を外して `scan_one` の `read_dir` を
/// 失敗させる。`WalkReport.error_count >= 1` が立ち、後続の削除判定を
/// トリガーする回帰テスト用の補助関数。
///
/// `TempDir` の drop で削除できなくなるため、テスト末尾で必ず
/// `restore_read_perms` を呼んで 0o755 に戻す。
#[cfg(unix)]
fn strip_read_perms(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(path, perms).unwrap();
}

#[cfg(unix)]
fn restore_read_perms(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

#[cfg(unix)]
#[test]
fn incremental_scanはread_dir失敗時に既存行を削除しない() {
    // mount_a 内のサブディレクトリに read 権限が無いケース。
    // scan_one の read_dir 失敗 → WalkReport.error_count >= 1 → delete_unseen スキップ。
    let env = MultiMountEnv::new_with_initial_scan();
    // sub_a 配下に追加の動画ファイル + 更に深いサブディレクトリを作り、
    // その一部から read 権限を剥奪することで走査エラーを誘発する
    let restricted = env.root_a.join("sub_a/restricted_dir");
    fs::create_dir_all(&restricted).unwrap();
    fs::write(restricted.join("deep.mp4"), b"deep").unwrap();

    // 追加エントリを一度インデックスに登録
    env.indexer
        .incremental_scan(
            &env.root_a,
            &env.ps_a(),
            "aaaaaaaaaaaaaaaa",
            2,
            None,
            &|| false,
        )
        .unwrap();
    let before_a = env.count_with_prefix("aaaaaaaaaaaaaaaa/");
    assert!(before_a >= 4);

    // restricted ディレクトリから read 権限を剥奪
    strip_read_perms(&restricted);

    // incremental_scan 実行。sub_a から restricted を subdir として列挙するが、
    // restricted 自体の read_dir は EACCES で失敗し WalkReport.error_count >= 1
    let _ = env.indexer.incremental_scan(
        &env.root_a,
        &env.ps_a(),
        "aaaaaaaaaaaaaaaa",
        2,
        None,
        &|| false,
    );

    let after_a = env.count_with_prefix("aaaaaaaaaaaaaaaa/");

    // 権限を戻す (TempDir の drop で削除できるようにする)
    restore_read_perms(&restricted);

    // 既存行が全件残っていることを確認（read_dir 失敗時に delete_unseen がスキップされた）
    assert_eq!(
        after_a, before_a,
        "read_dir 失敗でも既存行を保護すべきだが削除された"
    );
}

#[cfg(unix)]
#[test]
fn rebuildはスキャン失敗時に既存行を削除しない() {
    // mount_a 内のサブディレクトリに read 権限が無い状態で rebuild を実行。
    // 走査エラー時は stale 行の削除をスキップし、既存行を保護する。
    let env = MultiMountEnv::new_with_initial_scan();
    let restricted = env.root_a.join("sub_a/restricted_dir");
    fs::create_dir_all(&restricted).unwrap();
    fs::write(restricted.join("deep.mp4"), b"deep").unwrap();

    // 追加エントリを登録
    env.indexer
        .incremental_scan(
            &env.root_a,
            &env.ps_a(),
            "aaaaaaaaaaaaaaaa",
            2,
            None,
            &|| false,
        )
        .unwrap();
    let before_a = env.count_with_prefix("aaaaaaaaaaaaaaaa/");
    assert!(before_a >= 4);

    // 権限剥奪 → rebuild 実行 → 権限復元
    strip_read_perms(&restricted);
    let _ = env
        .indexer
        .rebuild(&env.root_a, &env.ps_a(), "aaaaaaaaaaaaaaaa", &|| false);
    let after_a = env.count_with_prefix("aaaaaaaaaaaaaaaa/");
    restore_read_perms(&restricted);

    // rebuild で走査エラーが発生した場合、stale 行削除はスキップされるため
    // 既存行は全件残っているべき
    assert_eq!(
        after_a, before_a,
        "rebuild スキャン失敗時でも既存行を保護すべきだが削除された"
    );
}

// --- Phase A: range scan 化の回帰テスト ---
//
// BINARY collation 前提の range scan `relative_path >= ?lo AND < ?hi` が
// `idx_entries_relative_path` を利用し `SCAN entries` にフォールバックしないことを
// EXPLAIN QUERY PLAN で検証する。`SEARCH` を期待するよりも「`SCAN entries` で
// ないこと」を否定形で確認する方が、SQLite planner の細部変化に耐性がある。

/// EXPLAIN QUERY PLAN の `detail` 列を連結して返す
fn explain_query_plan(
    conn: &rusqlite::Connection,
    sql: &str,
    params: &[&dyn rusqlite::types::ToSql],
) -> String {
    let explain_sql = format!("EXPLAIN QUERY PLAN {sql}");
    let mut stmt = conn.prepare(&explain_sql).unwrap();
    let rows = stmt
        .query_map(params, |row| row.get::<_, String>(3))
        .unwrap();
    rows.map(Result::unwrap).collect::<Vec<_>>().join("\n")
}

/// 指定 SQL のクエリプランに `SCAN entries` (非インデックス走査) が含まれないこと。
/// `entries` 以外のテーブル (例: `entries_fts`) の SCAN は許可する。
fn assert_not_scan_entries(plan: &str, context: &str) {
    // "SCAN entries" は "SCAN entries_fts" とは別で、完全一致 or 後続空白のみ許容
    let lower = plan.to_lowercase();
    let has_entries_scan = lower.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.contains("scan entries") && !trimmed.contains("scan entries_fts")
    });
    assert!(
        !has_entries_scan,
        "{context}: クエリプランに `SCAN entries` が含まれている (index 未利用)\nplan:\n{plan}"
    );
}

#[test]
fn mount_scope_rangeのsqlはscan_entriesにならない() {
    // load_existing_entries / load_dir_mtimes / delete_unseen が使う 3 種の SQL
    // について、BETWEEN による range scan が index を利用することを検証する。
    let (indexer, _tmp) = setup_indexer();
    let conn = indexer.connect().unwrap();
    let (lo, hi) = crate::services::path_keys::mount_scope_range("aaaaaaaaaaaaaaaa").unwrap();

    // load_existing_entries 相当
    let plan = explain_query_plan(
        &conn,
        "SELECT relative_path, mtime_ns FROM entries \
         WHERE relative_path >= ?1 AND relative_path < ?2",
        &[&lo, &hi],
    );
    assert_not_scan_entries(&plan, "load_existing_entries");

    // load_dir_mtimes 相当
    let plan = explain_query_plan(
        &conn,
        "SELECT relative_path, mtime_ns FROM entries \
         WHERE kind = 'directory' \
           AND relative_path >= ?1 AND relative_path < ?2",
        &[&lo, &hi],
    );
    assert_not_scan_entries(&plan, "load_dir_mtimes");

    // delete_unseen 相当（seen_paths 一時テーブルが必要）
    conn.execute_batch("CREATE TEMP TABLE IF NOT EXISTS seen_paths(path TEXT PRIMARY KEY);")
        .unwrap();
    let plan = explain_query_plan(
        &conn,
        "DELETE FROM entries \
         WHERE relative_path >= ?1 AND relative_path < ?2 \
           AND relative_path NOT IN (SELECT path FROM seen_paths)",
        &[&lo, &hi],
    );
    assert_not_scan_entries(&plan, "delete_unseen");
}

#[test]
fn search_combined_scope_range_ftsなしケースでscan_entriesにならない() {
    // 2 文字以下のトークン (LIKE フォールバック) + scope_range のみのケース。
    // 2 文字 LIKE `%t%` 自体は両端 wildcard で index 利用不可 (正当) だが、
    // scope_range の絞り込みが BETWEEN で index を使えること自体は大事。
    // planner が scope_range を先に使う場合は `SCAN entries` にならない。
    let (indexer, _tmp) = setup_indexer();
    let conn = indexer.connect().unwrap();
    let (lo, hi) = crate::services::path_keys::prefix_scope_range("mount1");

    // search_combined の FTS なしパスと同じ SQL 形状
    let sql = "SELECT relative_path, name, kind, size_bytes FROM entries \
               WHERE 1=1 \
                 AND (name LIKE ?1 ESCAPE '\\' OR relative_path LIKE ?1 ESCAPE '\\') \
                 AND relative_path >= ?2 AND relative_path < ?3 \
               LIMIT ?4 OFFSET ?5";
    let plan = explain_query_plan(&conn, sql, &[&"%ab%", &lo, &hi, &10_i64, &0_i64]);
    // 注: planner が LIKE を優先評価する場合は SCAN になり得る。
    //     その場合はこのテストが失敗するので、SQL 側の条件順序を調整する契機になる。
    assert_not_scan_entries(&plan, "search_combined FTS なし");
}

#[test]
fn search_combined_scope_range_ftsありケースはentries_fts経由で動作する() {
    // FTS5 MATCH + scope_range の組み合わせ。
    // entries_fts は `SCAN entries_fts` になるが、これは FTS 仮想テーブルの
    // 通常挙動で、`SCAN entries` (本体テーブル) ではない。
    let (indexer, _tmp) = setup_indexer();
    let conn = indexer.connect().unwrap();
    let (lo, hi) = crate::services::path_keys::prefix_scope_range("mount1");

    let sql = "SELECT e.relative_path, e.name, e.kind, e.size_bytes \
               FROM entries_fts f \
               JOIN entries e ON e.id = f.rowid \
               WHERE entries_fts MATCH ?1 \
                 AND e.relative_path >= ?2 AND e.relative_path < ?3 \
               LIMIT ?4 OFFSET ?5";
    let plan = explain_query_plan(&conn, sql, &[&"\"sunset\"", &lo, &hi, &10_i64, &0_i64]);
    // entries_fts 仮想テーブルの SCAN は許容、entries 本体の SCAN は NG
    assert_not_scan_entries(&plan, "search_combined FTS あり");
}

#[test]
fn prefix_scope_rangeはネスト_prefixで正しい境界を返す() {
    let (lo, hi) = crate::services::path_keys::prefix_scope_range("mount1/photos");
    assert_eq!(lo, "mount1/photos/");
    assert_eq!(hi, "mount1/photos0");

    // 実 entries を入れて range 内外の分別を確認
    let (indexer, _tmp) = setup_indexer();
    indexer
        .add_entry(&make_entry("mount1/photos/a.jpg", "a.jpg", "image"))
        .unwrap();
    indexer
        .add_entry(&make_entry("mount1/photos/z.jpg", "z.jpg", "image"))
        .unwrap();
    // 境界外: mount1/photos_backup/... は `mount1/photos0` より大きいので除外される
    indexer
        .add_entry(&make_entry("mount1/photos_backup/x.jpg", "x.jpg", "image"))
        .unwrap();

    let conn = indexer.connect().unwrap();
    let rows: Vec<String> = conn
        .prepare(
            "SELECT relative_path FROM entries \
                  WHERE relative_path >= ?1 AND relative_path < ?2",
        )
        .unwrap()
        .query_map([&lo, &hi], |row| row.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|p| p.starts_with("mount1/photos/")));
}

#[test]
fn prefix_scope_rangeはliteral_percent文字を含むprefixで安全() {
    let (lo, hi) = crate::services::path_keys::prefix_scope_range("mount/dir_100%");
    assert_eq!(lo, "mount/dir_100%/");
    assert_eq!(hi, "mount/dir_100%0");

    // LIKE ではないので `%` は literal として扱われる (escape 不要)
    let (indexer, _tmp) = setup_indexer();
    indexer
        .add_entry(&make_entry("mount/dir_100%/img.jpg", "img.jpg", "image"))
        .unwrap();
    indexer
        .add_entry(&make_entry("mount/dir_200/img.jpg", "img.jpg", "image"))
        .unwrap();

    let conn = indexer.connect().unwrap();
    let rows: Vec<String> = conn
        .prepare(
            "SELECT relative_path FROM entries \
                  WHERE relative_path >= ?1 AND relative_path < ?2",
        )
        .unwrap()
        .query_map([&lo, &hi], |row| row.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows, vec!["mount/dir_100%/img.jpg".to_string()]);
}

#[test]
fn mount_scope_rangeはinvariant違反でrejectする() {
    use crate::services::path_keys::mount_scope_range;
    // 長さ違反
    assert!(mount_scope_range("").is_err());
    assert!(mount_scope_range("short").is_err());
    assert!(mount_scope_range("toolong_mount_id_17chars").is_err());
    // 非 hex 文字
    assert!(mount_scope_range("mount_with_under").is_err()); // 16 chars but contains '_' and uppercase
    assert!(mount_scope_range("ABCDEF0123456789").is_err()); // uppercase hex は reject
    // 正常系: lowercase hex 16 桁
    let (lo, hi) = mount_scope_range("0123456789abcdef").unwrap();
    assert_eq!(lo, "0123456789abcdef/");
    assert_eq!(hi, "0123456789abcdef0");
}

// --- Phase B: load_stored_mount_ids / delete_mount_entries の回帰テスト ---

const MOUNT_HEX_A: &str = "aaaaaaaaaaaaaaaa";
const MOUNT_HEX_B: &str = "bbbbbbbbbbbbbbbb";

#[test]
fn load_stored_mount_idsはfingerprint未保存で空vecを返す() {
    let (indexer, _tmp) = setup_indexer();
    let ids = indexer.load_stored_mount_ids().unwrap();
    assert!(ids.is_empty());
}

#[test]
fn load_stored_mount_idsはソート済み重複排除されたvecを返す() {
    let (indexer, _tmp) = setup_indexer();
    // build_fingerprint は sort_unstable + join なのでここで重複 + 逆順を混ぜる
    indexer
        .save_mount_fingerprint(&[MOUNT_HEX_B, MOUNT_HEX_A, MOUNT_HEX_A])
        .unwrap();
    let ids = indexer.load_stored_mount_ids().unwrap();
    // BTreeSet でソート + 重複排除
    assert_eq!(ids, vec![MOUNT_HEX_A.to_string(), MOUNT_HEX_B.to_string()]);
}

#[test]
fn 破損fingerprint行はall_or_nothingで空vecを返す() {
    // codex v2 Warning 反映: 1 件でも不正な token があれば全体 reject
    for broken in [
        "short",                    // 短い
        "toolong_mount_id_17chars", // 長い
        "ABCDEF0123456789",         // uppercase hex (lowercase 限定)
        "aaaaaaaa_bbbbbbb",         // 非 hex (_) 含む 16 chars
        "0123456789abcdef,broken",  // 片方が非 hex
    ] {
        let (indexer, _tmp) = setup_indexer();
        // save_mount_fingerprint は build_fingerprint でソート・結合するだけで
        // invariant 検証しないため、壊れた値を直接 schema_meta に書く
        let conn = indexer.connect().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO schema_meta (key, value) \
             VALUES ('mount_fingerprint', ?1)",
            rusqlite::params![broken],
        )
        .unwrap();
        let ids = indexer.load_stored_mount_ids().unwrap();
        assert!(ids.is_empty(), "broken={broken:?} で空 Vec を期待");
    }
}

#[test]
fn delete_mount_entriesは自マウント配下のみ削除する() {
    let (indexer, _tmp) = setup_indexer();
    indexer
        .add_entry(&make_entry(
            &format!("{MOUNT_HEX_A}/file1.jpg"),
            "file1.jpg",
            "image",
        ))
        .unwrap();
    indexer
        .add_entry(&make_entry(
            &format!("{MOUNT_HEX_A}/sub/file2.mp4"),
            "file2.mp4",
            "video",
        ))
        .unwrap();
    indexer
        .add_entry(&make_entry(
            &format!("{MOUNT_HEX_B}/file3.pdf"),
            "file3.pdf",
            "pdf",
        ))
        .unwrap();

    let deleted = indexer.delete_mount_entries(MOUNT_HEX_A).unwrap();
    assert_eq!(deleted, 2);

    let remaining = indexer.list_entry_paths().unwrap();
    assert_eq!(remaining, vec![format!("{MOUNT_HEX_B}/file3.pdf")]);
}

#[test]
fn delete_mount_entriesはinvariant違反でrejectする() {
    let (indexer, _tmp) = setup_indexer();
    assert!(indexer.delete_mount_entries("").is_err());
    assert!(indexer.delete_mount_entries("short").is_err());
    assert!(indexer.delete_mount_entries("ABCDEF0123456789").is_err());
}
