use super::common::setup;
use crate::services::dir_index::DirIndex;

#[test]
fn init_dbでスキーマが作成される() {
    let (idx, _tmp) = setup();
    let count = idx.entry_count().unwrap();
    assert_eq!(count, 0);
}

#[test]
fn is_full_scan_doneのフラグ管理() {
    let (idx, _tmp) = setup();

    assert!(!idx.is_full_scan_done().unwrap());
    idx.mark_full_scan_done().unwrap();
    assert!(idx.is_full_scan_done().unwrap());
}

#[test]
fn mark_readyとmark_warm_startの状態遷移() {
    let idx = DirIndex::new(":memory:");

    // 初期状態
    assert!(!idx.is_ready());
    assert!(!idx.is_stale());

    // ウォームスタート
    idx.mark_warm_start();
    assert!(idx.is_ready());
    assert!(idx.is_stale());

    // 準備完了
    idx.mark_ready();
    assert!(idx.is_ready());
    assert!(!idx.is_stale());
}
