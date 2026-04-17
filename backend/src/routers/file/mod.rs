//! ファイル配信 API
//!
//! `GET /api/file/{node_id}` — ファイル配信 (Range 対応, ETag/Cache-Control 付き)
//!
//! - 通常ファイル: `regular` サブモジュールで処理
//! - アーカイブエントリ: `archive_entry` サブモジュールで処理

mod archive_entry;
mod regular;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::Request;
use axum::response::Response;

use crate::errors::AppError;
use crate::state::AppState;

/// `node_id` 解決結果
enum ResolveResult {
    /// 通常ファイル
    File(PathBuf),
    /// アーカイブ内エントリ
    ArchiveEntry {
        archive_path: PathBuf,
        entry_name: String,
    },
}

/// ファイルまたはアーカイブエントリを配信する
///
/// - 通常ファイル: `regular::serve_regular_file` で配信 (Range 自動処理)
/// - アーカイブエントリ: `archive_entry::serve_archive_entry` で処理
/// - ディレクトリ: 422 `NOT_A_FILE`
pub(crate) async fn serve_file(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
    req: Request<axum::body::Body>,
) -> Result<Response, AppError> {
    let registry = Arc::clone(&state.node_registry);
    let original_headers = req.headers().clone();
    let original_uri = req.uri().clone();

    // spawn_blocking 内で node_id 解決 + アーカイブエントリ判定
    let resolve_result = tokio::task::spawn_blocking({
        let nid = node_id.clone();
        move || -> Result<ResolveResult, AppError> {
            let mut reg = registry
                .lock()
                .map_err(|e| AppError::path_security(format!("ロック取得失敗: {e}")))?;

            // アーカイブエントリかチェック
            if let Some((archive_path, entry_name)) = reg.resolve_archive_entry(&nid) {
                return Ok(ResolveResult::ArchiveEntry {
                    archive_path,
                    entry_name,
                });
            }

            let path = reg.resolve(&nid)?.to_path_buf();
            Ok(ResolveResult::File(path))
        }
    })
    .await
    .map_err(|e| AppError::path_security(format!("タスク実行失敗: {e}")))??;

    match resolve_result {
        ResolveResult::ArchiveEntry {
            archive_path,
            entry_name,
        } => {
            archive_entry::serve_archive_entry(
                &state,
                &archive_path,
                &entry_name,
                &original_headers,
                &original_uri,
            )
            .await
        }
        ResolveResult::File(file_path) => {
            regular::serve_regular_file(&state, file_path, &original_headers, &original_uri).await
        }
    }
}
