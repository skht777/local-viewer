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

    let ids = vec!["mount_a", "mount_b"];
    indexer.save_mount_fingerprint(&ids).unwrap();

    // 同じ ID リストで検証 → true
    assert!(indexer.check_mount_fingerprint(&ids).unwrap());

    // 異なる ID リストで検証 → false
    let different = vec!["mount_c"];
    assert!(!indexer.check_mount_fingerprint(&different).unwrap());

    // 順序を入れ替えても一致する (ソート済みフィンガープリント)
    let reversed = vec!["mount_b", "mount_a"];
    assert!(indexer.check_mount_fingerprint(&reversed).unwrap());
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

    let count = env
        .indexer
        .scan_directory(&env.root, &ps, "test_mount", 2, None)
        .unwrap();

    // sub1 (directory) + movie.mp4 (video) + doc.pdf (pdf) = 3
    // image.jpg は画像なのでインデックス対象外
    assert_eq!(count, 3);
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
        .scan_directory(&env.root, &ps, "test_mount", 2, None)
        .unwrap();
    assert_eq!(env.indexer.entry_count().unwrap(), 3);

    // ファイルを追加して sub1 の mtime を変える
    fs::write(env.root.join("sub1/extra.mp4"), b"extra").unwrap();

    let (added, updated, deleted) = env
        .indexer
        .incremental_scan(&env.root, &ps, "test_mount", 2, None)
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
    indexer.scan_directory(&root, &ps, "m", 2, None).unwrap();
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
    let (added, _updated, deleted) = indexer.incremental_scan(&root, &ps, "m", 2, None).unwrap();

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
        .scan_directory(&env.root, &ps, "test_mount", 2, None)
        .unwrap();
    assert_eq!(env.indexer.entry_count().unwrap(), 3);

    // rebuild
    let count = env.indexer.rebuild(&env.root, &ps, "test_mount").unwrap();

    // 同じ件数で再構築される
    assert_eq!(count, 3);
    assert_eq!(env.indexer.entry_count().unwrap(), 3);

    // is_rebuilding は完了後 false
    assert!(!env.indexer.is_rebuilding());
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
