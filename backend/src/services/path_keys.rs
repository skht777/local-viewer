//! マウント配下の `{mount_id}/{relative}` キー操作ヘルパー
//!
//! - `entries.relative_path` / `dir_entries.dir_key` は `{mount_id}/{relative}` 形式で
//!   保存されており、どちらも同じ range 境界関数を使う
//! - `services/indexer` と `services/dir_index` の双方から参照されるため、
//!   いずれのモジュールにも属さない独立モジュールとして `services/path_keys` に配置
//! - エラー型 `PathKeyError` は `From<_>` 実装で `IndexerError` / `DirIndexError`
//!   双方に変換できるよう設計

use thiserror::Error;

/// `path_keys` 関数群のエラー型
#[derive(Debug, Error)]
pub(crate) enum PathKeyError {
    /// `mount_id` が 16 桁の lowercase hex という invariant 違反
    #[error("mount_id invariant 違反 (lowercase hex 16 桁): len={0}")]
    InvalidMountId(usize),
}

/// 指定マウント配下の `entries.relative_path` / `dir_entries.dir_key` range `(lo, hi)` を返す
///
/// - 入力: `mount_id` は `build_mount_id()` 生成値 (HMAC-SHA256 先頭 16 桁 lowercase hex)
///   の invariant を要請する
/// - 戻り値: lexicographic な半開区間 `[lo, hi)` = `("{mount_id}/", "{mount_id}0")`
/// - `/` (ASCII 0x2F) の次の文字 `0` (0x30) で終端を閉じ、`{mount_id}/...` 全行を覆う
/// - invariant 違反（空 / 長さ不一致 / 非 hex）は `PathKeyError::InvalidMountId` で reject
///   し、`mount1/photos` 等のネスト prefix 経路との混同を防ぐ
pub(crate) fn mount_scope_range(mount_id: &str) -> Result<(String, String), PathKeyError> {
    fn is_valid(id: &str) -> bool {
        id.len() == 16 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    }
    if !is_valid(mount_id) {
        return Err(PathKeyError::InvalidMountId(mount_id.len()));
    }
    Ok((format!("{mount_id}/"), format!("{mount_id}0")))
}

/// search の `scope_prefix` 用の range `(lo, hi)` を返す
///
/// - 入力: ワイルドカード非含みの literal prefix。末尾 `/` を含めない生の prefix
///   を想定（例: `mount1/photos`, `mount/dir_100%`）
/// - `(lo, hi)` = `(format!("{prefix}/"), format!("{prefix}0"))`
///   - ASCII 順で `/` (0x2F) の次は `0` (0x30)、`{prefix}/` 以降のすべての
///     キーはこの半開区間に収まる（`{prefix}0` は `{prefix}/...` よりも大きい）
/// - `mount_scope_range` と違い、この関数は invariant を課さない
///   （`mount_id` 専用ではないため）
/// - `%` `_` `\` を含む literal prefix でも escape 不要で range に乗る
pub(crate) fn prefix_scope_range(prefix: &str) -> (String, String) {
    (format!("{prefix}/"), format!("{prefix}0"))
}

#[cfg(test)]
#[allow(
    non_snake_case,
    reason = "日本語テスト名の可読性のため PathKeyError を PascalCase 残存"
)]
mod tests {
    use super::*;

    #[test]
    fn mount_scope_rangeは正常mount_idで範囲を返す() {
        let (lo, hi) = mount_scope_range("0123456789abcdef").unwrap();
        assert_eq!(lo, "0123456789abcdef/");
        assert_eq!(hi, "0123456789abcdef0");
    }

    #[test]
    fn mount_scope_rangeは空文字列でPathKeyErrorを返す() {
        let err = mount_scope_range("").unwrap_err();
        assert!(matches!(err, PathKeyError::InvalidMountId(0)));
    }

    #[test]
    fn mount_scope_rangeは長さ不一致でPathKeyErrorを返す() {
        let err = mount_scope_range("abc").unwrap_err();
        assert!(matches!(err, PathKeyError::InvalidMountId(3)));
    }

    #[test]
    fn mount_scope_rangeは非hexでPathKeyErrorを返す() {
        let err = mount_scope_range("gggggggggggggggg").unwrap_err();
        assert!(matches!(err, PathKeyError::InvalidMountId(16)));
    }

    #[test]
    fn prefix_scope_rangeはネストprefixで範囲を返す() {
        let (lo, hi) = prefix_scope_range("mount1/photos");
        assert_eq!(lo, "mount1/photos/");
        assert_eq!(hi, "mount1/photos0");
    }

    #[test]
    fn prefix_scope_rangeは特殊文字を含むprefixで範囲を返す() {
        let (lo, hi) = prefix_scope_range("mount/dir_100%");
        assert_eq!(lo, "mount/dir_100%/");
        assert_eq!(hi, "mount/dir_100%0");
    }
}
