//! BFS レベル単位の並列ディレクトリ走査
//!
//! rayon の `ThreadPool` で各ディレクトリの `std::fs::read_dir` + `metadata()` を並列化し、
//! WSL2 drvfs 等の高レイテンシ FS でのスキャンを高速化する。
//!
//! - `WalkEntry`: 1 ディレクトリの走査結果 (サブディレクトリ + ファイル)
//! - `WalkError` / `WalkErrorKind`: 走査中に発生した個別エラーの詳細
//! - `WalkReport`: 走査全体のサマリ。真の件数 `total_error_count` とサンプル
//!   `error_samples` を分離し、閾値判定の分母 `observed_entries` を別 field 化
//! - `parallel_walk()`: BFS でディレクトリ階層を並列走査し、コールバックで結果を通知

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// サンプルとして詳細保持するエラーの最大数
///
/// 超過分は `total_error_count` と `error_kind_counts` のみ加算し、
/// 詳細（`PathBuf` + `io::Error`）は破棄する。構造化ログの `sample_paths` は
/// 先頭 3-5 件しか出さないので 64 件保持すれば十分。
pub(crate) const MAX_WALK_ERROR_SAMPLES: usize = 64;

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

/// 走査中に発生したエラーの種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum WalkErrorKind {
    /// `read_dir` 失敗（ディレクトリ列挙不能、配下スキャン不可）
    ReadDir,
    /// `DirEntry` 取得失敗（反復中の特定エントリのみ）
    DirEntry,
    /// `metadata` 失敗（エントリは見えたが stat 不可）
    Metadata,
}

/// 走査中に発生した個別エラーの詳細
///
/// `io::Error` を保持するが、サンプル上限 (`MAX_WALK_ERROR_SAMPLES`) を超過すると
/// 詳細は破棄される。件数のみは `WalkReport.total_error_count` に全件加算される。
#[derive(Debug)]
pub(crate) struct WalkError {
    pub path: PathBuf,
    pub kind: WalkErrorKind,
    pub io_err: std::io::Error,
}

/// 走査全体のサマリ
///
/// - `entry_count`: `entry_callback` に通知した `WalkEntry` の総件数（走査ディレ
///   クトリ数に相当）
/// - `observed_entries`: 削除判定 (`should_skip_delete`) の分母となる「実試行数」。
///   `visited_dirs + visible_children + 各種失敗件数` の合算
/// - `total_error_count`: 真の総エラー件数。`error_samples.len()` と独立に
///   `MAX_WALK_ERROR_SAMPLES` を超えても減らない
/// - `error_kind_counts`: 種別ごとの件数（観測・監視向け、軽量）
/// - `error_samples`: 先頭 `MAX_WALK_ERROR_SAMPLES` 件のみ詳細保持
/// - `cancelled`: 協調キャンセルで中断したか。`true` の場合 `entry_count` は partial
///   で、呼び出し側は readiness マークや削除判定を skip すべき
///
/// 削除判定側は `error_count()` で真値、`observed_entries` で分母を参照する。
#[derive(Debug, Default)]
pub(crate) struct WalkReport {
    pub entry_count: usize,
    pub observed_entries: usize,
    pub total_error_count: usize,
    pub error_kind_counts: BTreeMap<WalkErrorKind, usize>,
    pub error_samples: Vec<WalkError>,
    pub cancelled: bool,
}

impl WalkReport {
    /// 真の総エラー件数を返す（`error_samples.len()` とは独立）
    pub(crate) fn error_count(&self) -> usize {
        self.total_error_count
    }
}

/// スレッドローカルの走査バッチ（`scan_one` 単位で構築、`parallel_walk` でマージ）
#[derive(Debug, Default)]
struct LocalErrorBatch {
    /// `MAX_WALK_ERROR_SAMPLES` まで保持するエラー詳細
    samples: Vec<WalkError>,
    /// 真の総エラー件数（サンプル上限とは独立）
    total_error_count: usize,
    /// 種別別件数（サンプル上限を超えても加算）
    kind_counts: BTreeMap<WalkErrorKind, usize>,
    /// このディレクトリ自体の `read_dir` が成功したか（0 or 1）
    visited_dirs: usize,
    /// `metadata` まで成功した子エントリ数（subdir + file）
    visible_children: usize,
}

impl LocalErrorBatch {
    /// エラーを記録する（サンプルは上限まで、件数は無制限に加算）
    fn record_error(&mut self, path: PathBuf, kind: WalkErrorKind, io_err: std::io::Error) {
        self.total_error_count += 1;
        *self.kind_counts.entry(kind).or_insert(0) += 1;
        if self.samples.len() < MAX_WALK_ERROR_SAMPLES {
            self.samples.push(WalkError { path, kind, io_err });
        }
    }

