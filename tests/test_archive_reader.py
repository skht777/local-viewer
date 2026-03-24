"""ArchiveReader (ZIP/7z) のテスト."""

import zipfile
from pathlib import Path

import pytest

from backend.services.archive_reader import (
    RarArchiveReader,
    SevenZipArchiveReader,
    ZipArchiveReader,
)
from backend.services.archive_security import (
    ArchiveEntryValidator,
    ArchivePasswordError,
)

# py7zr は Python 3.14 + _zstd 未ビルド環境で import 失敗する場合がある
try:
    import py7zr

    HAS_PY7ZR = True
except ImportError:
    HAS_PY7ZR = False

# 最小 JPEG (テスト用)
MINIMAL_JPEG = bytes(
    [
        0xFF,
        0xD8,
        0xFF,
        0xE0,
        0x00,
        0x10,
        0x4A,
        0x46,
        0x49,
        0x46,
        0x00,
        0x01,
        0x01,
        0x00,
        0x00,
        0x01,
        0x00,
        0x01,
        0x00,
        0x00,
        0xFF,
        0xD9,
    ]
)

MINIMAL_PNG = b"\x89PNG\r\n\x1a\n" + b"\x00" * 50


@pytest.fixture
def zip_archive(tmp_path: Path) -> Path:
    """テスト用 ZIP を動的生成する."""
    archive = tmp_path / "test.zip"
    with zipfile.ZipFile(archive, "w") as zf:
        zf.writestr("image01.jpg", MINIMAL_JPEG)
        zf.writestr("image02.png", MINIMAL_PNG)
        zf.writestr("subdir/image03.jpg", MINIMAL_JPEG)
        zf.writestr("readme.txt", "hello world")
    return archive


@pytest.fixture
def zip_reader(test_settings) -> ZipArchiveReader:
    validator = ArchiveEntryValidator(test_settings)
    return ZipArchiveReader(validator)


# --- list_entries ---


def test_ZIPのエントリ一覧を返す(
    zip_reader: ZipArchiveReader, zip_archive: Path
) -> None:
    entries = zip_reader.list_entries(zip_archive)
    # readme.txt は画像ではないので除外される
    assert len(entries) == 3
    names = [e.name for e in entries]
    assert "image01.jpg" in names
    assert "image02.png" in names
    assert "subdir/image03.jpg" in names


def test_ZIPエントリがフルパスでソートされる(
    zip_reader: ZipArchiveReader, zip_archive: Path
) -> None:
    entries = zip_reader.list_entries(zip_archive)
    names = [e.name for e in entries]
    assert names == sorted(names, key=str.lower)


def test_ZIPのディレクトリエントリが除外される(
    zip_reader: ZipArchiveReader, tmp_path: Path
) -> None:
    archive = tmp_path / "with_dirs.zip"
    with zipfile.ZipFile(archive, "w") as zf:
        zf.writestr("dir/", "")  # ディレクトリエントリ
        zf.writestr("dir/image.jpg", MINIMAL_JPEG)
    entries = zip_reader.list_entries(archive)
    assert len(entries) == 1
    assert entries[0].name == "dir/image.jpg"


def test_許可外拡張子のエントリが除外される(
    zip_reader: ZipArchiveReader, zip_archive: Path
) -> None:
    entries = zip_reader.list_entries(zip_archive)
    names = [e.name for e in entries]
    assert "readme.txt" not in names


# --- extract_entry ---


def test_ZIPエントリのバイナリデータを取得する(
    zip_reader: ZipArchiveReader, zip_archive: Path
) -> None:
    data = zip_reader.extract_entry(zip_archive, "image01.jpg")
    assert data == MINIMAL_JPEG


def test_存在しないエントリ名でKeyError(
    zip_reader: ZipArchiveReader, zip_archive: Path
) -> None:
    with pytest.raises(KeyError):
        zip_reader.extract_entry(zip_archive, "nonexistent.jpg")


# --- エラー系 ---


def test_壊れたZIPでエラーを返す(zip_reader: ZipArchiveReader, tmp_path: Path) -> None:
    broken = tmp_path / "broken.zip"
    broken.write_bytes(b"not a zip file")
    with pytest.raises(zipfile.BadZipFile):
        zip_reader.list_entries(broken)


