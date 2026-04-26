//! 起動時スキャンの診断 DTO と純粋関数
//!
//! - `/api/health` に公開する診断情報を提供し、partial init 状態
//!   (`/api/ready=503`) の原因を外部から識別可能にする
//! - `ScanDiagnostics` / `MountDiagnostic` / `WalkMetrics` / `FingerprintAction`
//!   を定義し、`AppState.last_scan_report` に格納される
//! - `decide_fingerprint_action` は Step 4 の fingerprint 分岐を純粋関数として
//!   切り出し、全 5 分岐を unit test で担保する
//! - `finalize_scan_success` は bootstrap / rebuild 双方から呼ばれる共通
//!   promote 経路: `all_ok` 時に readiness flag を立て、`last_scan_report`
//!   を原子的に更新する

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::services::dir_index::DirIndex;
use crate::services::parallel_walk::{WalkErrorKind, WalkReport};

/// 起動時スキャン 1 回の診断結果
///
/// - `completed_at_ms`: scan 完了時刻 (UNIX epoch ms)
/// - `is_warm_start`: warm 経路 (`incremental_scan`) で入ったか
/// - `cleanup_ok` / `scans_ok` / `all_ok`: readiness gate の 3 値
/// - `fingerprint`: 起動時 fingerprint 操作の結果 (NotNeeded/Saved/Cleared/ClearFailed/SaveFailed)
/// - `mounts`: 各マウントの成否。cold start のみ `walk` が `Some`
/// - `cancelled`: いずれかの mount で **per-mount scan/rebuild** が
///   `IndexerError::Cancelled` により途中終了した、または per-mount ループ
///   先頭 break で shutdown を検知した場合 true。起動時 stale cleanup の cancel
///   （`perform_full_stale_cleanup` は `bool` のみ返す）は本フィールドでは
///   追跡しない。cleanup の cancel は `cleanup_ok = false` で表現され、
///   本フィールドとは独立
#[derive(Debug, Clone, Serialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "readiness gate の 3 値 + is_warm_start + cancelled は起動時診断の独立軸で semantic 的に分離"
)]
pub(crate) struct ScanDiagnostics {
    pub completed_at_ms: u64,
    pub is_warm_start: bool,
    pub cleanup_ok: bool,
    pub scans_ok: bool,
    pub all_ok: bool,
    pub cancelled: bool,
    pub fingerprint: FingerprintAction,
    pub mounts: Vec<MountDiagnostic>,
}

/// 起動時 fingerprint 操作の結果
///
/// - `NotNeeded`: cold partial 等で操作自体を行わなかった
/// - `Saved`: `all_ok=true` で `save_mount_fingerprint` 成功
/// - `Cleared`: warm partial で `clear_mount_fingerprint` 成功 (次回 cold start で復旧)
/// - `ClearFailed`: warm partial で `clear_mount_fingerprint` 失敗 (自動復旧不能)
/// - `SaveFailed`: `all_ok=true` で `save_mount_fingerprint` 失敗
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FingerprintAction {
    NotNeeded,
    Saved,
    Cleared,
    ClearFailed,
    SaveFailed,
}

/// 各マウントのスキャン結果
///
/// - `panicked`: `spawn_blocking` が panic して outcome が `None` だった場合 true
/// - `walk`: cold start のみ収集。**warm start (incremental) では常に `None`** (API 契約)
/// - `cancelled`: 当該 mount の scan/rebuild が `IndexerError::Cancelled` を返したか
#[derive(Debug, Clone, Serialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "per-mount 診断の 4 軸 (scan_ok / dir_index_ok / panicked / cancelled) は独立した失敗軸で semantic 的に分離"
)]
pub(crate) struct MountDiagnostic {
    pub mount_id: String,
    pub scan_ok: bool,
    pub dir_index_ok: bool,
    pub panicked: bool,
    pub cancelled: bool,
    pub walk: Option<WalkMetrics>,
}

