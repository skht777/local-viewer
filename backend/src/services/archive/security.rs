//! アーカイブエントリの安全性検証
//!
//! - エントリ名: traversal, 絶対パス, NUL バイトを拒否
//! - バックスラッシュを `/` に正規化 (Windows 生成アーカイブ互換)
//! - 拡張子ホワイトリスト (画像 + 動画 + PDF)
//! - zip bomb 検出 (展開後サイズ上限、圧縮率上限)
//! - 動画エントリには画像とは別のサイズ上限を適用

use crate::config::Settings;
use crate::errors::AppError;
use crate::services::extensions::{IMAGE_EXTENSIONS, PDF_EXTENSIONS, VIDEO_EXTENSIONS};

/// アーカイブエントリの安全性バリデータ
///
/// Settings からサイズ上限・圧縮率上限を受け取り、
/// エントリ名・サイズ・合計サイズの検証メソッドを提供する。
#[allow(
    clippy::struct_field_names,
    reason = "max_ プレフィックスが意味的に適切"
)]
pub(crate) struct ArchiveEntryValidator {
    max_total_size: u64,
    max_entry_size: u64,
    max_video_entry_size: u64,
    max_ratio: f64,
}

impl ArchiveEntryValidator {
    pub(crate) fn new(settings: &Settings) -> Self {
        Self {
            max_total_size: settings.archive_max_total_size,
            max_entry_size: settings.archive_max_entry_size,
            max_video_entry_size: settings.archive_max_video_entry_size,
            max_ratio: settings.archive_max_ratio,
        }
    }

    /// エントリ名に応じたサイズ上限を返す (動画/PDF は別上限)
    pub(crate) fn max_entry_size_for(&self, name: &str) -> u64 {
        if is_video_extension(name) || is_pdf_extension(name) {
            self.max_video_entry_size
        } else {
            self.max_entry_size
        }
    }

