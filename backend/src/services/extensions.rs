//! ファイル拡張子定数と種別判定
//!
//! - 拡張子 → `EntryKind` のマッピング
//! - 頻出拡張子 → MIME タイプの辞書
//! - 複数モジュール (`node_registry`, `archive_security`, `file`) で共有

use serde::{Deserialize, Serialize};

// 拡張子セット (小文字、ドット付き)
pub(crate) const IMAGE_EXTENSIONS: &[&str] =
    &[".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".avif"];
pub(crate) const VIDEO_EXTENSIONS: &[&str] = &[".mp4", ".webm", ".mkv", ".avi", ".mov"];
pub(crate) const ARCHIVE_EXTENSIONS: &[&str] = &[".zip", ".rar", ".7z", ".cbz", ".cbr"];
pub(crate) const PDF_EXTENSIONS: &[&str] = &[".pdf"];
// ブラウザ非対応の動画コンテナ (MP4 への remux 対象)
pub(crate) const REMUX_EXTENSIONS: &[&str] = &[".mkv"];

/// サーバーサイドサムネイル生成対象かどうかを判定する
///
/// 画像 + アーカイブ + PDF + 動画 が対象
pub(crate) fn is_thumbnail_extension(ext: &str) -> bool {
    IMAGE_EXTENSIONS.contains(&ext)
        || ARCHIVE_EXTENSIONS.contains(&ext)
        || PDF_EXTENSIONS.contains(&ext)
        || VIDEO_EXTENSIONS.contains(&ext)
}

/// 拡張子から MIME タイプを返す
///
/// 頻出 18 エントリを高速判定。未知の拡張子は None
pub(crate) fn mime_for_extension(ext: &str) -> Option<&'static str> {
    match ext {
        ".jpg" | ".jpeg" => Some("image/jpeg"),
        ".png" => Some("image/png"),
        ".gif" => Some("image/gif"),
        ".webp" => Some("image/webp"),
        ".bmp" => Some("image/bmp"),
        ".avif" => Some("image/avif"),
        ".mp4" => Some("video/mp4"),
        ".webm" => Some("video/webm"),
        ".mkv" => Some("video/x-matroska"),
        ".avi" => Some("video/x-msvideo"),
        ".mov" => Some("video/quicktime"),
        ".pdf" => Some("application/pdf"),
        ".zip" => Some("application/zip"),
        ".rar" => Some("application/vnd.rar"),
        ".7z" => Some("application/x-7z-compressed"),
        ".cbz" => Some("application/vnd.comicbook+zip"),
        ".cbr" => Some("application/vnd.comicbook-rar"),
        _ => None,
    }
}

/// エントリの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum EntryKind {
    Directory,
    Image,
    Video,
    Pdf,
    Archive,
    Other,
}

impl EntryKind {
    /// 拡張子 (小文字、ドット付き) から `EntryKind` を判定する
    pub(crate) fn from_extension(ext: &str) -> Self {
        if IMAGE_EXTENSIONS.contains(&ext) {
            Self::Image
        } else if VIDEO_EXTENSIONS.contains(&ext) {
            Self::Video
        } else if PDF_EXTENSIONS.contains(&ext) {
            Self::Pdf
        } else if ARCHIVE_EXTENSIONS.contains(&ext) {
            Self::Archive
        } else {
            Self::Other
        }
    }
}

