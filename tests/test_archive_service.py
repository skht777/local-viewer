"""ArchiveService のテスト."""

import zipfile
from pathlib import Path

import pytest

from backend.services.archive_security import ArchiveEntryValidator
from backend.services.archive_service import ArchiveService, ByteLRUCache

# 最小 JPEG
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


@pytest.fixture
def archive_service(test_settings) -> ArchiveService:
    validator = ArchiveEntryValidator(test_settings)
    return ArchiveService(validator=validator, cache_max_bytes=1024)


@pytest.fixture
def zip_archive(tmp_path: Path) -> Path:
    archive = tmp_path / "test.zip"
    with zipfile.ZipFile(archive, "w") as zf:
        zf.writestr("img01.jpg", MINIMAL_JPEG)
        zf.writestr("img02.jpg", MINIMAL_JPEG)
    return archive


def test_ZIPファイルに対応するリーダーが選択される(
    archive_service: ArchiveService, zip_archive: Path
) -> None:
    reader = archive_service.get_reader(zip_archive)
    assert reader is not None


def test_サポート外拡張子でNoneを返す(
    archive_service: ArchiveService, tmp_path: Path
) -> None:
    reader = archive_service.get_reader(tmp_path / "file.txt")
    assert reader is None


def test_エントリ抽出結果がキャッシュされる(
    archive_service: ArchiveService, zip_archive: Path
) -> None:
    # 1回目: キャッシュミス
    data1 = archive_service.extract_entry(zip_archive, "img01.jpg")
    assert data1 == MINIMAL_JPEG

    # 2回目: キャッシュヒット (同一データ)
    data2 = archive_service.extract_entry(zip_archive, "img01.jpg")
    assert data2 == MINIMAL_JPEG


def test_キャッシュがバイト上限で古いエントリを追い出す() -> None:
    cache = ByteLRUCache(max_bytes=50)
    cache.put("a", b"x" * 30)
    cache.put("b", b"y" * 30)  # 合計 60 > 50 なので "a" が追い出される
    assert cache.get("a") is None
    assert cache.get("b") == b"y" * 30


def test_diagnosticsで各形式の利用可否を返す(
    archive_service: ArchiveService,
) -> None:
    diag = archive_service.get_diagnostics()
    assert "zip" in diag
    assert diag["zip"] is True
    assert "rar" in diag
    assert "7z" in diag
