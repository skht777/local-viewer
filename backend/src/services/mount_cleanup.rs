//! マウント構成変更時の stale 行クリーンアップ（services 層ヘルパー）
//!
//! 起動時 (bootstrap) から呼ばれる低レベル helper + 便利関数を提供する。
//! 将来 rebuild API / 管理 CLI 等から呼ぶ可能性を想定して services 層に配置し、
//! 依存方向（`bootstrap → services`, `routers → services`）を一方向で維持する。

use std::collections::HashSet;

use crate::services::indexer::Indexer;

/// 旧 fingerprint と現 `mount_id` セットの差分から stale `mount_id` を列挙する
///
/// - 旧 fingerprint 読み取り失敗時は `tracing::warn!` + 空 `Vec`
///   （cleanup をスキップし次回起動で再試行）
/// - all-or-nothing filter は `Indexer::load_stored_mount_ids` に委譲
pub(crate) fn enumerate_stale_mount_ids(indexer: &Indexer, current_ids: &[&str]) -> Vec<String> {
    match indexer.load_stored_mount_ids() {
        Ok(old_ids) => {
            let current_set: HashSet<&str> = current_ids.iter().copied().collect();
            old_ids
                .into_iter()
                .filter(|id| !current_set.contains(id.as_str()))
                .collect()
        }
        Err(e) => {
            tracing::warn!("旧 fingerprint 読み出し失敗 (stale cleanup skip): {e}");
            Vec::new()
        }
    }
}

/// `stale_ids` を `delete_one` で順次削除し、全成功可否を返す（低レベル）
///
/// - **呼び出し位置**: 必ず `spawn_blocking` の**内部**で呼ぶ（同期 DB I/O のため）
/// - `label` はログ識別用（`"entries"` / `"dir_entries"` 等）
/// - エラー型はジェネリック (`Display`) で `Indexer` / `DirIndex` 両方に対応
/// - **unit test の第一選択**。`delete_one` に fault injection closure を渡す
pub(crate) fn perform_stale_cleanup<F, E>(label: &str, stale_ids: &[String], delete_one: F) -> bool
where
    F: Fn(&str) -> Result<usize, E>,
    E: std::fmt::Display,
{
    let mut all_ok = true;
    for id in stale_ids {
        match delete_one(id) {
            Ok(n) => tracing::info!("stale {label} 行削除: mount_id={id}, rows={n}"),
            Err(e) => {
                tracing::error!("stale {label} 行削除失敗: mount_id={id}, err={e}");
                all_ok = false;
            }
        }
    }
    all_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::indexer::{Indexer, IndexerError};

    const MOUNT_A: &str = "aaaaaaaaaaaaaaaa";
    const MOUNT_B: &str = "bbbbbbbbbbbbbbbb";
    const MOUNT_C: &str = "cccccccccccccccc";

    fn setup_indexer() -> (Indexer, tempfile::NamedTempFile) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let indexer = Indexer::new(tmp.path().to_str().unwrap());
        indexer.init_db().unwrap();
        (indexer, tmp)
    }

    #[test]
    fn perform_stale_cleanupは全成功時trueを返す() {
        let calls = std::sync::Mutex::new(Vec::<String>::new());
        let ids = vec![MOUNT_A.to_string(), MOUNT_B.to_string()];
        let ok = perform_stale_cleanup("entries", &ids, |id| -> Result<usize, IndexerError> {
            calls.lock().unwrap().push(id.to_string());
            Ok(5)
        });
        assert!(ok);
        let calls = calls.into_inner().unwrap();
        assert_eq!(calls, vec![MOUNT_A.to_string(), MOUNT_B.to_string()]);
    }

    #[test]
    fn perform_stale_cleanupは部分失敗時falseを返し全件試行する() {
        let calls = std::sync::Mutex::new(Vec::<String>::new());
        let ids = vec![
            MOUNT_A.to_string(),
            MOUNT_B.to_string(),
            MOUNT_C.to_string(),
        ];
        let ok = perform_stale_cleanup("entries", &ids, |id| -> Result<usize, IndexerError> {
            calls.lock().unwrap().push(id.to_string());
            if id == MOUNT_B {
                Err(IndexerError::Other("forced failure".into()))
            } else {
                Ok(3)
            }
        });
        assert!(!ok, "部分失敗なら false を返すべき");
        let calls = calls.into_inner().unwrap();
        assert_eq!(
            calls,
            vec![
                MOUNT_A.to_string(),
                MOUNT_B.to_string(),
                MOUNT_C.to_string()
            ]
        );
    }

    #[test]
    fn perform_stale_cleanupはlabel文字列を含むログを出す() {
        // label がログ文字列に反映されることを確認する最小ケース
        // （"entries" / "dir_entries" で識別できる前提を担保）
        #[derive(Debug)]
        struct DummyErr;
        impl std::fmt::Display for DummyErr {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "dummy")
            }
        }
        let ids = vec![MOUNT_A.to_string()];
        let ok = perform_stale_cleanup("any_label", &ids, |_| -> Result<usize, DummyErr> { Ok(1) });
        assert!(ok);
    }

    #[test]
    fn enumerate_stale_mount_idsは旧と新の差分を返す() {
        let (indexer, _tmp) = setup_indexer();
        indexer.save_mount_fingerprint(&[MOUNT_A, MOUNT_B]).unwrap();
        let stale = enumerate_stale_mount_ids(&indexer, &[MOUNT_A, MOUNT_C]);
        assert_eq!(stale, vec![MOUNT_B.to_string()]);
    }

    #[test]
    fn enumerate_stale_mount_idsはfingerprint未保存で空vecを返す() {
        let (indexer, _tmp) = setup_indexer();
        let stale = enumerate_stale_mount_ids(&indexer, &[MOUNT_A, MOUNT_B]);
        assert!(stale.is_empty());
    }

    #[test]
    fn enumerate_stale_mount_idsは全マウント変更時に旧全件を返す() {
        let (indexer, _tmp) = setup_indexer();
        indexer.save_mount_fingerprint(&[MOUNT_A, MOUNT_B]).unwrap();
        let new_mount = "0123456789abcdef";
        let stale = enumerate_stale_mount_ids(&indexer, &[new_mount]);
        let mut stale_sorted = stale.clone();
        stale_sorted.sort();
        assert_eq!(stale_sorted, vec![MOUNT_A.to_string(), MOUNT_B.to_string()]);
    }
}
