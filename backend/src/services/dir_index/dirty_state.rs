//! `DirIndex` ディレクトリの dirty 状態管理
//!
//! `FileWatcher` がファイル変更を検知したとき、影響を受けた親ディレクトリを dirty 登録する。
//! browse の `fast_path` は dirty ディレクトリを検出すると fallback にフォールバックし、
//! fallback 完了後に `DirIndex` を更新して dirty を解除する。
//!
//! 世代カウンタにより TOCTOU 競合を防止:
//! - `mark_dirty` で世代番号をインクリメント
//! - `clear_if_generation_matches` は browse 開始時に取得した世代と一致する場合のみクリア
//! - スキャン中に追加の `FileWatcher` イベントが来ても、世代不一致でクリアされない

use std::collections::HashMap;

/// dirty セットの既定上限
///
/// browse されないディレクトリの dirty は解除されないため、`FileWatcher` の
/// イベントだけで単調増加し得る。メモリを有界化するため上限を設ける。
const DEFAULT_MAX_DIRTY_ENTRIES: usize = 10_000;

/// ディレクトリ単位の dirty 状態 + 世代カウンタ
pub(crate) struct DirtyState {
    /// `parent_key` → 世代番号
    dirty: HashMap<String, u64>,
    /// グローバル世代カウンタ
    counter: u64,
    /// dirty セットの上限 (超過時は古い世代から退避)
    max_entries: usize,
}

impl DirtyState {
    pub(crate) fn new() -> Self {
        Self {
            dirty: HashMap::new(),
            counter: 0,
            max_entries: DEFAULT_MAX_DIRTY_ENTRIES,
        }
    }

    /// テスト用: 小さい上限で生成する
    #[cfg(test)]
    pub(crate) fn with_max_entries(max_entries: usize) -> Self {
        Self {
            dirty: HashMap::new(),
            counter: 0,
            max_entries,
        }
    }

    /// ディレクトリを dirty にマークし、世代番号を返す
    pub(crate) fn mark_dirty(&mut self, parent_key: &str) -> u64 {
        self.counter += 1;
        self.dirty.insert(parent_key.to_owned(), self.counter);
        self.enforce_capacity();
        self.counter
    }

    /// 上限超過時に古い世代の dirty を退避する
    ///
    /// 退避されたディレクトリは fast-path で fallback に落ちなくなるため、
    /// 子ファイルの in-place 更新が次のディレクトリ mtime 変化 / 差分スキャンまで
    /// 反映されない縮退状態になる。発生を観測できるよう warn ログを出す。
    fn enforce_capacity(&mut self) {
        if self.dirty.len() <= self.max_entries {
            return;
        }
        // 上限の 3/4 (最低 1 件) まで新しい世代を残す
        let target = (self.max_entries * 3 / 4).max(1);
        let mut generations: Vec<u64> = self.dirty.values().copied().collect();
        generations.sort_unstable();
        let cutoff = generations[generations.len() - target];
        let before = self.dirty.len();
        self.dirty.retain(|_, generation| *generation >= cutoff);
        tracing::warn!(
            before,
            after = self.dirty.len(),
            max = self.max_entries,
            "dirty セットが上限を超過、古い世代を退避 (該当ディレクトリの自己修復は次回スキャンまで遅延)"
        );
    }

    /// ディレクトリが dirty かどうか
    pub(crate) fn is_dirty(&self, parent_key: &str) -> bool {
        self.dirty.contains_key(parent_key)
    }

    /// 世代番号が一致する場合のみ dirty を解除する
    ///
    /// browse fallback が開始時に取得した世代と比較し、
    /// 一致すればスキャン中に追加変更がなかったことを保証する
    pub(crate) fn clear_if_generation_matches(
        &mut self,
        parent_key: &str,
        generation: u64,
    ) -> bool {
        if self.dirty.get(parent_key) == Some(&generation) {
            self.dirty.remove(parent_key);
            true
        } else {
            false
        }
    }

    /// 全ディレクトリを dirty にマーク (inotify `Q_OVERFLOW` 時)
    ///
    /// 上限を超える分は登録せず件数を warn ログに出す (メモリ有界化)。
    /// 既存の dirty も含めて上限までしか保持しない。
    pub(crate) fn mark_all_dirty(&mut self, parent_keys: impl IntoIterator<Item = String>) {
        self.counter += 1;
        let mut dropped = 0usize;
        for key in parent_keys {
            if self.dirty.len() >= self.max_entries && !self.dirty.contains_key(&key) {
                dropped += 1;
                continue;
            }
            self.dirty.insert(key, self.counter);
        }
        if dropped > 0 {
            tracing::warn!(
                dropped,
                max = self.max_entries,
                "全体 dirty 化が上限を超過、超過分は登録せず (該当ディレクトリの自己修復は次回スキャンまで遅延)"
            );
        }
    }

    /// dirty エントリ数 (テスト・ログ用)
    #[cfg(test)]
    pub(crate) fn dirty_count(&self) -> usize {
        self.dirty.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_dirtyで世代番号がインクリメントされる() {
        let mut state = DirtyState::new();
        let g1 = state.mark_dirty("pictures/album");
        let g2 = state.mark_dirty("pictures/album2");
        assert_eq!(g1 + 1, g2);
        assert!(state.is_dirty("pictures/album"));
        assert!(state.is_dirty("pictures/album2"));
    }

    #[test]
    fn 世代一致でclearが成功する() {
        let mut state = DirtyState::new();
        let g = state.mark_dirty("pictures/album");
        assert!(state.clear_if_generation_matches("pictures/album", g));
        assert!(!state.is_dirty("pictures/album"));
    }

    #[test]
    fn 世代不一致でclearが失敗する() {
        let mut state = DirtyState::new();
        let g = state.mark_dirty("pictures/album");
        // 再度 dirty 化（世代が進む）
        let _g2 = state.mark_dirty("pictures/album");
        assert!(!state.clear_if_generation_matches("pictures/album", g));
        assert!(state.is_dirty("pictures/album"));
    }

    #[test]
    fn mark_all_dirtyで全キーが登録される() {
        let mut state = DirtyState::new();
        state.mark_all_dirty(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        assert!(state.is_dirty("a"));
        assert!(state.is_dirty("b"));
        assert!(state.is_dirty("c"));
        assert_eq!(state.dirty_count(), 3);
    }

    #[test]
    fn 未登録キーはdirtyでない() {
        let state = DirtyState::new();
        assert!(!state.is_dirty("nonexistent"));
    }

    #[test]
    fn mark_dirtyは上限を超えると古い世代を退避して有界化する() {
        // browse されないディレクトリの dirty が永久残留してメモリが単調増加するのを防ぐ
        let mut state = DirtyState::with_max_entries(4);
        for i in 0..6 {
            state.mark_dirty(&format!("m/dir{i}"));
        }
        assert!(
            state.dirty_count() <= 4,
            "dirty セットは上限以下に保たれるべき: {}",
            state.dirty_count()
        );
        // 最新の登録は残る
        assert!(state.is_dirty("m/dir5"));
        // 最古の登録は退避される
        assert!(!state.is_dirty("m/dir0"));
    }

    #[test]
    fn mark_all_dirtyは上限を超える分を登録しない() {
        let mut state = DirtyState::with_max_entries(3);
        state.mark_all_dirty((0..10).map(|i| format!("m/dir{i}")));
        assert_eq!(state.dirty_count(), 3);
    }
}