    /// `observed_entries` に寄与する試行数
    fn observed(&self) -> usize {
        self.visited_dirs + self.visible_children + self.total_error_count
    }
}

/// 1 ディレクトリを `read_dir` で走査し、`(WalkEntry, LocalErrorBatch)` を返す
///
/// - `path_validator` が指定されている場合、stat 前にパスを検証する
/// - 隠しファイル/ディレクトリ (先頭 '.') は `skip_hidden=true` でスキップ
/// - `cancelled()` を `read_dir` 前と各子エントリ処理の前で check、true なら
///   早期 return して空の batch を返す（呼び出し側は level 境界でもキャンセルを検知する）
/// - スレッドローカルなバッチを返すので lock-free。呼び出し側がレベル単位で
///   マージする
fn scan_one(
    dir_path: &Path,
    skip_hidden: bool,
    path_validator: PathValidator<'_>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> (WalkEntry, LocalErrorBatch) {
    let mut subdirs = Vec::new();
    let mut files = Vec::new();
    let mut batch = LocalErrorBatch::default();

    if cancelled() {
        let entry = WalkEntry {
            path: dir_path.to_path_buf(),
            mtime_ns: 0,
            subdirs,
            files,
        };
        return (entry, batch);
    }

    match std::fs::read_dir(dir_path) {
        Ok(entries) => {
            batch.visited_dirs = 1;
            for entry_result in entries {
                if cancelled() {
                    break;
                }
                let entry = match entry_result {
                    Ok(e) => e,
                    Err(e) => {
                        // 個々の DirEntry 取得失敗 (EACCES 等) はカウントして次へ
                        tracing::warn!("DirEntry 取得失敗: {} (path: {})", e, dir_path.display());
                        batch.record_error(dir_path.to_path_buf(), WalkErrorKind::DirEntry, e);
                        continue;
                    }
                };
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

                let meta = match entry.metadata() {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::warn!("metadata 失敗: {} (path: {})", e, child_path.display());
                        batch.record_error(child_path, WalkErrorKind::Metadata, e);
                        continue;
                    }
                };

                #[allow(clippy::cast_possible_wrap, reason = "mtime_ns は i64 に収まる")]
                let mtime_ns = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos() as i64);

                if meta.is_dir() {
                    subdirs.push((name, mtime_ns));
                    batch.visible_children += 1;
                } else if meta.is_file() {
                    #[allow(clippy::cast_possible_wrap, reason = "ファイルサイズは i64 に収まる")]
                    let size = meta.len() as i64;
                    files.push((name, size, mtime_ns));
                    batch.visible_children += 1;
                }
            }
        }
        Err(e) => {
            // ディレクトリ列挙失敗 (EACCES / ENOENT 等) は致命的: 配下をスキャンできず
            // 空の WalkEntry を返すと delete_unseen に誤って削除される危険がある
            tracing::warn!("read_dir 失敗: {} (path: {})", e, dir_path.display());
            batch.record_error(dir_path.to_path_buf(), WalkErrorKind::ReadDir, e);
        }
    }

    // ディレクトリ自体の mtime を取得
    #[allow(clippy::cast_possible_wrap, reason = "mtime_ns は i64 に収まる")]
    let dir_mtime_ns = std::fs::metadata(dir_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as i64);

    let entry = WalkEntry {
        path: dir_path.to_path_buf(),
        mtime_ns: dir_mtime_ns,
        subdirs,
        files,
    };
    (entry, batch)
}