/// ファイル名から拡張子を安全に取得するヘルパー
///
/// `dot_idx` > 0 で隠しファイル (.bashrc 等) の誤認を防止
pub(crate) fn extract_extension(name: &str) -> &str {
    match name.rfind('.') {
        Some(idx) if idx > 0 => &name[idx..],
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    // --- EntryKind::from_extension ---

    #[rstest]
    #[case(".jpg", EntryKind::Image)]
    #[case(".jpeg", EntryKind::Image)]
    #[case(".png", EntryKind::Image)]
    #[case(".gif", EntryKind::Image)]
    #[case(".webp", EntryKind::Image)]
    #[case(".bmp", EntryKind::Image)]
    #[case(".avif", EntryKind::Image)]
    fn 画像拡張子が正しく判定される(#[case] ext: &str, #[case] expected: EntryKind) {
        assert_eq!(EntryKind::from_extension(ext), expected);
    }

    #[rstest]
    #[case(".mp4", EntryKind::Video)]
    #[case(".webm", EntryKind::Video)]
    #[case(".mkv", EntryKind::Video)]
    #[case(".avi", EntryKind::Video)]
    #[case(".mov", EntryKind::Video)]
    fn 動画拡張子が正しく判定される(#[case] ext: &str, #[case] expected: EntryKind) {
        assert_eq!(EntryKind::from_extension(ext), expected);
    }

    #[rstest]
    #[case(".zip", EntryKind::Archive)]
    #[case(".rar", EntryKind::Archive)]
    #[case(".7z", EntryKind::Archive)]
    #[case(".cbz", EntryKind::Archive)]
    #[case(".cbr", EntryKind::Archive)]
    fn アーカイブ拡張子が正しく判定される(
        #[case] ext: &str,
        #[case] expected: EntryKind,
    ) {
        assert_eq!(EntryKind::from_extension(ext), expected);
    }

    #[test]
    fn pdf拡張子が正しく判定される() {
        assert_eq!(EntryKind::from_extension(".pdf"), EntryKind::Pdf);
    }

    #[test]
    fn 未知の拡張子がotherを返す() {
        assert_eq!(EntryKind::from_extension(".txt"), EntryKind::Other);
        assert_eq!(EntryKind::from_extension(""), EntryKind::Other);
    }

    // --- mime_for_extension ---

    #[rstest]
    #[case(".jpg", "image/jpeg")]
    #[case(".jpeg", "image/jpeg")]
    #[case(".png", "image/png")]
    #[case(".gif", "image/gif")]
    #[case(".webp", "image/webp")]
    #[case(".bmp", "image/bmp")]
    #[case(".avif", "image/avif")]
    #[case(".mp4", "video/mp4")]
    #[case(".webm", "video/webm")]
    #[case(".mkv", "video/x-matroska")]
    #[case(".avi", "video/x-msvideo")]
    #[case(".mov", "video/quicktime")]
    #[case(".pdf", "application/pdf")]
    #[case(".zip", "application/zip")]
    #[case(".rar", "application/vnd.rar")]
    #[case(".7z", "application/x-7z-compressed")]
    #[case(".cbz", "application/vnd.comicbook+zip")]
    #[case(".cbr", "application/vnd.comicbook-rar")]
    fn mime_mapの全エントリが正しい(#[case] ext: &str, #[case] expected: &str) {
        assert_eq!(mime_for_extension(ext), Some(expected));
    }

    #[test]
    fn 未知の拡張子でnoneを返す() {
        assert_eq!(mime_for_extension(".txt"), None);
        assert_eq!(mime_for_extension(""), None);
    }

    // --- EntryKind serde ---

    #[test]
    fn entrykindのシリアライズが小文字() {
        assert_eq!(
            serde_json::to_string(&EntryKind::Directory).unwrap(),
            r#""directory""#
        );
        assert_eq!(
            serde_json::to_string(&EntryKind::Image).unwrap(),
            r#""image""#
        );
        assert_eq!(serde_json::to_string(&EntryKind::Pdf).unwrap(), r#""pdf""#);
    }

    // --- is_thumbnail_extension ---

    #[rstest]
    #[case(".jpg", true)]
    #[case(".zip", true)]
    #[case(".pdf", true)]
    #[case(".mp4", true)]
    #[case(".txt", false)]
    fn サムネイル対象拡張子が正しく判定される(
        #[case] ext: &str,
        #[case] expected: bool,
    ) {
        assert_eq!(is_thumbnail_extension(ext), expected);
    }

    // --- REMUX_EXTENSIONS ---

    #[test]
    fn remux_extensionsがmkvを含む() {
        assert!(REMUX_EXTENSIONS.contains(&".mkv"));
    }

    // --- extract_extension ---

    #[rstest]
    #[case("file.jpg", ".jpg")]
    #[case("archive.tar.gz", ".gz")]
    #[case(".bashrc", "")]
    #[case("noext", "")]
    fn 拡張子抽出が正しく動作する(#[case] name: &str, #[case] expected: &str) {
        assert_eq!(extract_extension(name), expected);
    }
}
