"""テスト共通フィクスチャ."""

import os
import zipfile
from collections.abc import AsyncGenerator, Generator
from pathlib import Path

import pytest
from httpx import ASGITransport, AsyncClient

from backend.config import Settings
from backend.errors import (
    NodeNotFoundError,
    PathSecurityError,
    archive_password_error_handler,
    archive_security_error_handler,
    node_not_found_error_handler,
    path_security_error_handler,
)
from backend.main import app
from backend.routers import browse, file, mounts, search
from backend.services.archive_security import (
    ArchiveEntryValidator,
    ArchivePasswordError,
    ArchiveSecurityError,
)
from backend.services.archive_service import ArchiveService
from backend.services.indexer import Indexer
from backend.services.node_registry import NodeRegistry
from backend.services.path_security import PathSecurity


@pytest.fixture
def test_root(tmp_path: Path) -> Path:
    """テスト用ディレクトリ構造を作成する.

    tmp_path/
    ├── dir_a/
    │   ├── image.jpg (最小JPEG)
    │   └── nested/
    │       └── deep.txt
    ├── dir_b/
    │   └── video.mp4 (ダミー)
    ├── file.txt
    └── photo.png (ダミー)
    """
    (tmp_path / "dir_a").mkdir()
    (tmp_path / "dir_a" / "nested").mkdir()
    (tmp_path / "dir_b").mkdir()

    (tmp_path / "file.txt").write_text("hello")
    (tmp_path / "dir_a" / "nested" / "deep.txt").write_text("deep content")

    # 最小 JPEG
    minimal_jpeg = bytes(
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
    (tmp_path / "dir_a" / "image.jpg").write_bytes(minimal_jpeg)
    (tmp_path / "dir_b" / "video.mp4").write_bytes(b"\x00" * 1024)
    (tmp_path / "photo.png").write_bytes(b"\x89PNG\r\n\x1a\n" + b"\x00" * 100)

    # テスト用 ZIP アーカイブ (画像のみ)
    zip_path = tmp_path / "dir_a" / "archive.zip"
    with zipfile.ZipFile(zip_path, "w") as zf:
        zf.writestr("page01.jpg", minimal_jpeg)
        zf.writestr("page02.jpg", minimal_jpeg)
        zf.writestr("readme.txt", "not an image")

    # テスト用 ZIP アーカイブ (画像 + 動画)
    # 最小 MP4: ftyp ボックスのみ (ブラウザでは再生不可だがバイナリとして有効)
    minimal_mp4 = (
        b"\x00\x00\x00\x14"  # size=20
        b"ftypisom"  # type=ftyp, brand=isom
        b"\x00\x00\x00\x00"  # minor_version
        b"isom"  # compatible_brand
    )
    video_zip_path = tmp_path / "dir_a" / "mixed.zip"
    with zipfile.ZipFile(video_zip_path, "w") as zf:
        zf.writestr("clip.mp4", minimal_mp4 + b"\x00" * 100)
        zf.writestr("thumb.jpg", minimal_jpeg)
        zf.writestr("notes.txt", "not allowed")

    return tmp_path


@pytest.fixture
def test_settings(test_root: Path) -> Generator[Settings]:
    """テスト用 Settings."""
    os.environ["ROOT_DIR"] = str(test_root)
    os.environ.pop("ALLOW_SYMLINKS", None)
    s = Settings()
    yield s
    os.environ.pop("ROOT_DIR", None)


@pytest.fixture
def test_path_security(test_settings: Settings) -> PathSecurity:
    """テスト用 PathSecurity."""
    return PathSecurity(test_settings)


@pytest.fixture
def test_node_registry(test_path_security: PathSecurity) -> NodeRegistry:
    """テスト用 NodeRegistry."""
    return NodeRegistry(test_path_security)


@pytest.fixture
def test_archive_service(test_settings: Settings) -> ArchiveService:
    """テスト用 ArchiveService."""
    validator = ArchiveEntryValidator(test_settings)
    return ArchiveService(validator=validator)


@pytest.fixture
async def client(
    test_node_registry: NodeRegistry,
    test_archive_service: ArchiveService,
    test_root: Path,
) -> AsyncGenerator[AsyncClient]:
    """DI 差し替え済みの FastAPI TestClient.

    NodeRegistry と ArchiveService をテスト用インスタンスに差し替える。
    """
    app.dependency_overrides[browse.get_node_registry] = lambda: test_node_registry
    app.dependency_overrides[file.get_node_registry] = lambda: test_node_registry
    app.dependency_overrides[mounts.get_node_registry] = lambda: test_node_registry
    app.dependency_overrides[browse.get_archive_service] = lambda: test_archive_service
    app.dependency_overrides[file.get_archive_service] = lambda: test_archive_service

    # TempFileCache (テスト用)
    from backend.services.temp_file_cache import TempFileCache

    test_temp_cache = TempFileCache(
        cache_dir=test_root / ".disk-cache",
        max_size_bytes=100 * 1024 * 1024,
    )
    app.dependency_overrides[file.get_temp_file_cache] = lambda: test_temp_cache

    # VideoConverter (テスト用 — FFmpeg なし環境でも動作するよう無効化)
    from backend.services.video_converter import VideoConverter

    test_converter = VideoConverter(temp_cache=test_temp_cache, timeout=30)
    app.dependency_overrides[file.get_video_converter] = lambda: test_converter

    # 例外ハンドラ登録 (lifespan が動かないテスト用)
    app.add_exception_handler(
        PathSecurityError,
        path_security_error_handler,  # type: ignore[arg-type]
    )
    app.add_exception_handler(
        NodeNotFoundError,
        node_not_found_error_handler,  # type: ignore[arg-type]
    )
    app.add_exception_handler(
        ArchiveSecurityError,
        archive_security_error_handler,  # type: ignore[arg-type]
    )
    app.add_exception_handler(
        ArchivePasswordError,
        archive_password_error_handler,  # type: ignore[arg-type]
    )

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as ac:
        yield ac

    app.dependency_overrides.clear()


@pytest.fixture
def test_indexer(tmp_path: Path) -> Indexer:
    """テスト用 Indexer (初期化済み、is_ready=True)."""
    db_path = str(tmp_path / "test-index.db")
    idx = Indexer(db_path)
    idx.init_db()
    idx._is_ready = True
    return idx


@pytest.fixture
async def search_client(
    test_node_registry: NodeRegistry,
    test_archive_service: ArchiveService,
    test_indexer: Indexer,
    test_root: Path,
) -> AsyncGenerator[AsyncClient]:
    """検索 API 用の DI 差し替え済み TestClient.

    Indexer + NodeRegistry を差し替え。
    """
    app.dependency_overrides[browse.get_node_registry] = lambda: test_node_registry
    app.dependency_overrides[file.get_node_registry] = lambda: test_node_registry
    app.dependency_overrides[mounts.get_node_registry] = lambda: test_node_registry
    app.dependency_overrides[browse.get_archive_service] = lambda: test_archive_service
    app.dependency_overrides[file.get_archive_service] = lambda: test_archive_service
    # rebuild レート制限をリセット (monotonic 起点未定のため十分過去に設定)
    search._last_rebuild_time = -99999.0

    app.dependency_overrides[search.get_indexer] = lambda: test_indexer
    app.dependency_overrides[search.get_node_registry] = lambda: test_node_registry
    app.dependency_overrides[search.get_path_security] = lambda: (
        test_node_registry.path_security
    )
    # get_settings は config モジュール関数。test_settings フィクスチャが
    # init 済みだが、run_in_threadpool 内から呼ばれるため明示的に override
    from backend.config import get_settings as _get_settings

    app.dependency_overrides[_get_settings] = lambda: Settings()

    from backend.services.temp_file_cache import TempFileCache

    test_temp_cache = TempFileCache(
        cache_dir=test_root / ".disk-cache",
        max_size_bytes=100 * 1024 * 1024,
    )
    app.dependency_overrides[file.get_temp_file_cache] = lambda: test_temp_cache

    from backend.services.video_converter import VideoConverter

    test_converter = VideoConverter(temp_cache=test_temp_cache, timeout=30)
    app.dependency_overrides[file.get_video_converter] = lambda: test_converter

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as ac:
        yield ac

    app.dependency_overrides.clear()
