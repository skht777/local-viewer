"""ファイル拡張子定数と種別判定.

- 拡張子 → EntryKind のマッピング
- 頻出拡張子 → MIME タイプの辞書
- 複数モジュール (node_registry, archive_security, archive_service, file) で共有
"""

from enum import StrEnum

# 拡張子 → EntryKind のマッピング
IMAGE_EXTENSIONS = frozenset(
    {".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".avif"}
)
VIDEO_EXTENSIONS = frozenset({".mp4", ".webm", ".mkv", ".avi", ".mov"})
# ブラウザ非対応の動画コンテナ (MP4 への remux 対象)
REMUX_EXTENSIONS = frozenset({".mkv"})
ARCHIVE_EXTENSIONS = frozenset({".zip", ".rar", ".7z", ".cbz", ".cbr"})
PDF_EXTENSIONS = frozenset({".pdf"})

# サーバーサイドサムネイル���成対象 (画像 + アーカイブ + PDF)
THUMBNAIL_EXTENSIONS = IMAGE_EXTENSIONS | ARCHIVE_EXTENSIONS | PDF_EXTENSIONS

# 頻出拡張子 → MIME タイプ (辞書参照で高速化、未知は mimetypes にフォールバック)
MIME_MAP: dict[str, str] = {
    ".jpg": "image/jpeg",
    ".jpeg": "image/jpeg",
    ".png": "image/png",
    ".gif": "image/gif",
    ".webp": "image/webp",
    ".bmp": "image/bmp",
    ".avif": "image/avif",
    ".mp4": "video/mp4",
    ".webm": "video/webm",
    ".mkv": "video/x-matroska",
    ".avi": "video/x-msvideo",
    ".mov": "video/quicktime",
    ".pdf": "application/pdf",
    ".zip": "application/zip",
    ".rar": "application/vnd.rar",
    ".7z": "application/x-7z-compressed",
    ".cbz": "application/vnd.comicbook+zip",
    ".cbr": "application/vnd.comicbook-rar",
}


class EntryKind(StrEnum):
    """エントリの種類."""

    DIRECTORY = "directory"
    IMAGE = "image"
    VIDEO = "video"
    PDF = "pdf"
    ARCHIVE = "archive"
    OTHER = "other"
