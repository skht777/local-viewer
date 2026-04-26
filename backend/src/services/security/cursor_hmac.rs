//! `NODE_SECRET` ベースの HMAC-SHA256 ヘルパー
//!
//! - 署名対象: カーソル等の短い文字列
//! - 署名形式: HMAC-SHA256 の先頭 16 hex 文字 (64bit ビット強度)
//! - 比較: タイミング攻撃耐性のある定数時間比較
//!
//! `NODE_SECRET` は本番/通常運用で必須の環境変数（`09_security`）。
//! 未設定または空文字の場合は起動時に panic する。テストビルドのみ
//! 固定フォールバック値を許容する。

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// `NODE_SECRET` を環境変数から取得する
///
/// - 設定済み（非空）: その値を返す
/// - 未設定/空: 非テストビルドでは panic、テストビルドでは固定値
pub(crate) fn get_secret() -> Vec<u8> {
    match std::env::var("NODE_SECRET") {
        Ok(s) if !s.is_empty() => s.into_bytes(),
        _ => {
            #[cfg(test)]
            {
                b"local-viewer-default-secret".to_vec()
            }
            #[cfg(not(test))]
            {
                panic!(
                    "NODE_SECRET environment variable is required but not set (or empty). \
                     Set it in .env or export it before starting the server."
                );
            }
        }
    }
}

/// HMAC-SHA256 の先頭 16 hex 文字を返す
pub(crate) fn hmac_hex_16(secret: &[u8], input: &str) -> String {
    #[allow(clippy::expect_used, reason = "HMAC-SHA256 は任意長の鍵を受け付ける")]
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC は任意長の鍵を受け付ける");
    mac.update(input.as_bytes());
    let result = mac.finalize().into_bytes();
    hex::encode(result)[..16].to_string()
}

/// 定数時間比較
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_hex_16は同じ入力で同じ値を返す() {
        let secret = b"test-secret";
        let a = hmac_hex_16(secret, "hello");
        let b = hmac_hex_16(secret, "hello");
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn hmac_hex_16は異なる入力で異なる値を返す() {
        let secret = b"test-secret";
        let a = hmac_hex_16(secret, "hello");
        let b = hmac_hex_16(secret, "world");
        assert_ne!(a, b);
    }

    #[test]
    fn hmac_hex_16は異なる鍵で異なる値を返す() {
        let a = hmac_hex_16(b"secret-a", "payload");
        let b = hmac_hex_16(b"secret-b", "payload");
        assert_ne!(a, b);
    }

    #[test]
    fn constant_time_eqは同一バイト列でtrue() {
        assert!(constant_time_eq(b"abc", b"abc"));
    }

    #[test]
    fn constant_time_eqは異なるバイト列でfalse() {
        assert!(!constant_time_eq(b"abc", b"abd"));
    }

    #[test]
    fn constant_time_eqは長さが違う場合false() {
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
