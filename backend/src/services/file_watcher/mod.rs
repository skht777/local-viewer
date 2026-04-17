//! ファイルシステム変更監視
//!
//! `notify` v8 クレートでファイルの作成/削除/移動を検知し、
//! `Indexer` にインクリメンタルに反映する。
//! 1 秒間隔のバッチフラッシュで高頻度イベントをデバウンスする。
//!
//! サブモジュール構成:
//! - `events`: notify イベントの分類 (add / remove)
//! - `filter`: pending への enqueue + hidden / 拡張子フィルタ
//! - `path_utils`: マウント相対パス計算
//! - `worker`: 1 秒バッチ flush worker + 1 件イベント処理

mod events;
mod filter;
mod path_utils;
mod worker;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{info, warn};

use crate::services::dir_index::DirIndex;
use crate::services::indexer::Indexer;
use crate::services::path_security::PathSecurity;

/// ファイル監視エラー
#[derive(Debug, thiserror::Error)]
pub(crate) enum FileWatcherError {
    #[error("notify error: {0}")]
    Notify(#[from] notify::Error),
    #[error("{0}")]
    Other(String),
}

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
                    events::handle_notify_event(&pending_for_cb, &event, &mounts_for_cb);
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
        let handle = tokio::spawn(worker::flush_worker_loop(
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
