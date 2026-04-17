//! ファイルシステム変更監視
//!
//! `notify` v8 クレートでファイルの作成/削除/移動を検知し、
//! `Indexer` にインクリメンタルに反映する。
//! 1 秒間隔のバッチフラッシュで高頻度イベントをデバウンスする。

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{debug, info, warn};

use crate::services::dir_index::DirIndex;
use crate::services::extensions::{
    ARCHIVE_EXTENSIONS, PDF_EXTENSIONS, VIDEO_EXTENSIONS, classify_for_index, extract_extension,
};
use crate::services::indexer::{IndexEntry, Indexer};
use crate::services::path_security::PathSecurity;

// ---------- エラー型 ----------

/// ファイル監視エラー
#[derive(Debug, thiserror::Error)]
pub(crate) enum FileWatcherError {
    #[error("notify error: {0}")]
    Notify(#[from] notify::Error),
    #[error("{0}")]
    Other(String),
}

// ---------- ファイル監視サービス ----------

/// ファイルシステム変更を監視してインデックスを更新する
///
/// - `notify::RecommendedWatcher` で各マウントルートを再帰監視
/// - pending `HashMap` に path → action を蓄積し、1 秒間隔で flush
/// - flush 時に `Indexer` へ `add_entry` / `remove_entry` を発行
pub(crate) struct FileWatcher {
    indexer: Arc<Indexer>,
    path_security: Arc<PathSecurity>,
    dir_index: Arc<DirIndex>,
    mounts: Vec<(String, PathBuf)>,
    pending: Arc<std::sync::Mutex<HashMap<String, String>>>,
    is_running: AtomicBool,
    /// watcher ハンドル (stop 時に drop)
    watcher: std::sync::Mutex<Option<RecommendedWatcher>>,
    /// flush ワーカーの `JoinHandle` (stop 時に abort)
    flush_handle: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl FileWatcher {
    /// 新しい `FileWatcher` を生成する (未起動状態)
    pub(crate) fn new(
        indexer: Arc<Indexer>,
        path_security: Arc<PathSecurity>,
        dir_index: Arc<DirIndex>,
        mounts: Vec<(String, PathBuf)>,
    ) -> Self {
        Self {
            indexer,
            path_security,
            dir_index,
            mounts,
            pending: Arc::new(std::sync::Mutex::new(HashMap::new())),
            is_running: AtomicBool::new(false),
            watcher: std::sync::Mutex::new(None),
            flush_handle: std::sync::Mutex::new(None),
        }
    }

    /// 監視を開始する (watcher + flush worker を起動)
    pub(crate) fn start(&self) -> Result<(), FileWatcherError> {
        if self.is_running.load(Ordering::Acquire) {
            return Ok(());
        }
        self.is_running.store(true, Ordering::Release);

        // notify イベントコールバック用の参照
        let pending_for_cb = Arc::clone(&self.pending);
        let dir_index_for_cb = Arc::clone(&self.dir_index);
        let mounts_for_cb: Vec<(String, PathBuf)> = self.mounts.clone();

        // watcher 生成 — イベントコールバックで pending に蓄積
        let mut watcher = notify::recommended_watcher(
            move |res: Result<notify::Event, notify::Error>| match res {
                Ok(event) => {
                    // inotify Q_OVERFLOW 等で need_rescan が立つ場合、
                    // DirIndex を stale 化してイベント取りこぼしを補償する
                    if event.need_rescan() {
                        warn!("notify: need_rescan 検知、DirIndex を stale 化");
                        dir_index_for_cb.mark_warm_start();
                    }
                    handle_notify_event(&pending_for_cb, &event, &mounts_for_cb);
                }
                Err(e) => warn!("notify エラー: {e}"),
            },
        )?;

        // 各マウントルートを再帰監視
        for (mount_id, root) in &self.mounts {
            if root.is_dir() {
                watcher.watch(root, RecursiveMode::Recursive)?;
                let label = if mount_id.is_empty() {
                    "default"
                } else {
                    mount_id.as_str()
                };
                info!("FileWatcher: 監視開始 {} ({label})", root.display());
            } else {
                warn!(
                    "FileWatcher: マウントルートが存在しません: {}",
                    root.display()
                );
            }
        }

        // watcher ハンドルを保存
        {
            let mut guard = self
                .watcher
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *guard = Some(watcher);
        }

        // flush ワーカーを起動
        let flush_pending = Arc::clone(&self.pending);
        let flush_indexer = Arc::clone(&self.indexer);
        let flush_path_security = Arc::clone(&self.path_security);
        let flush_dir_index = Arc::clone(&self.dir_index);
        let flush_mounts: Vec<(String, PathBuf)> = self.mounts.clone();

        // flush ワーカーは JoinHandle の abort で停止する方式
        let handle = tokio::spawn(flush_worker_loop(
            flush_pending,
            flush_indexer,
            flush_path_security,
            flush_dir_index,
            flush_mounts,
        ));

        // flush ハンドルを保存
        {
            let mut guard = self
                .flush_handle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *guard = Some(handle);
        }

        Ok(())
    }

    /// 監視を停止する
    pub(crate) fn stop(&self) {
        self.is_running.store(false, Ordering::Release);

        // watcher を drop して OS 監視を解除
        {
            let mut guard = self
                .watcher
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let _ = guard.take();
        }

        // flush ワーカーを abort
        {
            let mut guard = self
                .flush_handle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(handle) = guard.take() {
                handle.abort();
            }
        }

        info!("FileWatcher: 監視停止");
    }

    /// 監視中かどうかを返す
    pub(crate) fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Acquire)
    }
}

