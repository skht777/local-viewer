//! インデックスリビルド / マウントホットリロードのレート制限
//!
//! `AppState::last_rebuild` の `tokio::sync::Mutex<Option<Instant>>` を
//! 入力に、前回コミット時刻から一定秒数経過していなければ `AppError::RateLimited`
//! を返す。
//!
//! 旧 `try_start_rebuild` は「判定 + 更新」を 1 ステップで行うため、guard
//! 取得失敗で 409 を返したいケースでも `last_rebuild` が更新されるレース
//! があった。本モジュールでは `peek` / `commit_now` に分離し、呼び出し側で
//! `guard → peek → 本処理 → commit_now` の順序を制御する。

use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::errors::AppError;

/// 前回コミットからの経過を read-only で検査する
///
/// - `last_rebuild` は更新しない
/// - 超過未満なら `AppError::RateLimited` を返す（呼び出し側で guard を Drop）
/// - 超過済なら `Ok(())`。呼び出し側は本処理を実行し、成功後に `commit_now` を呼ぶ
pub(crate) async fn peek(
    last_rebuild: &Mutex<Option<Instant>>,
    rate_limit_seconds: u64,
) -> Result<(), AppError> {
    let last = last_rebuild.lock().await;
    if let Some(instant) = *last {
        let elapsed = instant.elapsed().as_secs();
        if elapsed < rate_limit_seconds {
            return Err(AppError::RateLimited("レート制限に達しました".to_string()));
        }
    }
    Ok(())
}

/// 本処理成功時に `last_rebuild` を現在時刻で更新する
///
/// - 失敗経路では呼ばない（レート制限が無駄に消費されるのを防ぐ）
pub(crate) async fn commit_now(last_rebuild: &Mutex<Option<Instant>>) {
    let mut last = last_rebuild.lock().await;
    *last = Some(Instant::now());
}
