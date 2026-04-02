"""ArchiveReader (ZIP/7z) のテスト."""

import shutil
import subprocess
import zipfile
from pathlib import Path

import pytest

_has_7z = shutil.which("7z") is not None

from backend.services.archive_reader import (
    RarArchiveReader,
    SevenZipArchiveReader,
    ZipArchiveReader,
)
from backend.services.archive_security import (
    ArchiveEntryValidator,
    ArchivePasswordError,
    ArchiveSecurityError,
)

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


@pytest.fixture
def sevenz_archive(tmp_path: Path) -> Path:
    """テスト用 7z を p7zip CLI で動的生成する."""
    pytest.importorskip("subprocess")
    if not _has_7z:
        pytest.skip("p7zip (7z) が未インストール")

    # 一時ディレクトリにファイルを配置して 7z a で圧縮
    src_dir = tmp_path / "sevenz_src"
    src_dir.mkdir()
    (src_dir / "image01.jpg").write_bytes(MINIMAL_JPEG)
    (src_dir / "image02.png").write_bytes(MINIMAL_PNG)
    (src_dir / "readme.txt").write_text("text content")

    archive = tmp_path / "test.7z"
    subprocess.run(
        ["7z", "a", str(archive), str(src_dir / "*")],
        capture_output=True,
        check=True,
    )
    return archive


@pytest.fixture
def sevenz_reader(test_settings) -> SevenZipArchiveReader:
    validator = ArchiveEntryValidator(test_settings)
    return SevenZipArchiveReader(validator)


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


def test_7zエントリのバイナリデータを取得する(
    sevenz_reader: SevenZipArchiveReader,
    sevenz_archive: Path,
) -> None:
    data = sevenz_reader.extract_entry(sevenz_archive, "image01.jpg")
    assert data == MINIMAL_JPEG


def test_7z_supportsが正しい拡張子で真を返す(
    sevenz_reader: SevenZipArchiveReader,
    tmp_path: Path,
) -> None:
    assert sevenz_reader.supports(tmp_path / "test.7z") is True
    assert sevenz_reader.supports(tmp_path / "test.zip") is False
    assert sevenz_reader.supports(tmp_path / "test.rar") is False


# --- extract_entry サイズガード ---


def test_ZIP抽出時にサイズ上限を超えるとエラー(tmp_path: Path, test_settings) -> None:
    """抽出時のチャンク読みでサイズ上限を検出する."""
    # メタデータでは通過するが、実データが上限を超えるケースをシミュレート
    # → 小さい上限の validator で構築
    archive = tmp_path / "big_entry.zip"
    big_data = b"\x00" * 1024  # 1KB
    with zipfile.ZipFile(archive, "w") as zf:
        zf.writestr("large.jpg", big_data)

    # 上限を 512 バイトに設定
    test_settings.archive_max_entry_size = 512
    validator = ArchiveEntryValidator(test_settings)
    reader = ZipArchiveReader(validator)

    with pytest.raises(ArchiveSecurityError, match="抽出時にサイズ上限を超えました"):
        reader.extract_entry(archive, "large.jpg")


# ===== RAR テスト =====


# --- extract_entry_to_file ---


def test_ZIP_extract_entry_to_fileでファイルに書き出される(
    zip_reader: ZipArchiveReader, zip_archive: Path, tmp_path: Path
) -> None:
    dest = tmp_path / "output.jpg"
    zip_reader.extract_entry_to_file(zip_archive, "image01.jpg", dest)
    assert dest.exists()
    assert dest.read_bytes() == MINIMAL_JPEG


def test_ZIP_extract_entry_to_fileでサイズ上限超過時にエラー(
    tmp_path: Path, test_settings
) -> None:
    archive = tmp_path / "big_entry.zip"
    big_data = b"\x00" * 1024
    with zipfile.ZipFile(archive, "w") as zf:
        zf.writestr("large.jpg", big_data)

    test_settings.archive_max_entry_size = 512
    validator = ArchiveEntryValidator(test_settings)
    reader = ZipArchiveReader(validator)

    dest = tmp_path / "output.jpg"
    with pytest.raises(ArchiveSecurityError, match="抽出時にサイズ上限を超えました"):
        reader.extract_entry_to_file(archive, "large.jpg", dest)


def test_7z_extract_entry_to_fileでファイルに書き出される(
    sevenz_reader: SevenZipArchiveReader,
    sevenz_archive: Path,
    tmp_path: Path,
) -> None:
    dest = tmp_path / "output.jpg"
    sevenz_reader.extract_entry_to_file(sevenz_archive, "image01.jpg", dest)
    assert dest.exists()
    assert dest.read_bytes() == MINIMAL_JPEG


# ===== RAR テスト =====


def test_RAR_unrar未インストール時の動作(test_settings) -> None:
    """RarArchiveReader のロジックテスト (unrar 有無に関わらず実行可能)."""
    validator = ArchiveEntryValidator(test_settings)
    reader = RarArchiveReader(validator)
    # is_available はシステム依存
    # supports は is_available が False なら常に False
    if not reader.is_available:
        assert reader.supports(Path("test.rar")) is False