// ---------- notify イベントハンドラ ----------

/// notify イベントを pending マップに蓄積する
fn handle_notify_event(
    pending: &std::sync::Mutex<HashMap<String, String>>,
    event: &notify::Event,
    mounts: &[(String, PathBuf)],
) {
    let action = match &event.kind {
        EventKind::Create(CreateKind::File | CreateKind::Folder)
        | EventKind::Modify(ModifyKind::Name(RenameMode::To)) => "add",
        EventKind::Remove(RemoveKind::File | RemoveKind::Folder)
        | EventKind::Modify(ModifyKind::Name(RenameMode::From)) => "remove",
        _ => return,
    };

    for path in &event.paths {
        enqueue(pending, path, action, mounts);
    }
}

/// 対象パスを pending に追加する (隠しファイル・非対象拡張子をスキップ)
fn enqueue(
    pending: &std::sync::Mutex<HashMap<String, String>>,
    path: &Path,
    action: &str,
    mounts: &[(String, PathBuf)],
) {
    // 隠しファイル/ディレクトリをスキップ (full scan の parallel_walk と同じ判定基準)
    if is_hidden_under_mounts(path, mounts) {
        return;
    }

    // ファイルの場合: 拡張子チェック (ディレクトリは常に通過)
    // Remove イベントでは path.is_file() が false になるため、
    // ディレクトリ判定には is_dir() ではなく拡張子の有無で判断
    let Some(file_name) = path.file_name() else {
        return;
    };
    let name = file_name.to_string_lossy();
    let ext = extract_extension(&name).to_lowercase();

    // 拡張子がない → ディレクトリとみなして通過
    // 拡張子がある → インデックス対象かチェック
    if !ext.is_empty() && !is_indexable_extension(&ext) {
        return;
    }

    let key = path.to_string_lossy().into_owned();
    let mut guard = pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.insert(key, action.to_string());
}

// ---------- flush ワーカー ----------

/// 1 秒間隔で pending イベントを取り出し、`Indexer` + `DirIndex` に反映する
async fn flush_worker_loop(
    pending: Arc<std::sync::Mutex<HashMap<String, String>>>,
    indexer: Arc<Indexer>,
    path_security: Arc<PathSecurity>,
    dir_index: Arc<DirIndex>,
    mounts: Vec<(String, PathBuf)>,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;

        // pending を一括取得
        let events = {
            let mut guard = pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            std::mem::take(&mut *guard)
        };

        if events.is_empty() {
            continue;
        }

        debug!("FileWatcher flush: {} 件のイベントを処理", events.len());

        // spawn_blocking 内で同期処理
        let indexer_c = Arc::clone(&indexer);
        let ps_c = Arc::clone(&path_security);
        let di_c = Arc::clone(&dir_index);
        let mounts_c = mounts.clone();

        let _ = tokio::task::spawn_blocking(move || {
            for (path_str, action) in &events {
                process_event(&indexer_c, &ps_c, &di_c, &mounts_c, path_str, action);
            }
        })
        .await;
    }
}

