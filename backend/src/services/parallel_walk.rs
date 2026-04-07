//! BFS レベル単位の並列ディレクトリ走査
//!
//! rayon の `ThreadPool` で各ディレクトリの `std::fs::read_dir` + `metadata()` を並列化し、
//! WSL2 drvfs 等の高レイテンシ FS でのスキャンを高速化する。
//!
//! - `WalkEntry`: 1 ディレクトリの走査結果 (サブディレクトリ + ファイル)
//! - `parallel_walk()`: BFS でディレクトリ階層を並列走査し、コールバックで結果を通知

use std::path::{Path, PathBuf};

/// パス検証コールバック型 (`PathSecurity` 連携用)
type PathValidator<'a> = Option<&'a (dyn Fn(&Path) -> bool + Sync)>;

/// 1 ディレクトリの走査結果
#[derive(Debug)]
pub(crate) struct WalkEntry {
    /// ディレクトリパス
    pub path: PathBuf,
    /// ディレクトリ自体の `mtime` (ナノ秒)
    pub mtime_ns: i64,
    /// サブディレクトリ一覧: (名前, `mtime_ns`)
    pub subdirs: Vec<(String, i64)>,
    /// ファイル一覧: (名前, `size_bytes`, `mtime_ns`)
    pub files: Vec<(String, i64, i64)>,
}

/// 1 ディレクトリを `read_dir` で走査し、stat 付きの結果を返す
///
/// - `path_validator` が指定されている場合、stat 前にパスを検証する
/// - 隠しファイル/ディレクトリ (先頭 '.') は `skip_hidden=true` でスキップ
fn scan_one(dir_path: &Path, skip_hidden: bool, path_validator: PathValidator<'_>) -> WalkEntry {
    let mut subdirs = Vec::new();
    let mut files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir_path) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if skip_hidden && name.starts_with('.') {
                continue;
            }

            let child_path = dir_path.join(&name);

            // セキュリティ検証 (stat 前)
            if let Some(validator) = path_validator {
                if !validator(&child_path) {
                    continue;
                }
            }

            let Ok(meta) = entry.metadata() else {
                continue;
            };

            #[allow(clippy::cast_possible_wrap, reason = "mtime_ns は i64 に収まる")]
            let mtime_ns = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos() as i64);

            if meta.is_dir() {
                subdirs.push((name, mtime_ns));
            } else if meta.is_file() {
                #[allow(clippy::cast_possible_wrap, reason = "ファイルサイズは i64 に収まる")]
                let size = meta.len() as i64;
                files.push((name, size, mtime_ns));
            }
        }
    }

    // ディレクトリ自体の mtime を取得
    #[allow(clippy::cast_possible_wrap, reason = "mtime_ns は i64 に収まる")]
    let dir_mtime_ns = std::fs::metadata(dir_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as i64);

    WalkEntry {
        path: dir_path.to_path_buf(),
        mtime_ns: dir_mtime_ns,
        subdirs,
        files,
    }
}

