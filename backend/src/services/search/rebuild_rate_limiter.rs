//! インデックスリビルドのレート制限
//!
//! `AppState::last_rebuild` の `tokio::sync::Mutex<Option<Instant>>` を
//! 入力に、前回実行から一定秒数経過していなければ `AppError::RateLimited` を返す。

use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::errors::AppError;

/// レート制限を判定し、許容される場合は `last_rebuild` を現在時刻で更新する
pub(crate) async fn try_start_rebuild(
    last_rebuild: &Mutex<Option<Instant>>,
    rate_limit_seconds: u64,
) -> Result<(), AppError> {
    let mut last = last_rebuild.lock().await;
    if let Some(instant) = *last {
        let elapsed = instant.elapsed().as_secs();
        if elapsed < rate_limit_seconds {
            return Err(AppError::RateLimited("レート制限に達しました".to_string()));
        }
    }
    *last = Some(Instant::now());
    Ok(())
}
