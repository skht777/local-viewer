//! `cache_key` 単位の生成中ロック
//!
//! 同一 `cache_key` への並列生成リクエストを 1 本に集約する。`spawn_blocking` 内から
//! 同期的に使うため `std::sync::{Mutex, Condvar}` のみで実装する（`tokio::sync` は不可）。
//!
//! # 経路
//! - Owner: `acquire()` で新規 slot を作る側。実際に生成処理を走らせる
//! - Waiter: 既存 slot を見つけた側。`wait_blocking()` で完了通知を待つ
//!
//! # ロック獲得順序の不変条件
//! `map → slot.state` の単方向のみ。逆順を取得する関数を作らない。
//!
//! # `OwnerGuard::drop` の不変条件
//! `(1) map から該当 key を除去 → (2) slot.done = true → (3) cond.notify_all` の順。
//! 逆順だと、起きた Waiter が再 `acquire()` した瞬間、まだ map に残っている stale slot
//! を Waiter として取得し、即 `wait_blocking()` が return → 再 Owner 化されず Err になる
//! race が発生する。

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, PoisonError};

/// `cache_key` 単位の生成中ロックレジストリ
pub(crate) struct InflightLocks {
    map: Mutex<HashMap<String, Arc<InflightSlot>>>,
}

struct InflightSlot {
    state: Mutex<SlotState>,
    cond: Condvar,
}

struct SlotState {
    done: bool,
}

/// `InflightLocks::acquire` の戻り値
pub(crate) enum Acquired {
    /// 自分が生成担当。OwnerGuard を保持している間、他者は Waiter として待つ。
    /// Drop で map から除去 + 完了通知が走る。
    Owner(OwnerGuard),
    /// 他者が生成中。`wait_blocking()` で完了まで同期待機する。
    Waiter(WaiterHandle),
}

