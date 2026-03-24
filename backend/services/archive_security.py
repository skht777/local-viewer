"""アーカイブエントリの安全性検証.

- エントリ名の不正パターン拒否(traversal, 絶対パス, NUL バイト)
- バックスラッシュは / に正規化してから検証(Windows 生成アーカイブ互換)
- 拡張子ホワイトリストによるファイル種別制限(Phase 4 は画像のみ)
- zip bomb 検出(展開後サイズ、圧縮率)
"""

from pathlib import PurePosixPath

from backend.config import Settings
from backend.services.node_registry import IMAGE_EXTENSIONS


class ArchiveSecurityError(Exception):
    """アーカイブセキュリティ違反."""

    def __init__(self, message: str = "アーカイブのセキュリティ違反です") -> None:
        self.message = message
        super().__init__(message)


class ArchivePasswordError(Exception):
    """パスワード付きアーカイブの検出."""

    def __init__(self, message: str = "パスワード付きアーカイブは未対応です") -> None:
        self.message = message
        super().__init__(message)


class ArchiveEntryValidator:
    """アーカイブエントリの安全性を検証する.

    - validate_entry_name: エントリ名の traversal/絶対パス/NUL バイトを検証
    - validate_entry_size: 1エントリのサイズと圧縮率を検証
    - validate_total_size: アーカイブ全体の展開後サイズを検証
    - is_allowed_extension: 許可拡張子かどうかを判定
    """

    def __init__(self, settings: Settings) -> None:
        self._max_total_size = settings.archive_max_total_size
        self._max_entry_size = settings.archive_max_entry_size
        self._max_ratio = settings.archive_max_ratio

    @property
    def max_entry_size(self) -> int:
        """1エントリの展開後サイズ上限 (bytes)."""
        return self._max_entry_size

    def validate_entry_name(self, name: str) -> None:
        """エントリ名を検証する。不正なら ArchiveSecurityError."""
        # NUL バイト拒否
        if "\x00" in name:
            msg = "NUL バイトを含むエントリ名です"
            raise ArchiveSecurityError(msg)

        # バックスラッシュを / に正規化(Windows 生成アーカイブ互換)
        normalized = name.replace("\\", "/")

        # PurePosixPath で検証
        path = PurePosixPath(normalized)

        # 絶対パス拒否
        if path.is_absolute():
            msg = "絶対パスのエントリ名です"
            raise ArchiveSecurityError(msg)

        # トラバーサル拒否 (..)
        if ".." in path.parts:
            msg = "トラバーサルを含むエントリ名です"
            raise ArchiveSecurityError(msg)

    def validate_entry_size(self, *, compressed: int, uncompressed: int) -> None:
        """1エントリのサイズと圧縮率を検証する."""
        # エントリサイズ上限
        if uncompressed > self._max_entry_size:
            msg = (
                f"エントリサイズが上限を超えています: "
                f"{uncompressed} > {self._max_entry_size}"
            )
            raise ArchiveSecurityError(msg)

        # 圧縮率上限(compressed=0 のケースは合法: 無圧縮で空ファイル)
        if compressed > 0:
            ratio = uncompressed / compressed
            if ratio > self._max_ratio:
                msg = f"圧縮率が上限を超えています: {ratio:.1f} > {self._max_ratio}"
                raise ArchiveSecurityError(msg)

    def validate_total_size(self, total_uncompressed: int) -> None:
        """アーカイブ全体の展開後サイズを検証する."""
        if total_uncompressed > self._max_total_size:
            msg = (
                f"合計サイズが上限を超えています: "
                f"{total_uncompressed} > {self._max_total_size}"
            )
            raise ArchiveSecurityError(msg)

    def is_allowed_extension(self, name: str) -> bool:
        """許可拡張子かどうかを判定する(Phase 4: 画像のみ)."""
        dot_idx = name.rfind(".")
        if dot_idx <= 0:
            return False
        ext = name[dot_idx:].lower()
        return ext in IMAGE_EXTENSIONS
