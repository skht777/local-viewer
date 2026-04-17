//! `GET /api/search?scope=` の `node_id` → ディレクトリプレフィックス解決
//!
//! scope 検証 3 点セット + parent key 生成を 1 関数に集約する:
//! 1. `NodeRegistry::resolve` で `node_id` → 絶対パス
//! 2. `PathSecurity::validate_existing` で存在 + マウント配下検証
//! 3. ディレクトリ判定 (ファイルなら 422)
//! 4. `compute_parent_path_key` で `{mount_id}/{relative}` プレフィックスを生成

use std::sync::{Arc, Mutex};

use crate::errors::AppError;
use crate::services::node_registry::NodeRegistry;

/// scope `node_id` からディレクトリプレフィックスを解決する
///
/// - `scope_node_id` が `None` の場合は `Ok(None)`
/// - 解決成功時は `Ok(Some(prefix))` を返す（`compute_parent_path_key` が `None` の場合も `Ok(None)`）
/// - `NodeRegistry::resolve` 失敗 → `AppError::NotFound`
/// - `PathSecurity::validate_existing` 失敗 → その `AppError`
/// - ディレクトリでない → `AppError::InvalidQuery` (422)
pub(crate) fn resolve_scope_prefix(
    registry: &Arc<Mutex<NodeRegistry>>,
    scope_node_id: Option<&str>,
) -> Result<Option<String>, AppError> {
    let Some(scope_node_id) = scope_node_id else {
        return Ok(None);
    };

    #[allow(
        clippy::expect_used,
        reason = "Mutex poison は致命的エラー、パニックが適切"
    )]
    let reg = registry.lock().expect("NodeRegistry Mutex poisoned");

    // node_id → 絶対パス
    let abs_path = reg.resolve(scope_node_id)?.to_path_buf();

    // `PathSecurity` で存在確認 + マウントポイント配下か検証
    reg.path_security().validate_existing(&abs_path)?;

    // ディレクトリか確認
    if !abs_path.is_dir() {
        return Err(AppError::InvalidQuery(
            "scope はディレクトリの node_id を指定してください".to_string(),
        ));
    }

    // `{mount_id}/{relative}` プレフィックスを算出
    Ok(reg.compute_parent_path_key(&abs_path))
}