/// `WalkReport` のうち path を含まない集計値のみを公開
///
/// - `path` や `io::Error` 詳細は含めない (情報漏洩防止)
/// - `error_kind_counts` は `String` キーで serde 化 (`BTreeMap` で安定順)
#[derive(Debug, Clone, Serialize)]
pub(crate) struct WalkMetrics {
    pub entry_count: usize,
    pub observed_entries: usize,
    pub total_error_count: usize,
    pub error_kind_counts: BTreeMap<String, usize>,
}

impl From<&WalkReport> for WalkMetrics {
    fn from(report: &WalkReport) -> Self {
        let error_kind_counts = report
            .error_kind_counts
            .iter()
            .map(|(k, v)| (kind_label(*k).to_owned(), *v))
            .collect();
        Self {
            entry_count: report.entry_count,
            observed_entries: report.observed_entries,
            total_error_count: report.total_error_count,
            error_kind_counts,
        }
    }
}

/// `WalkErrorKind` を JSON 用の `snake_case` ラベルに変換
///
/// `WalkErrorKind` 自体への `Serialize` 派生は services 内のモデルを API 都合で
/// 変更することになるため避け、本関数で変換する
pub(crate) fn kind_label(kind: WalkErrorKind) -> &'static str {
    match kind {
        WalkErrorKind::ReadDir => "read_dir",
        WalkErrorKind::DirEntry => "dir_entry",
        WalkErrorKind::Metadata => "metadata",
    }
}

/// scan 完了時の readiness promote + diagnostics 永続化
///
/// 起動時 scan（`bootstrap::background_tasks`）と rebuild API の双方から呼ぶ。
/// 両経路で `/api/ready` を昇格させる際の手順を同一に保つため、
/// - `all_ok=true` かつ `is_warm_start=false` → `DirIndex::mark_full_scan_done`
/// - `all_ok=true` → `DirIndex::mark_ready` + `scan_complete=true`
/// - `last_scan_report` は成否によらず常に最新値で書き込み（poison 時は warn + スキップ）
pub(crate) fn finalize_scan_success(
    dir_index: &DirIndex,
    scan_complete: &AtomicBool,
    last_scan_report: &std::sync::RwLock<Option<Arc<ScanDiagnostics>>>,
    diagnostics: ScanDiagnostics,
) {
    if !diagnostics.is_warm_start
        && diagnostics.all_ok
        && let Err(e) = dir_index.mark_full_scan_done()
    {
        tracing::error!("DirIndex::mark_full_scan_done 失敗: {e}");
    }
    if diagnostics.all_ok {
        dir_index.mark_ready();
        scan_complete.store(true, Ordering::Relaxed);
    }
    match last_scan_report.write() {
        Ok(mut guard) => *guard = Some(Arc::new(diagnostics)),
        Err(poisoned) => {
            tracing::error!("last_scan_report poisoned (write): {poisoned}");
        }
    }
}

/// Step 4 の fingerprint 分岐を純粋関数に抽出
///
/// - 全成功 (`all_ok=true`): save 成否で `Saved` / `SaveFailed`
/// - warm partial (`all_ok=false && is_warm_start=true`): clear 成否で `Cleared` / `ClearFailed`
/// - cold partial (`all_ok=false && is_warm_start=false`): `NotNeeded` (次回起動で再試行)
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "readiness gate の 4 軸 (all_ok / is_warm / save_ok / clear_ok) は起動時診断の固有入力で意味的に分離"
)]
pub(crate) fn decide_fingerprint_action(
    all_ok: bool,
    is_warm_start: bool,
    save_ok: bool,
    clear_ok: bool,
) -> FingerprintAction {
    match (all_ok, is_warm_start) {
        (true, _) => {
            if save_ok {
                FingerprintAction::Saved
            } else {
                FingerprintAction::SaveFailed
            }
        }
        (false, true) => {
            if clear_ok {
                FingerprintAction::Cleared
            } else {
                FingerprintAction::ClearFailed
            }
        }
        (false, false) => FingerprintAction::NotNeeded,
    }
}

#[cfg(test)]
#[allow(
    non_snake_case,
    reason = "日本語テスト名で振る舞いを記述する規約 (07_testing.md)"
)]
mod tests {
    use super::*;