/// BFS レベル単位でディレクトリを並列走査する
///
/// - 各レベルのディレクトリを rayon で並列に `read_dir` + stat
/// - `path_validator`: stat 前にパスを検証するコールバック (`PathSecurity` 連携用)
/// - `dir_filter`: サブディレクトリを次レベルに追加するか判定するコールバック
///   (`false` を返すと枝刈り — incremental scan の mtime ベース最適化用)
/// - `entry_callback`: 走査結果を 1 ディレクトリずつ通知するコールバック
pub(crate) fn parallel_walk(
    root: &Path,
    workers: usize,
    skip_hidden: bool,
    path_validator: PathValidator<'_>,
    dir_filter: &mut dyn FnMut(&Path, i64) -> bool,
    entry_callback: &mut dyn FnMut(WalkEntry),
) {
    let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build();
    let Ok(pool) = pool else {
        tracing::error!("rayon ThreadPool の構築に失敗");
        return;
    };

    let mut current_level = vec![root.to_path_buf()];

    while !current_level.is_empty() {
        // 現在のレベルのディレクトリを並列スキャン
        let results: Vec<WalkEntry> = pool.install(|| {
            use rayon::prelude::*;
            current_level
                .par_iter()
                .map(|d| scan_one(d, skip_hidden, path_validator))
                .collect()
        });

        let mut next_level = Vec::new();

        for entry in results {
            // サブディレクトリを次のレベルに追加 (dir_filter で枝刈り)
            for (name, mtime_ns) in &entry.subdirs {
                let subdir_path = entry.path.join(name);
                if dir_filter(&subdir_path, *mtime_ns) {
                    next_level.push(subdir_path);
                }
            }
            entry_callback(entry);
        }

        current_level = next_level;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    struct TestTree {
        #[allow(dead_code, reason = "TempDir のドロップでディレクトリを保持")]
        dir: TempDir,
        root: PathBuf,
    }

    impl TestTree {
        fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let root = fs::canonicalize(dir.path()).unwrap();
            // ディレクトリ構造:
            //   root/
            //     sub1/
            //       inner.jpg
            //     sub2/
            //       clip.mp4
            //     .hidden/
            //       secret.txt
            //     file1.txt
            //     file2.jpg
            fs::create_dir_all(root.join("sub1")).unwrap();
            fs::create_dir_all(root.join("sub2")).unwrap();
            fs::create_dir_all(root.join(".hidden")).unwrap();
            fs::write(root.join("sub1/inner.jpg"), b"img").unwrap();
            fs::write(root.join("sub2/clip.mp4"), b"video").unwrap();
            fs::write(root.join(".hidden/secret.txt"), b"secret").unwrap();
            fs::write(root.join("file1.txt"), b"hello").unwrap();
            fs::write(root.join("file2.jpg"), b"img data").unwrap();
            Self { dir, root }
        }
    }

    #[test]
    fn 基本走査でルートとサブディレクトリが返る() {
        let tree = TestTree::new();
        let mut entries = Vec::new();

        parallel_walk(&tree.root, 2, true, None, &mut |_, _| true, &mut |e| {
            entries.push(e);
        });

        // ルート + sub1 + sub2 = 3 ディレクトリ (.hidden はスキップ)
        assert_eq!(entries.len(), 3);

        // ルートエントリのファイル・サブディレクトリを検証
        let root_entry = entries.iter().find(|e| e.path == tree.root).unwrap();
        assert_eq!(root_entry.subdirs.len(), 2); // sub1, sub2 (.hidden 除外)
        assert_eq!(root_entry.files.len(), 2); // file1.txt, file2.jpg
    }

    #[test]
    fn 隠しファイルがスキップされる() {
        let tree = TestTree::new();
        let mut entries = Vec::new();

        parallel_walk(&tree.root, 2, true, None, &mut |_, _| true, &mut |e| {
            entries.push(e);
        });

        // .hidden ディレクトリが子エントリとして含まれない (ルート自体は除外)
        let root_entry = entries.iter().find(|e| e.path == tree.root).unwrap();
        assert!(
            !root_entry
                .subdirs
                .iter()
                .any(|(name, _)| name.starts_with('.')),
            "隠しサブディレクトリが含まれている"
        );
        // .hidden 配下は走査されない
        assert!(!entries.iter().any(|e| e.path == tree.root.join(".hidden")));
    }

    #[test]
    fn skip_hidden_falseで隠しファイルも走査される() {
        let tree = TestTree::new();
        let mut entries = Vec::new();

        parallel_walk(&tree.root, 2, false, None, &mut |_, _| true, &mut |e| {
            entries.push(e);
        });

        // .hidden ディレクトリも含まれる → root + sub1 + sub2 + .hidden = 4
        assert_eq!(entries.len(), 4);
    }

    #[test]
    fn path_validatorで特定パスが除外される() {
        let tree = TestTree::new();
        let mut entries = Vec::new();
        let sub1_path = tree.root.join("sub1");

        // sub1 を拒否する validator
        let validator = move |path: &Path| !path.starts_with(&sub1_path);

        parallel_walk(
            &tree.root,
            2,
            true,
            Some(&validator),
            &mut |_, _| true,
            &mut |e| entries.push(e),
        );

        // sub1 がサブディレクトリとして認識されない → root + sub2 = 2
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn dir_filterで枝刈りされる() {
        let tree = TestTree::new();
        let mut entries = Vec::new();
        let sub2_path = tree.root.join("sub2");

        // sub2 を枝刈り
        parallel_walk(
            &tree.root,
            2,
            true,
            None,
            &mut |path, _| !path.starts_with(&sub2_path),
            &mut |e| entries.push(e),
        );

        // sub2 は枝刈りされて走査されない → root + sub1 = 2
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn ファイルメタデータが正しく取得される() {
        let tree = TestTree::new();
        let mut entries = Vec::new();

        parallel_walk(&tree.root, 2, true, None, &mut |_, _| true, &mut |e| {
            entries.push(e);
        });

        let sub1 = entries
            .iter()
            .find(|e| e.path == tree.root.join("sub1"))
            .unwrap();
        assert_eq!(sub1.files.len(), 1);
        let (name, size, mtime_ns) = &sub1.files[0];
        assert_eq!(name, "inner.jpg");
        assert_eq!(*size, 3); // b"img" = 3 bytes
        assert!(*mtime_ns > 0);
    }
}
