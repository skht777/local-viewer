"""アーカイブ読み取りの抽象インターフェースと ZIP 実装.

- ArchiveReader ABC: list_entries, extract_entry, supports
- ZipArchiveReader: zipfile.ZipFile でダイレクト読み取り
- エントリのフィルタ/ソート/セキュリティ検証を内包
"""

import zipfile
from abc import ABC, abstractmethod
from dataclasses import dataclass
from pathlib import Path

from backend.services.archive_security import (
    ArchiveEntryValidator,
    ArchivePasswordError,
)


@dataclass(frozen=True)
class ArchiveEntry:
    """アーカイブ内の1エントリ."""

    name: str  # フルパス (例: "dir/image01.jpg")
    size_compressed: int
    size_uncompressed: int
    is_dir: bool


class ArchiveReader(ABC):
    """アーカイブ読み取りの抽象インターフェース."""

    @abstractmethod
    def list_entries(self, archive_path: Path) -> list[ArchiveEntry]:
        """エントリ一覧を返す (許可拡張子のみ、ソート済み)."""

    @abstractmethod
    def extract_entry(self, archive_path: Path, entry_name: str) -> bytes:
        """エントリのバイナリデータを読み取る."""

    @abstractmethod
    def supports(self, path: Path) -> bool:
        """このリーダーが対応する拡張子か."""


class ZipArchiveReader(ArchiveReader):
    """ZIP/CBZ アーカイブリーダー.

    - zipfile.ZipFile で InfoList を取得
    - ArchiveEntryValidator でセキュリティ検証
    - ディレクトリエントリを除外、許可拡張子のみフィルタ
    - フルパスで大文字小文字無視ソート
    """

    _EXTENSIONS = frozenset({".zip", ".cbz"})

    def __init__(self, validator: ArchiveEntryValidator) -> None:
        self._validator = validator

    def supports(self, path: Path) -> bool:
        return path.suffix.lower() in self._EXTENSIONS

    def list_entries(self, archive_path: Path) -> list[ArchiveEntry]:
        with zipfile.ZipFile(archive_path, "r") as zf:
            # パスワード付き検出
            for info in zf.infolist():
                if info.flag_bits & 0x1:
                    raise ArchivePasswordError()

            entries: list[ArchiveEntry] = []
            total_uncompressed = 0

            for info in zf.infolist():
                # ディレクトリエントリ除外
                if info.is_dir():
                    continue

                # バックスラッシュを正規化
                name = info.filename.replace("\\", "/")

                # エントリ名セキュリティ検証
                self._validator.validate_entry_name(name)

                # 許可拡張子チェック
                if not self._validator.is_allowed_extension(name):
                    continue

                # サイズ検証
                self._validator.validate_entry_size(
                    compressed=info.compress_size,
                    uncompressed=info.file_size,
                )
                total_uncompressed += info.file_size

                entries.append(
                    ArchiveEntry(
                        name=name,
                        size_compressed=info.compress_size,
                        size_uncompressed=info.file_size,
                        is_dir=False,
                    )
                )

            # 合計サイズ検証
            self._validator.validate_total_size(total_uncompressed)

        # フルパスで大文字小文字無視ソート
        entries.sort(key=lambda e: e.name.lower())
        return entries

    def extract_entry(self, archive_path: Path, entry_name: str) -> bytes:
        with zipfile.ZipFile(archive_path, "r") as zf:
            return zf.read(entry_name)