/// BFS レベル単位でディレクトリを並列走査する
///
/// - 各レベルのディレクトリを rayon で並列に `read_dir` + stat
/// - `path_validator`: stat 前にパスを検証するコールバック (`PathSecurity` 連携用)
/// - `dir_filter`: サブディレクトリを次レベルに追加するか判定するコールバック
///   (`false` を返すと枝刈り — incremental scan の mtime ベース最適化用)
/// - `entry_callback`: 走査結果を 1 ディレクトリずつ通知するコールバック
/// - 戻り値 `WalkReport`: 通知件数 + エラー詳細サマリ + 観測試行数。呼び出し側
///   はエラー発生時に「見えなかった行を DELETE しない」判断に使える。
#[allow(
    clippy::too_many_arguments,
    reason = "BFS 走査に必要な 4 種のコールバック + cancelled をまとめて渡す都合で閾値超過"
)]
pub(crate) fn parallel_walk(
    root: &Path,
    workers: usize,
    skip_hidden: bool,
    path_validator: PathValidator<'_>,
    dir_filter: &mut dyn FnMut(&Path, i64) -> bool,
    entry_callback: &mut dyn FnMut(WalkEntry),
    cancelled: &(dyn Fn() -> bool + Sync),
) -> WalkReport {
    let mut report = WalkReport::default();

    let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build();
    let Ok(pool) = pool else {
        tracing::error!("rayon ThreadPool の構築に失敗");
        // ThreadPool 構築失敗は走査不能なのでエラー 1 件として返す
        report.total_error_count = 1;
        *report
            .error_kind_counts
            .entry(WalkErrorKind::ReadDir)
            .or_insert(0) += 1;
        return report;
    };

    let mut current_level = vec![root.to_path_buf()];

    while !current_level.is_empty() {
        // level 境界でキャンセル確認。partial WalkReport を返すため
        // 呼び出し側が `cancelled = true` を見て readiness マークを skip する
        if cancelled() {
            report.cancelled = true;
            return report;
        }

        // 現在のレベルのディレクトリを並列スキャン（スレッドローカルバッチで lock-free）
        let results: Vec<(WalkEntry, LocalErrorBatch)> = pool.install(|| {
            use rayon::prelude::*;
            current_level
                .par_iter()
                .map(|d| scan_one(d, skip_hidden, path_validator, cancelled))
                .collect()
        });

        let mut next_level = Vec::new();

        for (entry, batch) in results {
            // サブディレクトリを次のレベルに追加 (dir_filter で枝刈り)
            for (name, mtime_ns) in &entry.subdirs {
                let subdir_path = entry.path.join(name);
                if dir_filter(&subdir_path, *mtime_ns) {
                    next_level.push(subdir_path);
                }
            }

            // batch を report にマージ
            merge_batch(&mut report, batch);

            entry_callback(entry);
            report.entry_count += 1;
        }

        current_level = next_level;
    }

    report
}

