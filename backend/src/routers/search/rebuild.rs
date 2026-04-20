//! `POST /api/index/rebuild` ハンドラ
//!
//! 順序: `rebuild_guard.try_acquire()` → `rate_limiter.peek()` → 本処理 → `commit_now()`。
//! guard は RAII で `tokio::spawn` 内 task に move、task panic でも Drop で解放される。

use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::errors::AppError;
use crate::services::scan_diagnostics::{
    FingerprintAction, MountDiagnostic, ScanDiagnostics, finalize_scan_success,
};
use crate::services::search::rebuild_rate_limiter;
use crate::state::AppState;

use super::RebuildResponse;

/// `POST /api/index/rebuild`
///
/// インデックスの全件リビルドをバックグラウンドで開始する。
/// rebuild / mount reload 間の全体排他 + レート制限付き。
#[allow(
    clippy::too_many_lines,
    reason = "ハンドラ本体に rebuild task の本処理（per-mount ループ + readiness promote + fingerprint 保存 + last_scan_report 更新）を集約しているため"
)]
pub(crate) async fn rebuild_index(
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    // 1. guard 取得（rebuild / mount reload 間の全体排他）
    let Some(guard) = state.rebuild_guard.try_acquire() else {
        return Err(AppError::RebuildInProgress(
            "リビルド / マウントリロードが実行中です".to_string(),
        ));
    };

    // 2. レート制限の read-only チェック（last_rebuild は更新しない）
    //    失敗時は guard が Drop されて 429 を返す
    rebuild_rate_limiter::peek(
        &state.last_rebuild,
        state.settings.rebuild_rate_limit_seconds,
    )
    .await?;

    // 3. 本処理: バックグラウンドでリビルドを実行
    let indexer = Arc::clone(&state.indexer);
    let registry = Arc::clone(&state.node_registry);
    let state_for_commit = Arc::clone(&state);

    // mount_id_map からリビルド対象のルートを収集
    let mount_entries: Vec<(String, PathBuf)> = {
        #[allow(
            clippy::expect_used,
            reason = "Mutex poison は致命的エラー、パニックが適切"
        )]
        let reg = registry.lock().expect("NodeRegistry Mutex poisoned");
        reg.mount_id_map()
            .iter()
            .map(|(id, root)| (id.clone(), root.clone()))
            .collect()
    };

    tokio::spawn(async move {
        // guard を task 内でバインド（Drop は task 終了時 or panic 時）
        let _guard = guard;
        let mut all_success = true;
        let mut mount_diagnostics: Vec<MountDiagnostic> = Vec::with_capacity(mount_entries.len());

        for (mount_id, root) in &mount_entries {
            let indexer_ref = Arc::clone(&indexer);
            let registry_ref = Arc::clone(&registry);
            let root = root.clone();
            let mount_id_for_task = mount_id.clone();
            let mount_id_for_log = mount_id.clone();

            let result = tokio::task::spawn_blocking(move || {
                #[allow(
                    clippy::expect_used,
                    reason = "Mutex poison は致命的エラー、パニックが適切"
                )]
                let reg = registry_ref.lock().expect("NodeRegistry Mutex poisoned");
                let path_security = reg.path_security();
                indexer_ref.rebuild(&root, path_security, &mount_id_for_task)
            })
            .await;

            let (scan_ok, panicked) = match result {
                Ok(Ok(count)) => {
                    tracing::info!(
                        "インデックスリビルド完了: mount_id={mount_id_for_log}, entries={count}"
                    );
                    (true, false)
                }
                Ok(Err(e)) => {
                    all_success = false;
                    tracing::error!(
                        "インデックスリビルドエラー: mount_id={mount_id_for_log}, error={e}"
                    );
                    (false, false)
                }
                Err(e) => {
                    all_success = false;
                    tracing::error!(
                        "リビルドタスク実行エラー: mount_id={mount_id_for_log}, error={e}"
                    );
                    (false, true)
                }
            };

            mount_diagnostics.push(MountDiagnostic {
                mount_id: mount_id.clone(),
                scan_ok,
                // rebuild は DirIndex を触らず scan 成否が全てを決めるため同値扱い
                dir_index_ok: scan_ok,
                panicked,
                // rebuild 経路は WalkReport を集計しないため walk metrics は常に None
                walk: None,
            });
        }

        // 成功時のみ readiness を昇格し last_scan_report を更新
        //   cold partial init (scan_complete=false 固定) 状態でも rebuild が all_ok で
        //   完了すれば /api/ready=200 に復旧する（Phase C の目的）

        // 成功時は fingerprint を保存（次回起動での warm start 有効化）
        let fingerprint_action = if all_success {
            let mount_id_refs: Vec<&str> =
                mount_entries.iter().map(|(id, _)| id.as_str()).collect();
            match state_for_commit
                .indexer
                .save_mount_fingerprint(&mount_id_refs)
            {
                Ok(()) => FingerprintAction::Saved,
                Err(e) => {
                    tracing::error!("rebuild: fingerprint 保存失敗: {e}");
                    FingerprintAction::SaveFailed
                }
            }
        } else {
            FingerprintAction::NotNeeded
        };

        #[allow(
            clippy::cast_possible_truncation,
            reason = "UNIX epoch ms が u64 を超えることは現実的に存在しない"
        )]
        let completed_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        let diagnostics = ScanDiagnostics {
            completed_at_ms,
            // rebuild は cold start 相当のフルスキャンで動作（warm ではない）
            is_warm_start: false,
            cleanup_ok: true,
            scans_ok: all_success,
            all_ok: all_success,
            fingerprint: fingerprint_action,
            mounts: mount_diagnostics,
        };

        // finalize_scan_success は all_ok のときのみ readiness promote する（bootstrap と同じ手順）
        finalize_scan_success(
            &state_for_commit.dir_index,
            &state_for_commit.scan_complete,
            &state_for_commit.last_scan_report,
            diagnostics,
        );

        // 本処理成功時のみ last_rebuild を commit（失敗時はレート制限消費せず再試行可）
        if all_success {
            rebuild_rate_limiter::commit_now(&state_for_commit.last_rebuild).await;
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(RebuildResponse {
            message: "リビルドを開始しました".to_string(),
        }),
    )
        .into_response())
}