    /// エントリ名を検証する
    ///
    /// - NUL バイト拒否
    /// - バックスラッシュを `/` に正規化
    /// - 絶対パス拒否
    /// - `..` トラバーサル拒否
    pub(crate) fn validate_entry_name(name: &str) -> Result<(), AppError> {
        // NUL バイト拒否
        if name.contains('\0') {
            return Err(AppError::ArchiveSecurity(
                "NUL バイトを含むエントリ名です".to_string(),
            ));
        }

        // バックスラッシュを / に正規化 (Windows 生成アーカイブ互換)
        let normalized = name.replace('\\', "/");

        // 絶対パス拒否
        if normalized.starts_with('/') {
            return Err(AppError::ArchiveSecurity(
                "絶対パスのエントリ名です".to_string(),
            ));
        }

        // トラバーサル拒否 (..)
        for part in normalized.split('/') {
            if part == ".." {
                return Err(AppError::ArchiveSecurity(
                    "トラバーサルを含むエントリ名です".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// 1エントリのサイズと圧縮率を検証する
    ///
    /// - 展開後サイズが上限を超えていないか
    /// - 圧縮率が上限 (100:1) を超えていないか
    /// - compressed=0 のケースは合法 (無圧縮の空ファイル)
    pub(crate) fn validate_entry_size(
        &self,
        compressed: u64,
        uncompressed: u64,
        name: &str,
    ) -> Result<(), AppError> {
        // エントリ名に応じたサイズ上限を選択
        let max_size = if name.is_empty() {
            self.max_entry_size
        } else {
            self.max_entry_size_for(name)
        };

        if uncompressed > max_size {
            return Err(AppError::ArchiveSecurity(format!(
                "エントリサイズが上限を超えています: {uncompressed} > {max_size}"
            )));
        }

        // 圧縮率上限 (compressed=0 は合法: 無圧縮の空ファイル)
        if compressed > 0 {
            #[allow(clippy::cast_precision_loss, reason = "サイズ比較の精度で十分")]
            let ratio = uncompressed as f64 / compressed as f64;
            if ratio > self.max_ratio {
                return Err(AppError::ArchiveSecurity(format!(
                    "圧縮率が上限を超えています: {ratio:.1} > {}",
                    self.max_ratio
                )));
            }
        }

        Ok(())
    }

    /// アーカイブ全体の展開後サイズを検証する
    pub(crate) fn validate_total_size(&self, total_uncompressed: u64) -> Result<(), AppError> {
        if total_uncompressed > self.max_total_size {
            return Err(AppError::ArchiveSecurity(format!(
                "合計サイズが上限を超えています: {total_uncompressed} > {}",
                self.max_total_size
            )));
        }
        Ok(())
    }

    /// 許可拡張子かどうかを判定する (画像 + 動画 + PDF)
    pub(crate) fn is_allowed_extension(name: &str) -> bool {
        let Some(dot_idx) = name.rfind('.') else {
            return false;
        };
        // 隠しファイル (.bashrc) を除外
        if dot_idx == 0 {
            return false;
        }
        let ext = name[dot_idx..].to_ascii_lowercase();
        IMAGE_EXTENSIONS.contains(&ext.as_str())
            || VIDEO_EXTENSIONS.contains(&ext.as_str())
            || PDF_EXTENSIONS.contains(&ext.as_str())
    }
}

/// PDF 拡張子かどうかを判定する (サイズ上限判定で使用)
fn is_pdf_extension(name: &str) -> bool {
    let Some(dot_idx) = name.rfind('.') else {
        return false;
    };
    if dot_idx == 0 {
        return false;
    }
    let ext = name[dot_idx..].to_ascii_lowercase();
    PDF_EXTENSIONS.contains(&ext.as_str())
}

/// 動画拡張子かどうかを判定する (キャッシュバイパス判定で使用)
pub(crate) fn is_video_extension(name: &str) -> bool {
    let Some(dot_idx) = name.rfind('.') else {
        return false;
    };
    if dot_idx == 0 {
        return false;
    }
    let ext = name[dot_idx..].to_ascii_lowercase();
    VIDEO_EXTENSIONS.contains(&ext.as_str())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rstest::rstest;

    use super::*;

    fn test_validator() -> ArchiveEntryValidator {
        let settings = Settings::from_map(&HashMap::from([(
            "MOUNT_BASE_DIR".to_string(),
            "/tmp".to_string(),
        )]))
        .unwrap();
        ArchiveEntryValidator::new(&settings)
    }

    // --- validate_entry_name ---

    #[rstest]
    #[case("image01.jpg")]
    #[case("dir/image01.jpg")]
    #[case("深い/パス/画像.png")]
    #[case("-leading-hyphen.jpg")]
    fn 正常なエントリ名が検証を通過する(#[case] name: &str) {
        assert!(ArchiveEntryValidator::validate_entry_name(name).is_ok());
    }

    #[test]
    fn nulバイトを含むエントリ名がエラーになる() {
        let result = ArchiveEntryValidator::validate_entry_name("file\x00.jpg");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("NUL"));
    }

    #[rstest]
    #[case("/etc/passwd")]
    #[case("/absolute/path.jpg")]
    fn 絶対パスのエントリ名がエラーになる(#[case] name: &str) {
        let result = ArchiveEntryValidator::validate_entry_name(name);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("絶対パス"));
    }

    #[rstest]
    #[case("../secret.txt")]
    #[case("dir/../../../etc/passwd")]
    #[case("dir/..")]
    fn トラバーサルを含むエントリ名がエラーになる(#[case] name: &str) {
        let result = ArchiveEntryValidator::validate_entry_name(name);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("トラバーサル"));
    }

    #[test]
    fn バックスラッシュがスラッシュに正規化される() {
        // Windows 形式のパスは正規化後に検証を通過する
        assert!(ArchiveEntryValidator::validate_entry_name("dir\\file.jpg").is_ok());
    }

    #[test]
    fn バックスラッシュ正規化後のトラバーサルも拒否される() {
        let result = ArchiveEntryValidator::validate_entry_name("dir\\..\\secret.txt");
        assert!(result.is_err());
    }

    #[test]
    fn 先頭がハイフンのエントリ名が安全に扱われる() {
        // CLI injection 防止: ハイフン始まりでもエントリ名として有効
        assert!(ArchiveEntryValidator::validate_entry_name("-rf.jpg").is_ok());
        assert!(ArchiveEntryValidator::validate_entry_name("--version").is_ok());
    }

    // --- validate_entry_size ---

    #[test]
    fn 制限内のサイズが検証を通過する() {
        let v = test_validator();
        assert!(v.validate_entry_size(1000, 2000, "image.jpg").is_ok());
    }

    #[test]
    fn 画像エントリのサイズ上限超過がエラーになる() {
        let v = test_validator();
        // デフォルト上限: 32MB
        let result = v.validate_entry_size(1000, 33 * 1024 * 1024, "image.jpg");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("エントリサイズが上限を超えています"));
    }

    #[test]
    fn 動画エントリに動画用上限が適用される() {
        let v = test_validator();
        // 画像上限 (32MB) < テストサイズ (100MB) < 動画上限 (500MB)
        assert!(
            v.validate_entry_size(50 * 1024 * 1024, 100 * 1024 * 1024, "video.mp4")
                .is_ok()
        );
    }

    #[test]
    fn 圧縮率超過がエラーになる() {
        let v = test_validator();
        // 圧縮率 200:1 > 上限 100:1
        let result = v.validate_entry_size(100, 20_000, "image.jpg");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("圧縮率が上限を超えています"));
    }

