//! `POST /api/index/rebuild` ハンドラ
//!
//! レート制限と同時実行チェックを通過後、バックグラウンドでリビルドを開始する。

use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::errors::AppError;
use crate::services::search::rebuild_rate_limiter::try_start_rebuild;
use crate::state::AppState;

use super::RebuildResponse;

/// `POST /api/index/rebuild`
///
/// インデックスの全件リビルドをバックグラウンドで開始する。
/// 同時実行制御 + レート制限付き。
pub(crate) async fn rebuild_index(
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    // リビルド実行中チェック
    if state.indexer.is_rebuilding() {
        return Err(AppError::RebuildInProgress(
            "リビルドが実行中です".to_string(),
        ));
    }

    // レート制限チェック
    try_start_rebuild(
        &state.last_rebuild,
        state.settings.rebuild_rate_limit_seconds,
    )
    .await?;

    // バックグラウンドでリビルドを実行
    let indexer = Arc::clone(&state.indexer);
    let registry = Arc::clone(&state.node_registry);

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
                    tracing::error!(
                        "インデックスリビルドエラー: mount_id={mount_id_for_log}, error={e}"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "リビルドタスク実行エラー: mount_id={mount_id_for_log}, error={e}"
                    );
                }
            }
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
