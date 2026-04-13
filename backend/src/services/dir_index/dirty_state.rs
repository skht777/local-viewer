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

/// ディレクトリ単位の dirty 状態 + 世代カウンタ
pub(crate) struct DirtyState {
    /// `parent_key` → 世代番号
    dirty: HashMap<String, u64>,
    /// グローバル世代カウンタ
    counter: u64,
}

impl DirtyState {
    pub(crate) fn new() -> Self {
        Self {
            dirty: HashMap::new(),
            counter: 0,
        }
    }

    /// ディレクトリを dirty にマークし、世代番号を返す
    pub(crate) fn mark_dirty(&mut self, parent_key: &str) -> u64 {
        self.counter += 1;
        self.dirty.insert(parent_key.to_owned(), self.counter);
        self.counter
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
    pub(crate) fn mark_all_dirty(&mut self, parent_keys: impl IntoIterator<Item = String>) {
        self.counter += 1;
        for key in parent_keys {
            self.dirty.insert(key, self.counter);
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
}