/// 1 件のイベントを処理してインデックスに反映し、DirIndex の親ディレクトリを dirty 化する
fn process_event(
    indexer: &Indexer,
    path_security: &PathSecurity,
    dir_index: &DirIndex,
    mounts: &[(String, PathBuf)],
    path_str: &str,
    action: &str,
) {
    let path = Path::new(path_str);

    // 親ディレクトリの parent_key を計算して dirty 化
    // remove/rename-from では対象パスが既に存在しないため、存在する親ディレクトリから計算
    if let Some(parent) = path.parent() {
        if let Some(parent_key) = compute_relative_path(parent, mounts) {
            dir_index.mark_dir_dirty(&parent_key);
        }
    }

    if action == "remove" {
        // 削除: 相対パスを計算して remove_entry
        if let Some(rel) = compute_relative_path(path, mounts) {
            if let Err(e) = indexer.remove_entry(&rel) {
                debug!("remove_entry 失敗: {e} (path: {path_str})");
            }
        }
        return;
    }

    // action == "add"
    // パスの存在確認
    if !path.exists() {
        return;
    }

    // PathSecurity 検証
    if path_security.validate(path).is_err() {
        return;
    }

    // メタデータ取得
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };

    let Some(file_name) = path.file_name() else {
        return;
    };
    let name = file_name.to_string_lossy().into_owned();
    let is_dir = meta.is_dir();

    // インデックス対象の種別を判定
    let Some(kind) = classify_for_index(&name, is_dir) else {
        return;
    };

    // 相対パスを計算
    let Some(rel) = compute_relative_path(path, mounts) else {
        return;
    };

    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| {
            // ナノ秒を i64 に変換 (u128 → i64)
            i64::try_from(d.as_nanos()).unwrap_or(i64::MAX)
        });

    #[allow(
        clippy::cast_possible_wrap,
        reason = "ファイルサイズが i64::MAX を超えることはない"
    )]
    let size_bytes = if is_dir {
        None
    } else {
        Some(meta.len() as i64)
    };

    let entry = IndexEntry {
        relative_path: rel,
        name,
        kind: kind.to_string(),
        size_bytes,
        mtime_ns,
    };

    if let Err(e) = indexer.add_entry(&entry) {
        debug!("add_entry 失敗: {e} (path: {path_str})");
    }
}

// ---------- ヘルパー関数 ----------

/// パスがマウント配下で隠し要素 (名前が '.' で始まる) を含むか判定する
///
/// - マウントルートからの相対パスを取り、各コンポーネントを検査
/// - いずれかのコンポーネント名が '.' で始まるなら hidden
/// - マウントルート自身の名前は判定対象外（`parallel_walk::scan_one` の BFS 起点が
///   `skip_hidden` の対象外であるのと一致。`/data/.archive` をマウント登録しても配下は走査される）
/// - マウント外パスは fail-safe として hidden 扱い（FileWatcher は通常マウント配下のみ監視）
fn is_hidden_under_mounts(path: &Path, mounts: &[(String, PathBuf)]) -> bool {
    for (_, root) in mounts {
        if let Ok(rel) = path.strip_prefix(root) {
            return rel.components().any(|comp| {
                if let Component::Normal(name) = comp {
                    name.to_string_lossy().starts_with('.')
                } else {
                    false
                }
            });
        }
    }
    true
}

/// 拡張子がインデックス対象 (動画/アーカイブ/PDF) か判定する
///
/// 画像はファイル数が膨大になるため除外 (`classify_for_index` と同じ方針)
fn is_indexable_extension(ext: &str) -> bool {
    VIDEO_EXTENSIONS.contains(&ext)
        || ARCHIVE_EXTENSIONS.contains(&ext)
        || PDF_EXTENSIONS.contains(&ext)
}

