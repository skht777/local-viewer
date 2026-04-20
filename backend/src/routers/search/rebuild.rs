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
use crate::services::search::rebuild_rate_limiter;
use crate::state::AppState;

use super::RebuildResponse;

/// `POST /api/index/rebuild`
///
/// インデックスの全件リビルドをバックグラウンドで開始する。
/// rebuild / mount reload 間の全体排他 + レート制限付き。
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

            match result {
                Ok(Ok(count)) => {
                    tracing::info!(
                        "インデックスリビルド完了: mount_id={mount_id_for_log}, entries={count}"
                    );
                }
                Ok(Err(e)) => {
                    all_success = false;
                    tracing::error!(
                        "インデックスリビルドエラー: mount_id={mount_id_for_log}, error={e}"
                    );
                }
                Err(e) => {
                    all_success = false;
                    tracing::error!(
                        "リビルドタスク実行エラー: mount_id={mount_id_for_log}, error={e}"
                    );
                }
            }
        }

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
