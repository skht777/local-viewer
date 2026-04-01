"""ファイル配信 API のテスト."""

import io
import os
import zipfile
from collections.abc import AsyncGenerator
from pathlib import Path

import pytest
from httpx import ASGITransport, AsyncClient
from PIL import Image

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


async def test_ファイル配信が200を返す(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dirs[0]
    entries = test_node_registry.list_directory(root)
    file_entry = next(e for e in entries if e.name == "file.txt")

    response = await client.get(f"/api/file/{file_entry.node_id}")
    assert response.status_code == 200


async def test_ファイル配信の内容が正しい(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dirs[0]
    entries = test_node_registry.list_directory(root)
    file_entry = next(e for e in entries if e.name == "file.txt")

    response = await client.get(f"/api/file/{file_entry.node_id}")
    assert response.text == "hello"


async def test_ETagヘッダが返る(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dirs[0]
    entries = test_node_registry.list_directory(root)
    file_entry = next(e for e in entries if e.name == "file.txt")

    response = await client.get(f"/api/file/{file_entry.node_id}")
    assert "etag" in response.headers


async def test_CacheControlヘッダが返る(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dirs[0]
    entries = test_node_registry.list_directory(root)
    file_entry = next(e for e in entries if e.name == "file.txt")

    response = await client.get(f"/api/file/{file_entry.node_id}")
    assert response.headers.get("cache-control") == "private, max-age=3600"


async def test_IfNoneMatchで304を返す(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dirs[0]
    entries = test_node_registry.list_directory(root)
    file_entry = next(e for e in entries if e.name == "file.txt")

    # まず通常リクエストで ETag を取得
    first = await client.get(f"/api/file/{file_entry.node_id}")
    etag = first.headers["etag"]

    # 同じ ETag で条件付きリクエスト
    response = await client.get(
        f"/api/file/{file_entry.node_id}",
        headers={"If-None-Match": etag},
    )
    assert response.status_code == 304


async def test_IfNoneMatchが不一致なら200を返す(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dirs[0]
    entries = test_node_registry.list_directory(root)
    file_entry = next(e for e in entries if e.name == "file.txt")

    response = await client.get(
        f"/api/file/{file_entry.node_id}",
        headers={"If-None-Match": '"wrong-etag"'},
    )
    assert response.status_code == 200


async def test_存在しないnode_idで404を返す(client: AsyncClient) -> None:
    response = await client.get("/api/file/nonexistent12345")
    assert response.status_code == 404


async def test_ディレクトリのnode_idで422を返す(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dirs[0]
    entries = test_node_registry.list_directory(root)
    dir_entry = next(e for e in entries if e.name == "dir_a")

    response = await client.get(f"/api/file/{dir_entry.node_id}")
    assert response.status_code == 422


async def test_MIMEタイプがContentTypeに反映される(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dirs[0]
    dir_a_entries = test_node_registry.list_directory(root / "dir_a")
    img_entry = next(e for e in dir_a_entries if e.name == "image.jpg")

    response = await client.get(f"/api/file/{img_entry.node_id}")
    assert "image/jpeg" in response.headers.get("content-type", "")


async def test_Rangeリクエストで206を返す(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dirs[0]
    entries = test_node_registry.list_directory(root)
    file_entry = next(e for e in entries if e.name == "file.txt")

    response = await client.get(
        f"/api/file/{file_entry.node_id}",
        headers={"Range": "bytes=0-2"},
    )
    assert response.status_code == 206
    assert response.text == "hel"


# --- アーカイブエントリ配信テスト ---

# Pillow で生成した有効な最小 JPEG (conftest.py と同一)
_img = Image.new("RGB", (1, 1), color="red")
_buf = io.BytesIO()
_img.save(_buf, format="JPEG")
_MINIMAL_JPEG = _buf.getvalue()


async def test_アーカイブエントリのnode_idでファイル配信される(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    # archive.zip 内の page01.jpg にアクセス
    archive = test_root / "dir_a" / "archive.zip"
    # まず browse でエントリの node_id を取得
    archive_node_id = test_node_registry.register(archive)
    browse_resp = await client.get(f"/api/browse/{archive_node_id}")
    entries = browse_resp.json()["entries"]
    entry_node_id = entries[0]["node_id"]

    response = await client.get(f"/api/file/{entry_node_id}")
    assert response.status_code == 200


async def test_アーカイブエントリの内容が正しい(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    archive = test_root / "dir_a" / "archive.zip"
    archive_node_id = test_node_registry.register(archive)
    browse_resp = await client.get(f"/api/browse/{archive_node_id}")
    entries = browse_resp.json()["entries"]
    entry_node_id = entries[0]["node_id"]

    response = await client.get(f"/api/file/{entry_node_id}")
    assert response.content == _MINIMAL_JPEG


async def test_アーカイブエントリのMIMEタイプが正しい(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    archive = test_root / "dir_a" / "archive.zip"
    archive_node_id = test_node_registry.register(archive)
    browse_resp = await client.get(f"/api/browse/{archive_node_id}")
    entries = browse_resp.json()["entries"]
    entry_node_id = entries[0]["node_id"]

    response = await client.get(f"/api/file/{entry_node_id}")
    assert "image/jpeg" in response.headers.get("content-type", "")


async def test_アーカイブエントリのETagが返る(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    archive = test_root / "dir_a" / "archive.zip"
    archive_node_id = test_node_registry.register(archive)
    browse_resp = await client.get(f"/api/browse/{archive_node_id}")
    entries = browse_resp.json()["entries"]
    entry_node_id = entries[0]["node_id"]

    response = await client.get(f"/api/file/{entry_node_id}")
    assert "etag" in response.headers


async def test_アーカイブエントリのIfNoneMatchで304を返す(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    archive = test_root / "dir_a" / "archive.zip"
    archive_node_id = test_node_registry.register(archive)
    browse_resp = await client.get(f"/api/browse/{archive_node_id}")
    entries = browse_resp.json()["entries"]
    entry_node_id = entries[0]["node_id"]

    # 1回目: ETag を取得
    resp1 = await client.get(f"/api/file/{entry_node_id}")
    etag = resp1.headers["etag"]

    # 2回目: If-None-Match で 304
    resp2 = await client.get(
        f"/api/file/{entry_node_id}",
        headers={"If-None-Match": etag},
    )
    assert resp2.status_code == 304


async def test_既存のファイル配信が引き続き動作する(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    # 通常ファイルの配信が壊れていないことを確認
    file_path = test_root / "file.txt"
    node_id = test_node_registry.register(file_path)
    response = await client.get(f"/api/file/{node_id}")
    assert response.status_code == 200
    assert response.text == "hello"


# --- 抽出時サイズ上限超過のAPIテスト ---


@pytest.fixture
async def client_with_small_limit(
    tmp_path: Path,
) -> AsyncGenerator[tuple[AsyncClient, NodeRegistry]]:
    """抽出サイズ上限を小さく設定した TestClient."""
    # テスト用ディレクトリ構造
    (tmp_path / "dir_a").mkdir()
    (tmp_path / "file.txt").write_text("hello")

    # 1KB のダミー画像を含む ZIP
    zip_path = tmp_path / "dir_a" / "big.zip"
    with zipfile.ZipFile(zip_path, "w") as zf:
        zf.writestr("large.jpg", b"\xff\xd8" + b"\x00" * 1024)

    os.environ["MOUNT_BASE_DIR"] = str(tmp_path)
    # 抽出上限を 512 バイトに設定
    os.environ["ARCHIVE_MAX_ENTRY_SIZE"] = "512"
    settings = Settings()
    os.environ.pop("ARCHIVE_MAX_ENTRY_SIZE")

    ps = PathSecurity(settings)
    registry = NodeRegistry(ps)
    validator = ArchiveEntryValidator(settings)
    archive_svc = ArchiveService(validator=validator)

    from backend.services.temp_file_cache import TempFileCache

    temp_cache = TempFileCache(
        cache_dir=tmp_path / ".disk-cache",
        max_size_bytes=100 * 1024 * 1024,
    )

    app.dependency_overrides[browse.get_node_registry] = lambda: registry
    app.dependency_overrides[file.get_node_registry] = lambda: registry
    app.dependency_overrides[browse.get_archive_service] = lambda: archive_svc
    app.dependency_overrides[file.get_archive_service] = lambda: archive_svc
    app.dependency_overrides[file.get_temp_file_cache] = lambda: temp_cache

    from backend.services.video_converter import VideoConverter

    converter = VideoConverter(temp_cache=temp_cache, timeout=30)
    app.dependency_overrides[file.get_video_converter] = lambda: converter

    app.add_exception_handler(PathSecurityError, path_security_error_handler)  # type: ignore[arg-type]
    app.add_exception_handler(NodeNotFoundError, node_not_found_error_handler)  # type: ignore[arg-type]
    app.add_exception_handler(ArchiveSecurityError, archive_security_error_handler)  # type: ignore[arg-type]
    app.add_exception_handler(ArchivePasswordError, archive_password_error_handler)  # type: ignore[arg-type]

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as ac:
        yield ac, registry

    app.dependency_overrides.clear()
    os.environ.pop("MOUNT_BASE_DIR", None)


async def test_抽出上限超過でAPIが422を返す(
    client_with_small_limit: tuple[AsyncClient, NodeRegistry],
) -> None:
    """アーカイブエントリの抽出がサイズ上限を超えた場合、422 を返す.

    list_entries のメタデータ検証はパスするが、実データ抽出時に上限を超えるケース:
    NodeRegistry に直接エントリを登録してテスト。
    """
    ac, registry = client_with_small_limit
    root = registry.path_security.root_dirs[0]

    archive = root / "dir_a" / "big.zip"
    entry_node_id = registry.register_archive_entry(archive, "large.jpg")

    response = await ac.get(f"/api/file/{entry_node_id}")
    assert response.status_code == 422