    #[test]
    fn compressed_0の場合に圧縮率チェックをスキップする() {
        let v = test_validator();
        // compressed=0 は合法 (無圧縮の空ファイル)
        assert!(v.validate_entry_size(0, 1000, "image.jpg").is_ok());
    }

    // --- validate_total_size ---

    #[test]
    fn 制限内の合計サイズが検証を通過する() {
        let v = test_validator();
        assert!(v.validate_total_size(500 * 1024 * 1024).is_ok());
    }

    #[test]
    fn 合計サイズ上限超過がエラーになる() {
        let v = test_validator();
        // デフォルト上限: 1GB
        let result = v.validate_total_size(2 * 1024 * 1024 * 1024);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("合計サイズが上限を超えています"));
    }

    // --- is_allowed_extension ---

    #[rstest]
    #[case("image.jpg", true)]
    #[case("image.jpeg", true)]
    #[case("image.png", true)]
    #[case("image.gif", true)]
    #[case("image.webp", true)]
    #[case("image.bmp", true)]
    #[case("image.avif", true)]
    #[case("video.mp4", true)]
    #[case("video.webm", true)]
    #[case("video.mkv", true)]
    #[case("document.pdf", true)]
    #[case("readme.txt", false)]
    #[case("program.exe", false)]
    #[case(".bashrc", false)]
    #[case("noext", false)]
    fn 拡張子フィルタが正しく判定される(
        #[case] name: &str,
        #[case] expected: bool,
    ) {
        assert_eq!(ArchiveEntryValidator::is_allowed_extension(name), expected);
    }

    #[test]
    fn 大文字拡張子が許可される() {
        assert!(ArchiveEntryValidator::is_allowed_extension("IMAGE.JPG"));
        assert!(ArchiveEntryValidator::is_allowed_extension("Photo.PNG"));
    }

    // --- max_entry_size_for ---

    #[test]
    fn 画像エントリに画像用上限を返す() {
        let v = test_validator();
        assert_eq!(v.max_entry_size_for("image.jpg"), 32 * 1024 * 1024);
    }

    #[test]
    fn 動画エントリに動画用上限を返す() {
        let v = test_validator();
        assert_eq!(v.max_entry_size_for("video.mp4"), 500 * 1024 * 1024);
    }

    #[test]
    fn pdfエントリに動画用上限を返す() {
        let v = test_validator();
        assert_eq!(v.max_entry_size_for("document.pdf"), 500 * 1024 * 1024);
    }

    // --- is_video_extension ---

    #[rstest]
    #[case("video.mp4", true)]
    #[case("video.webm", true)]
    #[case("video.mkv", true)]
    #[case("image.jpg", false)]
    #[case("noext", false)]
    #[case(".hidden", false)]
    fn 動画拡張子判定が正しく動作する(#[case] name: &str, #[case] expected: bool) {
        assert_eq!(is_video_extension(name), expected);
    }
}
