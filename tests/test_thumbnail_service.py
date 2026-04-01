"""ThumbnailService のテスト."""

import io
from pathlib import Path

import pytest
from PIL import Image

from backend.services.temp_file_cache import TempFileCache
from backend.services.thumbnail_service import ThumbnailService


@pytest.fixture
def temp_cache(tmp_path: Path) -> TempFileCache:
    """テスト用 TempFileCache."""
    return TempFileCache(cache_dir=tmp_path / "cache", max_size_bytes=50 * 1024 * 1024)


@pytest.fixture
def service(temp_cache: TempFileCache) -> ThumbnailService:
    """テスト用 ThumbnailService."""
    return ThumbnailService(temp_cache)


def _make_jpeg(width: int = 800, height: int = 600) -> bytes:
    """テスト用 JPEG 画像を生成する."""
    img = Image.new("RGB", (width, height), color="red")
    buf = io.BytesIO()
    img.save(buf, format="JPEG")
    return buf.getvalue()


def _make_png_rgba(width: int = 400, height: int = 300) -> bytes:
    """テスト用 RGBA PNG 画像を生成する."""
    img = Image.new("RGBA", (width, height), color=(255, 0, 0, 128))
    buf = io.BytesIO()
    img.save(buf, format="PNG")
    return buf.getvalue()


def test_JPEG画像からサムネイルが生成される(service: ThumbnailService) -> None:
    source = _make_jpeg(800, 600)
    result = service.generate_thumbnail(source)
    # 結果が JPEG バイト列であること
    assert result[:2] == b"\xff\xd8"
    # リサイズされていること (300px 以内)
    img = Image.open(io.BytesIO(result))
    assert max(img.size) <= 300


def test_PNG透過画像がJPEGに変換される(service: ThumbnailService) -> None:
    source = _make_png_rgba()
    result = service.generate_thumbnail(source)
    # JPEG として出力されること
    assert result[:2] == b"\xff\xd8"
    img = Image.open(io.BytesIO(result))
    assert img.mode == "RGB"


def test_キャッシュヒット時に再生成されない(
    service: ThumbnailService, temp_cache: TempFileCache
) -> None:
    source = _make_jpeg()
    cache_key = service.make_cache_key("test_node_id", 1234567890)

    # 初回: キャッシュミス → 生成
    result1 = service.get_or_generate(source, cache_key)
    assert result1.exists()

    # 2回目: キャッシュヒット
    result2 = service.get_or_generate(source, cache_key)
    assert result2 == result1


def test_指定幅でリサイズされる(service: ThumbnailService) -> None:
    source = _make_jpeg(1000, 500)
    result = service.generate_thumbnail(source, width=150)
    img = Image.open(io.BytesIO(result))
    assert img.width <= 150
    assert img.height <= 150


def test_キャッシュキーにmtimeが含まれる(service: ThumbnailService) -> None:
    key1 = service.make_cache_key("node_abc", 1000)
    key2 = service.make_cache_key("node_abc", 2000)
    assert key1 != key2


def test_破損画像でエラーが発生する(service: ThumbnailService) -> None:
    with pytest.raises(Exception):
        service.generate_thumbnail(b"not an image")
