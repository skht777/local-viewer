"""アーカイブ読み取りの抽象インターフェースと ZIP/RAR/7z 実装.

- ArchiveReader ABC: list_entries, extract_entry, supports
- ZipArchiveReader: zipfile.ZipFile でダイレクト読み取り
- RarArchiveReader: rarfile.RarFile + unrar-free
- SevenZipArchiveReader: p7zip CLI (subprocess)
- エントリのフィルタ/ソート/セキュリティ検証を内包
"""

import os
import tempfile
import zipfile
from abc import ABC, abstractmethod
from dataclasses import dataclass
from pathlib import Path

from backend.services.archive_security import (
    ArchiveEntryValidator,
    ArchivePasswordError,
    ArchiveSecurityError,
)
from backend.services.natural_sort import natural_sort_key

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

    def extract_entry_to_file(
        self, archive_path: Path, entry_name: str, dest: Path
    ) -> None:
        """エントリをファイルに直接展開する (デフォルトは bytes 経由フォールバック)."""
        data = self.extract_entry(archive_path, entry_name)
        dest.write_bytes(data)

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
        entries.sort(key=lambda e: natural_sort_key(e.name))
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

    def extract_entry_to_file(
        self, archive_path: Path, entry_name: str, dest: Path
    ) -> None:
        """ZIP エントリをチャンク読みでファイルに書き出す (メモリ節約)."""
        max_size = self._validator.max_entry_size_for(entry_name)
        with zipfile.ZipFile(archive_path, "r") as zf:
            with zf.open(entry_name) as src, open(dest, "wb") as dst:
                total = 0
                while True:
                    chunk = src.read(_EXTRACT_CHUNK_SIZE)
                    if not chunk:
                        break
                    total += len(chunk)
                    if total > max_size:
                        msg = f"抽出時にサイズ上限を超えました: {entry_name}"
                        raise ArchiveSecurityError(msg)
                    dst.write(chunk)


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

        entries.sort(key=lambda e: natural_sort_key(e.name))
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

    def extract_entry_to_file(
        self, archive_path: Path, entry_name: str, dest: Path
    ) -> None:
        """RAR エントリをチャンク読みでファイルに書き出す (メモリ節約)."""
        import rarfile

        max_size = self._validator.max_entry_size_for(entry_name)
        with rarfile.RarFile(archive_path, "r") as rf:
            with rf.open(entry_name) as src, open(dest, "wb") as dst:
                total = 0
                while True:
                    chunk = src.read(_EXTRACT_CHUNK_SIZE)
                    if not chunk:
                        break
                    total += len(chunk)
                    if total > max_size:
                        msg = f"抽出時にサイズ上限を超えました: {entry_name}"
                        raise ArchiveSecurityError(msg)
                    dst.write(chunk)


