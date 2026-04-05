"""アーカイブ一括展開のテスト."""

import io
import os
import zipfile
from pathlib import Path

import pytest
from PIL import Image

from py_backend.config import Settings
from py_backend.services.archive_security import ArchiveEntryValidator
from py_backend.services.archive_service import ArchiveService


def _make_test_zip(path: Path) -> None:
    """テスト用 ZIP を作成する (3 画像エントリ)."""
    buf = io.BytesIO()
    Image.new("RGB", (1, 1), color="red").save(buf, format="JPEG")
    jpeg = buf.getvalue()

    with zipfile.ZipFile(path, "w") as zf:
        zf.writestr("img01.jpg", jpeg)
        zf.writestr("img02.jpg", jpeg)
        zf.writestr("img03.jpg", jpeg)


@pytest.fixture(autouse=True)
def _set_mount_base_dir(tmp_path: Path) -> None:
    """テスト用に MOUNT_BASE_DIR を設定."""
    os.environ["MOUNT_BASE_DIR"] = str(tmp_path)
    yield  # type: ignore[misc]
    os.environ.pop("MOUNT_BASE_DIR", None)


def test_ZIPの複数エントリを1回のオープンで一括抽出(tmp_path: Path) -> None:
    zip_path = tmp_path / "test.zip"
    _make_test_zip(zip_path)

    settings = Settings()
    validator = ArchiveEntryValidator(settings)
    service = ArchiveService(validator=validator)

    result = service.extract_entries_batch(
        zip_path, ["img01.jpg", "img02.jpg", "img03.jpg"]
    )

    assert len(result) == 3
    assert set(result.keys()) == {"img01.jpg", "img02.jpg", "img03.jpg"}
    # 各エントリが JPEG データであること
    for data in result.values():
        assert data[:2] == b"\xff\xd8"


def test_一括抽出でキャッシュ済みエントリをスキップし未キャッシュのみ展開(
    tmp_path: Path,
) -> None:
    zip_path = tmp_path / "test.zip"
    _make_test_zip(zip_path)

    settings = Settings()
    validator = ArchiveEntryValidator(settings)
    service = ArchiveService(validator=validator)

    # 先に img01 だけ個別抽出してキャッシュに入れる
    data1 = service.extract_entry(zip_path, "img01.jpg")
    assert data1[:2] == b"\xff\xd8"

    # バッチ抽出: img01 はキャッシュから、img02/03 は新規展開
    result = service.extract_entries_batch(
        zip_path, ["img01.jpg", "img02.jpg", "img03.jpg"]
    )

    assert len(result) == 3
    # キャッシュ済みの img01 と同じデータが返ること
    assert result["img01.jpg"] == data1
