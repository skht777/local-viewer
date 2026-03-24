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
from backend.routers import browse, file
from backend.services.archive_security import (
    ArchiveEntryValidator,
    ArchivePasswordError,
    ArchiveSecurityError,
)
from backend.services.archive_service import ArchiveService
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

    # テスト用 ZIP アーカイブ
    zip_path = tmp_path / "dir_a" / "archive.zip"
    with zipfile.ZipFile(zip_path, "w") as zf:
        zf.writestr("page01.jpg", minimal_jpeg)
        zf.writestr("page02.jpg", minimal_jpeg)
        zf.writestr("readme.txt", "not an image")

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
) -> AsyncGenerator[AsyncClient]:
    """DI 差し替え済みの FastAPI TestClient.

    NodeRegistry と ArchiveService をテスト用インスタンスに差し替える。
    """
    app.dependency_overrides[browse.get_node_registry] = lambda: test_node_registry
    app.dependency_overrides[file.get_node_registry] = lambda: test_node_registry
    app.dependency_overrides[browse.get_archive_service] = lambda: test_archive_service
    app.dependency_overrides[file.get_archive_service] = lambda: test_archive_service

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