impl InflightLocks {
    /// 新規作成
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            map: Mutex::new(HashMap::new()),
        })
    }

    /// `cache_key` の生成権を取得する
    ///
    /// - 既存 slot が無ければ新規作成し `Owner` を返す
    /// - 既存 slot があれば `Waiter` を返す
    pub(crate) fn acquire(self: &Arc<Self>, cache_key: &str) -> Acquired {
        let mut map = self.map.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = map.get(cache_key) {
            let slot = Arc::clone(slot);
            return Acquired::Waiter(WaiterHandle { slot });
        }
        let slot = Arc::new(InflightSlot {
            state: Mutex::new(SlotState { done: false }),
            cond: Condvar::new(),
        });
        map.insert(cache_key.to_string(), Arc::clone(&slot));
        Acquired::Owner(OwnerGuard {
            locks: Arc::clone(self),
            key: cache_key.to_string(),
            slot,
        })
    }

    /// 現在 inflight な `cache_key` 数を返す（テスト用）
    #[cfg(test)]
    fn pending_count(&self) -> usize {
        self.map
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

/// 生成権を持つガード。Drop でクリーンアップする
pub(crate) struct OwnerGuard {
    locks: Arc<InflightLocks>,
    key: String,
    slot: Arc<InflightSlot>,
}

impl Drop for OwnerGuard {
    fn drop(&mut self) {
        // 順序: (1) map から除去 → (2) slot.done=true → (3) notify_all
        // 逆順 race については module doc 参照
        {
            let mut map = self
                .locks
                .map
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            map.remove(&self.key);
        }
        {
            let mut state = self
                .slot
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            state.done = true;
        }
        self.slot.cond.notify_all();
    }
}

/// 待機側ハンドル
pub(crate) struct WaiterHandle {
    slot: Arc<InflightSlot>,
}

impl WaiterHandle {
    /// Owner の完了まで同期的に待機する
    pub(crate) fn wait_blocking(self) {
        let mut state = self
            .slot
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        while !state.done {
            state = self
                .slot
                .cond
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    use super::*;

    #[test]
    fn acquire単発でownerを返す() {
        let locks = InflightLocks::new();
        let acq = locks.acquire("k1");
        assert!(matches!(acq, Acquired::Owner(_)));
        // map に登録されていること
        assert_eq!(locks.pending_count(), 1);
    }

    #[test]
    fn 異なるcache_key同士は独立にownerを取れる() {
        let locks = InflightLocks::new();
        let _a = locks.acquire("k1");
        let b = locks.acquire("k2");
        assert!(matches!(b, Acquired::Owner(_)));
        assert_eq!(locks.pending_count(), 2);
    }

    #[test]
    fn 同一keyの並列acquireでowner_1と_waiter_nになり通知で起きる() {
        let locks = InflightLocks::new();
        let n = 8;
        let barrier = Arc::new(Barrier::new(n + 1));
        let owner_count = Arc::new(AtomicUsize::new(0));
        let waiter_done = Arc::new(AtomicUsize::new(0));

        // Owner を先に取る
        let owner_acq = locks.acquire("k");
        assert!(matches!(owner_acq, Acquired::Owner(_)));

        let handles: Vec<_> = (0..n)
            .map(|_| {
                let locks = Arc::clone(&locks);
                let barrier = Arc::clone(&barrier);
                let owner_count = Arc::clone(&owner_count);
                let waiter_done = Arc::clone(&waiter_done);
                thread::spawn(move || {
                    barrier.wait();
                    match locks.acquire("k") {
                        Acquired::Owner(_) => {
                            owner_count.fetch_add(1, Ordering::Relaxed);
                        }
                        Acquired::Waiter(h) => {
                            h.wait_blocking();
                            waiter_done.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                })
            })
            .collect();

        barrier.wait();
        // 少し待ってから Owner を解放（全 Waiter が wait に入る時間を確保）
        thread::sleep(Duration::from_millis(50));
        drop(owner_acq);

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(owner_count.load(Ordering::Relaxed), 0);
        assert_eq!(waiter_done.load(Ordering::Relaxed), n);
        // map は空（Owner も Drop されたあと）
        assert_eq!(locks.pending_count(), 0);
    }

    #[test]
    fn owner_guardをpanicでdropしても全waiterが起きる() {
        let locks = InflightLocks::new();
        let n = 4;
        let barrier = Arc::new(Barrier::new(n + 1));
        let waiter_done = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..n)
            .map(|_| {
                let locks = Arc::clone(&locks);
                let barrier = Arc::clone(&barrier);
                let waiter_done = Arc::clone(&waiter_done);
                thread::spawn(move || {
                    barrier.wait();
                    if let Acquired::Waiter(h) = locks.acquire("k") {
                        h.wait_blocking();
                        waiter_done.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();

        // 親スレッドで Owner を取り panic させる
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _owner = locks.acquire("k");
            barrier.wait();
            // Waiter が wait に入るまで少し待つ
            thread::sleep(Duration::from_millis(50));
            panic!("simulated panic");
        }));
        assert!(result.is_err());

        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(waiter_done.load(Ordering::Relaxed), n);
        assert_eq!(locks.pending_count(), 0);
    }

    #[test]
    fn owner_panic後の再acquireで新ownerになるのは1スレッドだけ() {
        // 複数 Waiter が起きて再 acquire しても、新 Owner は 1 つだけ
        let locks = InflightLocks::new();
        let n = 8;
        let barrier = Arc::new(Barrier::new(n + 1));
        let owner_count = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..n)
            .map(|_| {
                let locks = Arc::clone(&locks);
                let barrier = Arc::clone(&barrier);
                let owner_count = Arc::clone(&owner_count);
                thread::spawn(move || {
                    barrier.wait();
                    if let Acquired::Waiter(h) = locks.acquire("k") {
                        h.wait_blocking();
                        // 起きたあと再 acquire を試みる（最大 2 回までのうちの 1 回目）
                        match locks.acquire("k") {
                            Acquired::Owner(_g) => {
                                // 新 Owner になったら少し滞在して他のスレッドを Waiter 化させる
                                owner_count.fetch_add(1, Ordering::Relaxed);
                                thread::sleep(Duration::from_millis(20));
                            }
                            Acquired::Waiter(h2) => {
                                h2.wait_blocking();
                            }
                        }
                    }
                })
            })
            .collect();

        // 親スレッドが Owner → panic で drop（Waiter を起こす）
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _owner = locks.acquire("k");
            barrier.wait();
            thread::sleep(Duration::from_millis(50));
            panic!("simulated panic");
        }));

        for h in handles {
            h.join().unwrap();
        }

        // 新 Owner は 1 つだけ（残りは新 active slot の Waiter として待ったあと、最終 Owner の Drop で起きる）
        assert_eq!(owner_count.load(Ordering::Relaxed), 1);
        assert_eq!(locks.pending_count(), 0);
    }

    #[test]
    fn drop通知直後の再acquireがstale_done_slotを踏まない() {
        // Drop 順序（map remove → done=true → notify）が守られていれば、
        // 再 acquire は新 Owner または新 active slot の Waiter になる。
        let locks = InflightLocks::new();
        let iterations = 100;

        for i in 0..iterations {
            let key = format!("k{i}");
            let owner = locks.acquire(&key);
            assert!(matches!(owner, Acquired::Owner(_)));
            drop(owner);
            // Drop 直後の再 acquire は必ず新 Owner（map から除去済み）
            let again = locks.acquire(&key);
            assert!(
                matches!(again, Acquired::Owner(_)),
                "iteration {i}: stale done slot を Waiter として返してしまった"
            );
            drop(again);
        }
        assert_eq!(locks.pending_count(), 0);
    }

    #[test]
    fn acquireとdropのシーケンスでmapにキーが残らない() {
        let locks = InflightLocks::new();
        for i in 0..50 {
            let key = format!("k{i}");
            let g = locks.acquire(&key);
            drop(g);
        }
        assert_eq!(locks.pending_count(), 0);
    }
}