    #[test]
    fn kind_labelはWalkErrorKindをsnake_caseに変換する() {
        assert_eq!(kind_label(WalkErrorKind::ReadDir), "read_dir");
        assert_eq!(kind_label(WalkErrorKind::DirEntry), "dir_entry");
        assert_eq!(kind_label(WalkErrorKind::Metadata), "metadata");
    }

    #[test]
    fn WalkMetricsはWalkReportから変換できる() {
        let report = WalkReport {
            entry_count: 100,
            observed_entries: 120,
            total_error_count: 3,
            error_kind_counts: [(WalkErrorKind::ReadDir, 2), (WalkErrorKind::Metadata, 1)]
                .into_iter()
                .collect(),
            ..WalkReport::default()
        };

        let m = WalkMetrics::from(&report);
        assert_eq!(m.entry_count, 100);
        assert_eq!(m.observed_entries, 120);
        assert_eq!(m.total_error_count, 3);
        assert_eq!(m.error_kind_counts.get("read_dir"), Some(&2));
        assert_eq!(m.error_kind_counts.get("metadata"), Some(&1));
        assert_eq!(m.error_kind_counts.get("dir_entry"), None);
    }

    #[test]
    fn FingerprintActionはsnake_caseでJSON化される() {
        let pairs = [
            (FingerprintAction::NotNeeded, "\"not_needed\""),
            (FingerprintAction::Saved, "\"saved\""),
            (FingerprintAction::Cleared, "\"cleared\""),
            (FingerprintAction::ClearFailed, "\"clear_failed\""),
            (FingerprintAction::SaveFailed, "\"save_failed\""),
        ];
        for (action, expected) in pairs {
            let json = serde_json::to_string(&action).expect("serialize");
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn decide_fingerprint_actionはall_ok_trueかつsave成功でsavedを返す() {
        assert_eq!(
            decide_fingerprint_action(true, false, true, true),
            FingerprintAction::Saved
        );
        assert_eq!(
            decide_fingerprint_action(true, true, true, true),
            FingerprintAction::Saved
        );
    }

    #[test]
    fn decide_fingerprint_actionはall_ok_trueかつsave失敗でsave_failedを返す() {
        assert_eq!(
            decide_fingerprint_action(true, false, false, true),
            FingerprintAction::SaveFailed
        );
    }

    #[test]
    fn decide_fingerprint_actionはwarm_partialかつclear成功でclearedを返す() {
        assert_eq!(
            decide_fingerprint_action(false, true, true, true),
            FingerprintAction::Cleared
        );
    }

    #[test]
    fn decide_fingerprint_actionはwarm_partialかつclear失敗でclear_failedを返す() {
        assert_eq!(
            decide_fingerprint_action(false, true, true, false),
            FingerprintAction::ClearFailed
        );
    }

    #[test]
    fn decide_fingerprint_actionはcold_partialでnot_neededを返す() {
        assert_eq!(
            decide_fingerprint_action(false, false, true, true),
            FingerprintAction::NotNeeded
        );
        assert_eq!(
            decide_fingerprint_action(false, false, false, false),
            FingerprintAction::NotNeeded
        );
    }

    #[test]
    fn ScanDiagnosticsのJSONにcancelledフィールドが含まれる() {
        let diag = ScanDiagnostics {
            completed_at_ms: 1,
            is_warm_start: false,
            cleanup_ok: true,
            scans_ok: false,
            all_ok: false,
            cancelled: true,
            fingerprint: FingerprintAction::NotNeeded,
            mounts: vec![MountDiagnostic {
                mount_id: "aaaaaaaaaaaaaaaa".to_string(),
                scan_ok: false,
                dir_index_ok: false,
                panicked: false,
                cancelled: true,
                walk: None,
            }],
        };
        let json = serde_json::to_value(&diag).expect("serialize");
        assert_eq!(json["cancelled"], serde_json::json!(true));
        assert_eq!(json["mounts"][0]["cancelled"], serde_json::json!(true));
    }
}
