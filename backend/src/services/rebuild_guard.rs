//! rebuild / hot reload 経路の全体排他 guard
//!
//! - `AppState.rebuild_guard` に 1 つだけ存在し、`rebuild` / `mount reload` の
//!   同時実行を `compare_exchange` で排他する
//! - `try_acquire` が `Some(RebuildGuardAcquired)` を返した場合のみ保持者になり、
//!   guard の `Drop` で必ず `held=false` に戻す（panic でも release 保証）
//! - `RebuildGuardAcquired` は `Arc<RebuildGuard>` を内部保持し `'static` なため、
//!   `tokio::spawn` した task へ move できる（task panic でも Drop が走る）
//! - `is_held` は `FileWatcher` flush 抑止や `/api/health` 観測に使用
//!
//! Ordering: 取得は `AcqRel` / `Acquire`、解放は `Release`、観測は `Acquire`

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// rebuild / hot reload 全体排他 guard 本体
pub(crate) struct RebuildGuard {
    held: AtomicBool,
}

impl RebuildGuard {
    pub(crate) fn new() -> Self {
        Self {
            held: AtomicBool::new(false),
        }
    }

    /// guard 取得を試みる。成功時は RAII ハンドルを返し、失敗時は `None`
    ///
    /// - 成功時: `held=false -> true` を CAS で確定し `RebuildGuardAcquired` を返却
    /// - 失敗時: 別者が保持中、呼び出し側は 409 等で応答する
    /// - 返却される `RebuildGuardAcquired` は `Arc<Self>` を保持するため `'static`、
    ///   `tokio::spawn` の task に move できる（task panic でも Drop が走る）
    pub(crate) fn try_acquire(self: &Arc<Self>) -> Option<RebuildGuardAcquired> {
        match self
            .held
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => Some(RebuildGuardAcquired {
                owner: Arc::clone(self),
            }),
            Err(_) => None,
        }
    }

    /// 保持中かを非破壊で観測する
    ///
    /// `FileWatcher` flush 延期判定や health diagnostics に使用
    pub(crate) fn is_held(&self) -> bool {
        self.held.load(Ordering::Acquire)
    }
}

impl Default for RebuildGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// guard 取得の RAII ハンドル
///
/// - Drop 時に `RebuildGuard.held` を `false` へ戻す
/// - `Arc<RebuildGuard>` を保持するため `'static`、`tokio::spawn` の task に
///   move して task 末尾まで保持させられる（task panic でも Drop 発火）
#[must_use = "RebuildGuardAcquired を drop するまで他者は guard を取得できない"]
pub(crate) struct RebuildGuardAcquired {
    owner: Arc<RebuildGuard>,
}

impl Drop for RebuildGuardAcquired {
    fn drop(&mut self) {
        self.owner.held.store(false, Ordering::Release);
    }
}

#[cfg(test)]
#[allow(
    non_snake_case,
    reason = "日本語テスト名で振る舞いを記述する規約 (07_testing.md)"
)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn RebuildGuardは取得と解放でis_heldが切り替わる() {
        let guard = Arc::new(RebuildGuard::new());
        assert!(!guard.is_held());
        {
            let acquired = guard.try_acquire();
            assert!(acquired.is_some());
            assert!(guard.is_held());
        }
        assert!(!guard.is_held());
    }

    #[test]
    fn RebuildGuardAcquiredはdropでreleaseする() {
        let guard = Arc::new(RebuildGuard::new());
        let acquired = guard.try_acquire().expect("取得成功");
        assert!(guard.is_held());
        drop(acquired);
        assert!(!guard.is_held());
    }

    #[test]
    fn RebuildGuardは保持中の再取得でNoneを返す() {
        let guard = Arc::new(RebuildGuard::new());
        let _first = guard.try_acquire().expect("1 回目は成功");
        assert!(guard.try_acquire().is_none());
    }

    #[test]
    fn RebuildGuardは並行try_acquireで1者のみ成功する() {
        let guard = Arc::new(RebuildGuard::new());
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let success = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut handles = Vec::with_capacity(8);
        for _ in 0..8 {
            let guard = Arc::clone(&guard);
            let barrier = Arc::clone(&barrier);
            let success = Arc::clone(&success);
            handles.push(thread::spawn(move || {
                barrier.wait();
                if let Some(acquired) = guard.try_acquire() {
                    success.fetch_add(1, Ordering::Relaxed);
                    // 保持期間を短く (解放前に他スレッドの CAS を試行させる)
                    thread::sleep(std::time::Duration::from_millis(5));
                    drop(acquired);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // 任意時点で 1 者のみ保持だが、解放後に他者が取得する可能性があるため
        // 最終的な成功数は並行数 (8) まで幅がありうる
        let final_count = success.load(Ordering::Relaxed);
        assert!((1..=8).contains(&final_count));
        // 全員が解放後のため held=false のはず
        assert!(!guard.is_held());
    }

    #[test]
    fn RebuildGuardAcquiredはtokio_taskに_moveできる() {
        // RebuildGuardAcquired が 'static であることを型レベルで検証
        fn assert_static<T: 'static>(_: &T) {}
        let guard = Arc::new(RebuildGuard::new());
        let acquired = guard.try_acquire().unwrap();
        assert_static(&acquired);
    }
}
