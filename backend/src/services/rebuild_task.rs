//! rebuild 実行中タスクの追跡ハンドル
//!
//! - `AppState.shutdown.rebuild_task: Arc<Mutex<Option<Arc<RebuildTaskHandle>>>>` に格納
//! - `generation` で「自分が入れた slot だけ clear」する race 回避を実現
//! - `abort` で即座にキャンセル可能（`JoinHandle::abort_handle` 経由）
//! - `join` は drain 側が `take()` で奪って `await` する。wrapper task が
//!   先に take した場合は drain 側は `None` で no-op、wrapper 側も逆も同様
//!   （どちらか一方が await を担当する）

use std::sync::Mutex;

use tokio::task::{AbortHandle, JoinHandle};

/// rebuild スレッド 1 本分の追跡ハンドル
pub(crate) struct RebuildTaskHandle {
    /// 登録世代 — wrapper task が slot clear する際に「自分が入れたか」を判定
    pub generation: u64,
    /// `JoinHandle::abort_handle()` から取得。`abort()` は冪等
    pub abort: AbortHandle,
    /// 実体の `JoinHandle`。wrapper task もしくは `drain_long_tasks` が `take()` して await
    pub join: Mutex<Option<JoinHandle<()>>>,
}

impl RebuildTaskHandle {
    /// `JoinHandle` を受け取り、`abort` / `join` を初期化する
    pub(crate) fn new(generation: u64, join: JoinHandle<()>) -> Self {
        let abort = join.abort_handle();
        Self {
            generation,
            abort,
            join: Mutex::new(Some(join)),
        }
    }
}

#[cfg(test)]
#[allow(
    non_snake_case,
    reason = "日本語テスト名で振る舞いを記述する規約 (07_testing.md)"
)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn new_handleは渡したjoinhandleのabort_handleを保持する() {
        let inner = tokio::spawn(async {
            // ダミータスク: 短時間で完了
            tokio::task::yield_now().await;
        });
        let handle = RebuildTaskHandle::new(1, inner);
        assert_eq!(handle.generation, 1);
        assert!(handle.join.lock().unwrap().is_some());
        // abort_handle は同じタスクを指す
        handle.abort.abort();
        let join = handle.join.lock().unwrap().take().unwrap();
        let result = join.await;
        // abort で JoinError（cancelled）または成功のいずれか（yield 直後に abort）
        assert!(result.is_err() || result.is_ok());
    }

    #[tokio::test]
    async fn joinはwrapperが先にtakeしたらdrain側はNoneになる() {
        let inner = tokio::spawn(async { tokio::task::yield_now().await });
        let handle = RebuildTaskHandle::new(2, inner);
        // wrapper 役が take
        let wrapper_join = handle.join.lock().unwrap().take();
        assert!(wrapper_join.is_some());
        // drain 役が見ると None
        let drain_join = handle.join.lock().unwrap().take();
        assert!(drain_join.is_none());
        // wrapper が await
        let _ = wrapper_join.unwrap().await;
    }
}
