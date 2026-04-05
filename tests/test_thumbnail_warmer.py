"""サムネイルプリウォームのテスト."""

import asyncio
import io
from pathlib import Path

import pytest
from PIL import Image

from py_backend.config import Settings
from py_backend.services.archive_security import ArchiveEntryValidator
from py_backend.services.archive_service import ArchiveService
from py_backend.services.extensions import EntryKind
from py_backend.services.node_registry import EntryMeta, NodeRegistry
from py_backend.services.temp_file_cache import TempFileCache
from py_backend.services.thumbnail_service import ThumbnailService
from py_backend.services.thumbnail_warmer import ThumbnailWarmer
from py_backend.services.video_converter import VideoConverter


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
    video_converter = VideoConverter(temp_cache=temp_cache, timeout=30)
    warmer = ThumbnailWarmer(
        thumb_service=thumb_service,
        archive_service=test_archive_service,
        registry=test_node_registry,
        video_converter=video_converter,
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


async def test_プリウォームがPDFエントリを処理する(
    warmer_setup: tuple[ThumbnailWarmer, ThumbnailService, NodeRegistry, Path],
) -> None:
    warmer, thumb_service, registry, root = warmer_setup

    # 有効な PDF を作成
    _img = Image.new("RGB", (10, 10), color="green")
    _buf = io.BytesIO()
    _img.save(_buf, format="PDF")
    pdf_path = root / "warmable.pdf"
    pdf_path.write_bytes(_buf.getvalue())
    node_id = registry.register(pdf_path)

    pdf_entry = EntryMeta(
        node_id=node_id,
        name="warmable.pdf",
        kind=EntryKind.PDF,
        size_bytes=len(_buf.getvalue()),
        modified_at=pdf_path.stat().st_mtime,
    )

    # プリウォーム実行
    await warmer.warm([pdf_entry])

    # キャッシュにサムネイルが生成されている
    cache_key = ThumbnailService.make_cache_key(node_id, pdf_path.stat().st_mtime_ns)
    assert thumb_service.is_cached(cache_key)


async def test_プリウォームが動画エントリを処理する(
    warmer_setup: tuple[ThumbnailWarmer, ThumbnailService, NodeRegistry, Path],
) -> None:
    warmer, thumb_service, registry, root = warmer_setup

    # 動画ファイルを作成
    video_path = root / "test_warm.mp4"
    video_path.write_bytes(b"\x00" * 1024)
    node_id = registry.register(video_path)

    video_entry = EntryMeta(
        node_id=node_id,
        name="test_warm.mp4",
        kind=EntryKind.VIDEO,
        size_bytes=1024,
        modified_at=video_path.stat().st_mtime,
    )

    # extract_frame をモックして有効な JPEG を返す
    _img = Image.new("RGB", (10, 10), color="blue")
    _buf = io.BytesIO()
    _img.save(_buf, format="JPEG")
    fake_jpeg = _buf.getvalue()

    from unittest.mock import patch

    with patch(
        "py_backend.services.video_converter.VideoConverter.extract_frame",
        return_value=fake_jpeg,
    ):
        await warmer.warm([video_entry])

    # キャッシュにサムネイルが生成されている
    cache_key = ThumbnailService.make_cache_key(
        node_id, video_path.stat().st_mtime_ns
    )
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


async def test_プリウォームの重複排除が動作する(
    warmer_setup: tuple[ThumbnailWarmer, ThumbnailService, NodeRegistry, Path],
) -> None:
    """同じ node_id の warm が同時に呼ばれても 1 回しか処理されない."""
    from unittest.mock import patch

    warmer, thumb_service, registry, root = warmer_setup

    entries = registry.list_directory(root / "dir_a")
    image_entry = next(e for e in entries if e.name == "image.jpg")

    call_count = 0

    def counting_generate(*args: object, **kwargs: object) -> bytes:
        nonlocal call_count
        call_count += 1
        return b""  # ダミー

    with patch(
        "py_backend.routers.thumbnail._generate_thumbnail_bytes",
        side_effect=counting_generate,
    ):
        # 同じエントリを 2 回並行で warm
        await asyncio.gather(
            warmer.warm([image_entry]),
            warmer.warm([image_entry]),
        )

    # _pending による重複排除で生成は 1 回のみ
    assert call_count <= 1


async def test_Semaphoreの並行度上限が守られる(
    warmer_setup: tuple[ThumbnailWarmer, ThumbnailService, NodeRegistry, Path],
) -> None:
    """Semaphore(4) により同時実行数が 4 以下であることを検証."""
    import threading
    from unittest.mock import patch

    warmer, _, registry, root = warmer_setup

    max_concurrent = 0
    current_concurrent = 0
    lock = threading.Lock()

    def tracking_generate(*args: object, **kwargs: object) -> bytes:
        nonlocal max_concurrent, current_concurrent
        import time

        with lock:
            current_concurrent += 1
            if current_concurrent > max_concurrent:
                max_concurrent = current_concurrent
        try:
            time.sleep(0.01)  # run_in_threadpool で並行度を観測しやすくする
            return b""  # ダミー
        finally:
            with lock:
                current_concurrent -= 1

    # 6 件のエントリを作成 (Semaphore(4) より多い)
    entries = []
    for i in range(6):
        p = root / f"parallel_{i}.jpg"
        _img = Image.new("RGB", (1, 1), color="red")
        _buf = io.BytesIO()
        _img.save(_buf, format="JPEG")
        p.write_bytes(_buf.getvalue())
        node_id = registry.register(p)
        entries.append(
            EntryMeta(
                node_id=node_id,
                name=f"parallel_{i}.jpg",
                kind=EntryKind.IMAGE,
                size_bytes=len(_buf.getvalue()),
                modified_at=p.stat().st_mtime,
            )
        )

    with patch(
        "py_backend.routers.thumbnail._generate_thumbnail_bytes",
        side_effect=tracking_generate,
    ):
        await warmer.warm(entries)

    # Semaphore(4) により最大同時実行数は 4 以下
    assert max_concurrent <= 4