/// `LocalErrorBatch` を `WalkReport` にマージする
///
/// - `observed_entries` は各バッチの `observed()` 合計
/// - `error_samples` は先頭 `MAX_WALK_ERROR_SAMPLES` 件のみ保持、超過分は破棄
/// - `total_error_count` / `error_kind_counts` は件数ベースで全加算
fn merge_batch(report: &mut WalkReport, batch: LocalErrorBatch) {
    report.observed_entries += batch.observed();
    report.total_error_count += batch.total_error_count;
    for (kind, count) in batch.kind_counts {
        *report.error_kind_counts.entry(kind).or_insert(0) += count;
    }
    for sample in batch.samples {
        if report.error_samples.len() < MAX_WALK_ERROR_SAMPLES {
            report.error_samples.push(sample);
        } else {
            break;
        }
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

        let report = parallel_walk(
            &tree.root,
            2,
            true,
            None,
            &mut |_, _| true,
            &mut |e| {
                entries.push(e);
            },
            &|| false,
        );

        // ルート + sub1 + sub2 = 3 ディレクトリ (.hidden はスキップ)
        assert_eq!(entries.len(), 3);
        assert_eq!(report.entry_count, 3);
        assert_eq!(report.error_count(), 0);
        assert_eq!(report.total_error_count, 0);
        assert!(report.error_samples.is_empty());

        // ルートエントリのファイル・サブディレクトリを検証
        let root_entry = entries.iter().find(|e| e.path == tree.root).unwrap();
        assert_eq!(root_entry.subdirs.len(), 2); // sub1, sub2 (.hidden 除外)
        assert_eq!(root_entry.files.len(), 2); // file1.txt, file2.jpg
    }

    #[test]
    fn 隠しファイルがスキップされる() {
        let tree = TestTree::new();
        let mut entries = Vec::new();

        parallel_walk(
            &tree.root,
            2,
            true,
            None,
            &mut |_, _| true,
            &mut |e| {
                entries.push(e);
            },
            &|| false,
        );

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

        parallel_walk(
            &tree.root,
            2,
            false,
            None,
            &mut |_, _| true,
            &mut |e| {
                entries.push(e);
            },
            &|| false,
        );

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
            &|| false,
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
            &|| false,
        );

        // sub2 は枝刈りされて走査されない → root + sub1 = 2
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn ファイルメタデータが正しく取得される() {
        let tree = TestTree::new();
        let mut entries = Vec::new();

        parallel_walk(
            &tree.root,
            2,
            true,
            None,
            &mut |_, _| true,
            &mut |e| {
                entries.push(e);
            },
            &|| false,
        );

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

    #[test]
    fn 存在しないルートではエラーがカウントされる() {
        let nonexistent = PathBuf::from("/nonexistent/path/xyz");
        let mut entries = Vec::new();

        let report = parallel_walk(
            &nonexistent,
            2,
            true,
            None,
            &mut |_, _| true,
            &mut |e| {
                entries.push(e);
            },
            &|| false,
        );

        // ルート 1 件は WalkEntry として返る (空の subdirs/files + mtime_ns=0)
        assert_eq!(report.entry_count, 1);
        // read_dir 失敗で error_count が加算される
        assert!(
            report.error_count() >= 1,
            "read_dir 失敗が報告されるべき, got {}",
            report.error_count()
        );
        // 種別別件数も ReadDir で加算される
        assert_eq!(
            report
                .error_kind_counts
                .get(&WalkErrorKind::ReadDir)
                .copied(),
            Some(1)
        );
        // サンプルには path / kind / io_err が保持される
        assert_eq!(report.error_samples.len(), 1);
        assert_eq!(report.error_samples[0].kind, WalkErrorKind::ReadDir);
        assert_eq!(report.error_samples[0].path, nonexistent);
    }

    #[test]
    fn observed_entriesは試行数の合算である() {
        // 既知構造: root (dir) + 2 subdirs + 2 files の children
        //   root の visited=1, children=4 (sub1, sub2, file1, file2)
        //   sub1: visited=1, children=1 (inner.jpg)
        //   sub2: visited=1, children=1 (clip.mp4)
        // 合計 observed = 3 + 6 = 9（.hidden はスキップで metadata を呼ばない）
        let tree = TestTree::new();
        let mut entries = Vec::new();

        let report = parallel_walk(
            &tree.root,
            2,
            true,
            None,
            &mut |_, _| true,
            &mut |e| {
                entries.push(e);
            },
            &|| false,
        );

        // .hidden はスキップされるが、metadata が呼ばれる前に名前判定で弾かれるため
        // visible_children には含まれない
        assert_eq!(report.observed_entries, 9);
        assert_eq!(report.error_count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn max_walk_error_samplesを超えたエラーは詳細が破棄され件数のみ加算される() {
        use std::os::unix::fs::PermissionsExt;
        // MAX_WALK_ERROR_SAMPLES (64) を超える件数のエラーを発生させる。
        // read 権限を剥奪したディレクトリを大量作成 → read_dir 時に全部失敗。
        let dir = TempDir::new().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let n = MAX_WALK_ERROR_SAMPLES + 30; // 94 件
        for i in 0..n {
            let sub = root.join(format!("denied_{i:03}"));
            fs::create_dir(&sub).unwrap();
            // 権限剥奪で read_dir 失敗を誘発
            let mut perms = fs::metadata(&sub).unwrap().permissions();
            perms.set_mode(0o000);
            fs::set_permissions(&sub, perms).unwrap();
        }

        let report = parallel_walk(&root, 2, true, None, &mut |_, _| true, &mut |_| {}, &|| {
            false
        });

        // 権限復元（TempDir drop のため）
        for i in 0..n {
            let sub = root.join(format!("denied_{i:03}"));
            let mut perms = fs::metadata(&sub).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&sub, perms).unwrap();
        }

        // 真の件数は全件加算
        assert_eq!(report.total_error_count, n);
        assert_eq!(report.error_count(), n);
        // サンプルは上限まで
        assert_eq!(report.error_samples.len(), MAX_WALK_ERROR_SAMPLES);
        // 種別別件数も全件加算
        assert_eq!(
            report
                .error_kind_counts
                .get(&WalkErrorKind::ReadDir)
                .copied(),
            Some(n)
        );
    }

    #[test]
    fn cancel後にlevel境界で早期returnしcancelled_trueを返す() {
        // cancelled 初回から true → root の scan もスキップされ、level ループ先頭で bail
        let tree = TestTree::new();
        let mut entries = Vec::new();

        let report = parallel_walk(
            &tree.root,
            2,
            true,
            None,
            &mut |_, _| true,
            &mut |e| entries.push(e),
            &|| true, // 常に cancel
        );

        assert!(report.cancelled, "cancelled フラグが true であること");
        assert_eq!(entries.len(), 0, "level 先頭で bail するので entry 0 件");
        assert_eq!(report.entry_count, 0);
    }

    #[test]
    fn cancelされない通常経路ではcancelled_falseが返る() {
        let tree = TestTree::new();
        let mut entries = Vec::new();

        let report = parallel_walk(
            &tree.root,
            2,
            true,
            None,
            &mut |_, _| true,
            &mut |e| entries.push(e),
            &|| false,
        );

        assert!(!report.cancelled);
        assert_eq!(report.entry_count, 3);
    }
}