/// 絶対パスからマウント相対パスを計算する
///
/// `mounts` の各ルートに対して `strip_prefix` を試み、
/// `mount_id` が空でなければ `"{mount_id}/{relative}"` 形式で返す
fn compute_relative_path(path: &Path, mounts: &[(String, PathBuf)]) -> Option<String> {
    for (mount_id, root) in mounts {
        if let Ok(rel) = path.strip_prefix(root) {
            let rel_str = rel.to_string_lossy();
            if mount_id.is_empty() {
                return Some(rel_str.to_string());
            }
            return Some(format!("{mount_id}/{rel_str}"));
        }
    }
    None
}

// ---------- テスト ----------

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::path::PathBuf;

    // --- is_hidden_under_mounts ---

    fn pictures_mount() -> Vec<(String, PathBuf)> {
        vec![("pictures".to_string(), PathBuf::from("/data/pictures"))]
    }

    #[rstest]
    // マウント直下のファイル名が . 始まり (既存ケース)
    #[case("/data/pictures/.hidden", true)]
    #[case("/data/pictures/.gitignore", true)]
    // 通常ケース
    #[case("/data/pictures/visible.txt", false)]
    #[case("/data/pictures/dir/file.zip", false)]
    // 親 hidden + 子は通常名 (本改修で拾うケース)
    #[case("/data/pictures/.hidden/foo.mp4", true)]
    // 中間 hidden (より深いネスト)
    #[case("/data/pictures/dir/.hidden/sub/foo.mp4", true)]
    fn 隠しファイルのフィルタリングが正しく動作する(
        #[case] path: &str,
        #[case] expected: bool,
    ) {
        assert_eq!(
            is_hidden_under_mounts(Path::new(path), &pictures_mount()),
            expected,
        );
    }

    #[test]
    fn マウント外パスは安全側で隠し扱いにする() {
        // FileWatcher は本来マウント配下のみ監視するため、マウント外は fail-safe で hidden
        assert!(is_hidden_under_mounts(
            Path::new("/other/path/file.txt"),
            &pictures_mount(),
        ));
    }

    #[test]
    fn マウントルート自身がドット始まりでも配下は通常パスなら通す() {
        // parallel_walk::scan_one の BFS 起点が skip_hidden 対象外なのと一致させる
        let mounts = vec![("archive".to_string(), PathBuf::from("/data/.archive"))];
        assert!(!is_hidden_under_mounts(
            Path::new("/data/.archive/album/pic.jpg"),
            &mounts,
        ));
        // 配下に . 始まりがあれば hidden
        assert!(is_hidden_under_mounts(
            Path::new("/data/.archive/.secret/pic.jpg"),
            &mounts,
        ));
    }

    // --- compute_relative_path ---

    #[test]
    fn compute_relative_pathが正しくパスを解決する() {
        let mounts = vec![
            ("pictures".to_string(), PathBuf::from("/data/pictures")),
            ("videos".to_string(), PathBuf::from("/data/videos")),
        ];

        // マウント内のパス → mount_id/relative 形式
        assert_eq!(
            compute_relative_path(Path::new("/data/pictures/album/photo.jpg"), &mounts),
            Some("pictures/album/photo.jpg".to_string()),
        );

        // 別のマウント
        assert_eq!(
            compute_relative_path(Path::new("/data/videos/movie.mp4"), &mounts),
            Some("videos/movie.mp4".to_string()),
        );

        // マウント外のパス → None
        assert_eq!(
            compute_relative_path(Path::new("/other/path/file.txt"), &mounts),
            None,
        );
    }

    #[test]
    fn compute_relative_pathが空mount_idで正しく動作する() {
        let mounts = vec![(String::new(), PathBuf::from("/data"))];

        assert_eq!(
            compute_relative_path(Path::new("/data/subdir/file.zip"), &mounts),
            Some("subdir/file.zip".to_string()),
        );
    }

    // --- is_indexable_extension ---

    #[rstest]
    #[case(".mp4", true)]
    #[case(".mkv", true)]
    #[case(".zip", true)]
    #[case(".rar", true)]
    #[case(".7z", true)]
    #[case(".cbz", true)]
    #[case(".pdf", true)]
    #[case(".jpg", false)]
    #[case(".png", false)]
    #[case(".txt", false)]
    #[case(".exe", false)]
    #[case("", false)]
    fn is_indexable_extensionが正しく判定する(#[case] ext: &str, #[case] expected: bool) {
        assert_eq!(is_indexable_extension(ext), expected);
    }
}
