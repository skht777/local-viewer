"""アーカイブ読み取りの抽象インターフェースと ZIP/RAR/7z 実装.

- ArchiveReader ABC: list_entries, extract_entry, supports
- ZipArchiveReader: zipfile.ZipFile でダイレクト読み取り
- RarArchiveReader: rarfile.RarFile + unrar-free
- SevenZipArchiveReader: py7zr.SevenZipFile (Pure Python)
- エントリのフィルタ/ソート/セキュリティ検証を内包
"""

import zipfile
from abc import ABC, abstractmethod
from dataclasses import dataclass
from pathlib import Path

from backend.services.archive_security import (
    ArchiveEntryValidator,
    ArchivePasswordError,
    ArchiveSecurityError,
)

# extract_entry のチャンク読みサイズ (64KiB)
_EXTRACT_CHUNK_SIZE = 64 * 1024


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

                # エントリ名セキュリティ検証 (不正エントリは個別スキップ)
                try:
                    self._validator.validate_entry_name(name)
                except ArchiveSecurityError:
                    continue

                # 許可拡張子チェック
                if not self._validator.is_allowed_extension(name):
                    continue

                # サイズ検証 (超過エントリは個別スキップ)
                try:
                    self._validator.validate_entry_size(
                        compressed=info.compress_size,
                        uncompressed=info.file_size,
                        name=name,
                    )
                except ArchiveSecurityError:
                    continue
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
        """エントリをチャンク読みで抽出する (サイズ上限付き)."""
        max_size = self._validator.max_entry_size_for(entry_name)
        with zipfile.ZipFile(archive_path, "r") as zf:
            with zf.open(entry_name) as f:
                chunks: list[bytes] = []
                total = 0
                while True:
                    chunk = f.read(_EXTRACT_CHUNK_SIZE)
                    if not chunk:
                        break
                    total += len(chunk)
                    if total > max_size:
                        msg = f"抽出時にサイズ上限を超えました: {entry_name}"
                        raise ArchiveSecurityError(msg)
                    chunks.append(chunk)
                return b"".join(chunks)


class RarArchiveReader(ArchiveReader):
    """RAR/CBR アーカイブリーダー.

    rarfile + unrar-free が必要。未インストール時は supports() が False を返す。
    """

    _EXTENSIONS = frozenset({".rar", ".cbr"})

    def __init__(self, validator: ArchiveEntryValidator) -> None:
        self._validator = validator
        self._is_available = self._check_availability()

    @staticmethod
    def _check_availability() -> bool:
        try:
            import rarfile as _rf

            # unrar コマンドの存在確認
            _rf.UNRAR_TOOL  # noqa: B018
            return True
        except ImportError, Exception:
            return False

    @property
    def is_available(self) -> bool:
        return self._is_available

    def supports(self, path: Path) -> bool:
        return self._is_available and path.suffix.lower() in self._EXTENSIONS

    def list_entries(self, archive_path: Path) -> list[ArchiveEntry]:
        import rarfile

        with rarfile.RarFile(archive_path, "r") as rf:
            # パスワード付き検出
            if rf.needs_password():
                raise ArchivePasswordError()

            entries: list[ArchiveEntry] = []
            total_uncompressed = 0

            for info in rf.infolist():
                if info.is_dir():
                    continue

                name = info.filename.replace("\\", "/")
                try:
                    self._validator.validate_entry_name(name)
                except ArchiveSecurityError:
                    continue

                if not self._validator.is_allowed_extension(name):
                    continue

                try:
                    self._validator.validate_entry_size(
                        compressed=info.compress_size,
                        uncompressed=info.file_size,
                        name=name,
                    )
                except ArchiveSecurityError:
                    continue
                total_uncompressed += info.file_size

                entries.append(
                    ArchiveEntry(
                        name=name,
                        size_compressed=info.compress_size,
                        size_uncompressed=info.file_size,
                        is_dir=False,
                    )
                )

            self._validator.validate_total_size(total_uncompressed)

        entries.sort(key=lambda e: e.name.lower())
        return entries

    def extract_entry(self, archive_path: Path, entry_name: str) -> bytes:
        """エントリをチャンク読みで抽出する (サイズ上限付き)."""
        import rarfile

        max_size = self._validator.max_entry_size_for(entry_name)
        with rarfile.RarFile(archive_path, "r") as rf:
            with rf.open(entry_name) as f:
                chunks: list[bytes] = []
                total = 0
                while True:
                    chunk = f.read(_EXTRACT_CHUNK_SIZE)
                    if not chunk:
                        break
                    total += len(chunk)
                    if total > max_size:
                        msg = f"抽出時にサイズ上限を超えました: {entry_name}"
                        raise ArchiveSecurityError(msg)
                    chunks.append(chunk)
                return b"".join(chunks)


class SevenZipArchiveReader(ArchiveReader):
    """7z アーカイブリーダー.

    py7zr (Pure Python) を使用。システムパッケージ不要。
    """

    _EXTENSIONS = frozenset({".7z"})

    def __init__(self, validator: ArchiveEntryValidator) -> None:
        self._validator = validator

    def supports(self, path: Path) -> bool:
        return path.suffix.lower() in self._EXTENSIONS

    def list_entries(self, archive_path: Path) -> list[ArchiveEntry]:
        import py7zr

        try:
            with py7zr.SevenZipFile(archive_path, "r") as sz:
                # パスワード付き検出
                if sz.needs_password():
                    raise ArchivePasswordError()

                entries: list[ArchiveEntry] = []
                total_uncompressed = 0

                for entry in sz.list():
                    if entry.is_directory:
                        continue

                    name = entry.filename.replace("\\", "/")
                    try:
                        self._validator.validate_entry_name(name)
                    except ArchiveSecurityError:
                        continue

                    if not self._validator.is_allowed_extension(name):
                        continue

                    compressed = entry.compressed or 0
                    uncompressed = entry.uncompressed or 0

                    try:
                        self._validator.validate_entry_size(
                            compressed=compressed,
                            uncompressed=uncompressed,
                            name=name,
                        )
                    except ArchiveSecurityError:
                        continue
                    total_uncompressed += uncompressed

                    entries.append(
                        ArchiveEntry(
                            name=name,
                            size_compressed=compressed,
                            size_uncompressed=uncompressed,
                            is_dir=False,
                        )
                    )

                self._validator.validate_total_size(total_uncompressed)
        except py7zr.PasswordRequired:
            raise ArchivePasswordError() from None

        entries.sort(key=lambda e: e.name.lower())
        return entries

    def extract_entry(self, archive_path: Path, entry_name: str) -> bytes:
        """メモリ上に単一エントリを展開する.

        BytesIOFactory でディスク I/O を回避し、
        limit で max_entry_size を強制する。
        """
        import py7zr
        import py7zr.io

        factory = py7zr.io.BytesIOFactory(
            limit=self._validator.max_entry_size_for(entry_name),
        )
        with py7zr.SevenZipFile(archive_path, "r") as sz:
            sz.extract(targets=[entry_name], factory=factory)

        if entry_name not in factory.products:
            msg = entry_name
            raise KeyError(msg)

        bio = factory.products[entry_name]
        bio.seek(0)
        return bio.read()