class SevenZipArchiveReader(ArchiveReader):
    """7z アーカイブリーダー.

    p7zip CLI (7z コマンド) を使用。py7zr (Pure Python) より 5-20x 高速。
    """

    _EXTENSIONS = frozenset({".7z"})

    def __init__(self, validator: ArchiveEntryValidator) -> None:
        self._validator = validator

    @property
    def is_available(self) -> bool:
        """7z コマンドが利用可能かを返す."""
        import shutil

        return shutil.which("7z") is not None

    def supports(self, path: Path) -> bool:
        return self.is_available and path.suffix.lower() in self._EXTENSIONS

    def list_entries(self, archive_path: Path) -> list[ArchiveEntry]:
        """7z l -slt で Key=Value 形式のエントリ情報を取得する."""
        import subprocess

        # パスワード検出: 7z t で事前チェック
        self._check_password(archive_path)

        # archive_path は PathSecurity 検証済み
        result = subprocess.run(  # noqa: S603
            ["7z", "l", "-slt", str(archive_path)],  # noqa: S607
            capture_output=True,
            text=True,
            timeout=30,
        )
        if result.returncode != 0:
            msg = f"7z l failed: {result.stderr.strip()}"
            raise OSError(msg)

        entries: list[ArchiveEntry] = []
        total_uncompressed = 0

        # -slt 出力をパース: 空行でブロック区切り、各行が Key = Value
        for block in self._parse_slt_blocks(result.stdout):
            path = block.get("Path", "")
            if not path:
                continue
            # ディレクトリをスキップ
            if block.get("Folder", "") == "+":
                continue

            name = path.replace("\\", "/")
            try:
                self._validator.validate_entry_name(name)
            except ArchiveSecurityError:
                continue

            if not self._validator.is_allowed_extension(name):
                continue

            # ソリッドアーカイブでは Packed Size が空文字列
            compressed = int(block.get("Packed Size") or "0")
            uncompressed = int(block.get("Size") or "0")

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
        entries.sort(key=lambda e: natural_sort_key(e.name))
        return entries

    def extract_entry(self, archive_path: Path, entry_name: str) -> bytes:
        """stdout ストリーミングで単一エントリを展開する.

        チャンク読みでサイズ上限を強制し、超過時は即座に kill する。
        """
        import subprocess

        max_size = self._validator.max_entry_size_for(entry_name)

        # archive_path: PathSecurity 検証済み, entry_name: Validator 検証済み
        proc = subprocess.Popen(  # noqa: S603
            ["7z", "x", "-so", str(archive_path), entry_name],  # noqa: S607
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        try:
            chunks: list[bytes] = []
            total = 0
            while True:
                chunk = proc.stdout.read(_EXTRACT_CHUNK_SIZE)  # type: ignore[union-attr]
                if not chunk:
                    break
                total += len(chunk)
                if total > max_size:
                    proc.kill()
                    msg = f"抽出時にサイズ上限を超えました: {entry_name}"
                    raise ArchiveSecurityError(msg)
                chunks.append(chunk)

            proc.wait(timeout=60)
            if proc.returncode != 0:
                stderr = proc.stderr.read().decode(errors="replace")  # type: ignore[union-attr]
                # エントリが見つからない場合
                msg = entry_name
                raise (
                    KeyError(msg)
                    if "cannot find" in stderr.lower()
                    else OSError(f"7z x failed: {stderr.strip()}")
                )
        finally:
            proc.stdout.close()  # type: ignore[union-attr]
            proc.stderr.close()  # type: ignore[union-attr]

        data = b"".join(chunks)
        if not data:
            msg = entry_name
            raise KeyError(msg)
        return data

    def extract_entry_to_file(
        self, archive_path: Path, entry_name: str, dest: Path
    ) -> None:
        """7z エントリをディレクトリに展開し dest に移動する.

        - dest.parent 配下に一時ディレクトリを作成し同一 filesystem を保証
        - os.replace() でアトミックに配置
        """
        import shutil
        import subprocess

        max_size = self._validator.max_entry_size_for(entry_name)

        # dest.parent 配下に一時ディレクトリを作成
        tmp_dir = Path(tempfile.mkdtemp(dir=dest.parent, prefix=".tmp_7z_"))
        try:
            # -o と tmp_dir の間にスペースなし (7z の仕様)
            # archive_path: PathSecurity 検証済み, entry_name: Validator 検証済み
            subprocess.run(  # noqa: S603
                ["7z", "x", "-o" + str(tmp_dir), str(archive_path), entry_name],  # noqa: S607
                capture_output=True,
                check=True,
                timeout=60,
            )

            extracted = tmp_dir / entry_name
            if not extracted.exists():
                msg = entry_name
                raise KeyError(msg)

            # サイズ検証
            actual_size = extracted.stat().st_size
            if actual_size > max_size:
                msg = f"抽出時にサイズ上限を超えました: {entry_name}"
                raise ArchiveSecurityError(msg)

            os.replace(str(extracted), str(dest))
            dest.chmod(0o644)
        finally:
            shutil.rmtree(tmp_dir, ignore_errors=True)

    @staticmethod
    def _check_password(archive_path: Path) -> None:
        """7z t でパスワード保護を検出する."""
        import subprocess

        # archive_path は PathSecurity 検証済み
        result = subprocess.run(  # noqa: S603
            ["7z", "t", str(archive_path)],  # noqa: S607
            capture_output=True,
            text=True,
            timeout=30,
        )
        if result.returncode != 0:
            stderr = result.stderr.lower()
            if "wrong password" in stderr or "cannot open encrypted" in stderr:
                raise ArchivePasswordError()

    @staticmethod
    def _parse_slt_blocks(output: str) -> list[dict[str, str]]:
        """7z l -slt の出力を Key=Value ブロックのリストにパースする."""
        blocks: list[dict[str, str]] = []
        current: dict[str, str] = {}

        for line in output.splitlines():
            line = line.strip()
            if not line:
                if current:
                    blocks.append(current)
                    current = {}
                continue
            if " = " in line:
                key, _, value = line.partition(" = ")
                current[key] = value

        if current:
            blocks.append(current)
        return blocks