def test_パスワード付きZIPでArchivePasswordError(
    zip_reader: ZipArchiveReader, tmp_path: Path
) -> None:
    # 通常の ZIP を作成し、バイナリレベルで暗号化フラグを立てる
    archive = tmp_path / "encrypted.zip"
    with zipfile.ZipFile(archive, "w") as zf:
        zf.writestr("secret.jpg", MINIMAL_JPEG)

    # ZIP ローカルファイルヘッダの general purpose bit flag (offset 6) にビット0を設定
    data = bytearray(archive.read_bytes())
    # ローカルファイルヘッダは先頭 (PK\x03\x04)
    assert data[0:4] == b"PK\x03\x04"
    data[6] |= 0x01  # 暗号化フラグ
    # セントラルディレクトリヘッダにも同じフラグを設定
    cd_offset = data.index(b"PK\x01\x02")
    data[cd_offset + 8] |= 0x01
    archive.write_bytes(bytes(data))

    with pytest.raises(ArchivePasswordError):
        zip_reader.list_entries(archive)


# --- supports ---


def test_ZIP_supportsが正しい拡張子で真を返す(
    zip_reader: ZipArchiveReader, tmp_path: Path
) -> None:
    assert zip_reader.supports(tmp_path / "test.zip") is True
    assert zip_reader.supports(tmp_path / "test.cbz") is True
    assert zip_reader.supports(tmp_path / "test.ZIP") is True
    assert zip_reader.supports(tmp_path / "test.rar") is False
    assert zip_reader.supports(tmp_path / "test.7z") is False
    assert zip_reader.supports(tmp_path / "test.txt") is False


# ===== 7z テスト =====

skip_no_py7zr = pytest.mark.skipif(
    not HAS_PY7ZR, reason="py7zr unavailable (_zstd not built)"
)


@pytest.fixture
def sevenz_archive(tmp_path: Path) -> Path:
    """テスト用 7z を動的生成する."""
    if not HAS_PY7ZR:
        pytest.skip("py7zr unavailable")
    archive = tmp_path / "test.7z"
    with py7zr.SevenZipFile(archive, "w") as sz:
        sz.writestr(MINIMAL_JPEG, "image01.jpg")
        sz.writestr(MINIMAL_PNG, "image02.png")
        sz.writestr(b"text content", "readme.txt")
    return archive


@pytest.fixture
def sevenz_reader(test_settings) -> SevenZipArchiveReader:
    validator = ArchiveEntryValidator(test_settings)
    return SevenZipArchiveReader(validator)


@skip_no_py7zr
def test_7zのエントリ一覧を返す(
    sevenz_reader: SevenZipArchiveReader,
    sevenz_archive: Path,
) -> None:
    entries = sevenz_reader.list_entries(sevenz_archive)
    # readme.txt は画像ではないので除外
    assert len(entries) == 2
    names = [e.name for e in entries]
    assert "image01.jpg" in names
    assert "image02.png" in names


@skip_no_py7zr
def test_7zエントリのバイナリデータを取得する(
    sevenz_reader: SevenZipArchiveReader,
    sevenz_archive: Path,
) -> None:
    data = sevenz_reader.extract_entry(sevenz_archive, "image01.jpg")
    assert data == MINIMAL_JPEG


@skip_no_py7zr
def test_7z_supportsが正しい拡張子で真を返す(
    sevenz_reader: SevenZipArchiveReader,
    tmp_path: Path,
) -> None:
    assert sevenz_reader.supports(tmp_path / "test.7z") is True
    assert sevenz_reader.supports(tmp_path / "test.zip") is False
    assert sevenz_reader.supports(tmp_path / "test.rar") is False


# ===== RAR テスト =====


def test_RAR_unrar未インストール時の動作(test_settings) -> None:
    """RarArchiveReader のロジックテスト (unrar 有無に関わらず実行可能)."""
    validator = ArchiveEntryValidator(test_settings)
    reader = RarArchiveReader(validator)
    # is_available はシステム依存
    # supports は is_available が False なら常に False
    if not reader.is_available:
        assert reader.supports(Path("test.rar")) is False
