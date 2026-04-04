"""サムネイルプリウォームのテスト."""

import io
from pathlib import Path

import pytest
from PIL import Image

from backend.config import Settings
from backend.services.archive_security import ArchiveEntryValidator
from backend.services.archive_service import ArchiveService
from backend.services.extensions import EntryKind
from backend.services.node_registry import EntryMeta, NodeRegistry
from backend.services.temp_file_cache import TempFileCache
from backend.services.thumbnail_service import ThumbnailService
from backend.services.thumbnail_warmer import ThumbnailWarmer


@pytest.fixture
def warmer_setup(
    test_node_registry: NodeRegistry,
    test_archive_service: ArchiveService,
    test_root: Path,
) -> tuple[ThumbnailWarmer, ThumbnailService, NodeRegistry, Path]:
    """プリウォームテスト用のセットアップ."""
    temp_cache = TempFileCache(
        cache_dir=test_root / ".thumb-cache",
        max_size_bytes=100 * 1024 * 1024,
    )
    thumb_service = ThumbnailService(temp_cache=temp_cache)
    warmer = ThumbnailWarmer(
        thumb_service=thumb_service,
        archive_service=test_archive_service,
        registry=test_node_registry,
    )
    return warmer, thumb_service, test_node_registry, test_root


async def test_プリウォームがキャッシュミスのサムネイルを生成する(
    warmer_setup: tuple[ThumbnailWarmer, ThumbnailService, NodeRegistry, Path],
) -> None:
    warmer, thumb_service, registry, root = warmer_setup

    entries = registry.list_directory(root / "dir_a")
    image_entry = next(e for e in entries if e.name == "image.jpg")

    # キャッシュは空
    cache_key = ThumbnailService.make_cache_key(
        image_entry.node_id, (root / "dir_a" / "image.jpg").stat().st_mtime_ns
    )
    assert not thumb_service.is_cached(cache_key)

    # プリウォーム実行
    await warmer.warm([image_entry])

    # キャッシュにサムネイルが生成されている
    assert thumb_service.is_cached(cache_key)


async def test_プリウォームがキャッシュ済みエントリをスキップする(
    warmer_setup: tuple[ThumbnailWarmer, ThumbnailService, NodeRegistry, Path],
) -> None:
    warmer, thumb_service, registry, root = warmer_setup

    entries = registry.list_directory(root / "dir_a")
    image_entry = next(e for e in entries if e.name == "image.jpg")

    # 先にサムネイルを生成してキャッシュに入れる
    path = root / "dir_a" / "image.jpg"
    cache_key = ThumbnailService.make_cache_key(
        image_entry.node_id, path.stat().st_mtime_ns
    )
    thumb_service.get_or_generate_from_path(path, cache_key)
    assert thumb_service.is_cached(cache_key)

    # プリウォーム実行 (エラーなく完了すること)
    await warmer.warm([image_entry])

    # キャッシュは引き続き存在
    assert thumb_service.is_cached(cache_key)


async def test_プリウォームが画像以外のエントリをスキップする(
    warmer_setup: tuple[ThumbnailWarmer, ThumbnailService, NodeRegistry, Path],
) -> None:
    warmer, _, _, _ = warmer_setup

    # kind=directory のエントリはスキップされる
    dir_entry = EntryMeta(
        node_id="fake_dir",
        name="some_dir",
        kind=EntryKind.DIRECTORY,
        child_count=5,
    )

    # エラーなく完了すること (directory はスキップ)
    await warmer.warm([dir_entry])
