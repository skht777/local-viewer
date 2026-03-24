"""ファイル配信 API のテスト."""

from pathlib import Path

from httpx import AsyncClient

from backend.services.node_registry import NodeRegistry


async def test_ファイル配信が200を返す(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dir
    entries = test_node_registry.list_directory(root)
    file_entry = next(e for e in entries if e.name == "file.txt")

    response = await client.get(f"/api/file/{file_entry.node_id}")
    assert response.status_code == 200


async def test_ファイル配信の内容が正しい(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dir
    entries = test_node_registry.list_directory(root)
    file_entry = next(e for e in entries if e.name == "file.txt")

    response = await client.get(f"/api/file/{file_entry.node_id}")
    assert response.text == "hello"


async def test_ETagヘッダが返る(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dir
    entries = test_node_registry.list_directory(root)
    file_entry = next(e for e in entries if e.name == "file.txt")

    response = await client.get(f"/api/file/{file_entry.node_id}")
    assert "etag" in response.headers


async def test_CacheControlヘッダが返る(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dir
    entries = test_node_registry.list_directory(root)
    file_entry = next(e for e in entries if e.name == "file.txt")

    response = await client.get(f"/api/file/{file_entry.node_id}")
    assert response.headers.get("cache-control") == "private, max-age=3600"


async def test_IfNoneMatchで304を返す(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dir
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
    root = test_node_registry.path_security.root_dir
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
    root = test_node_registry.path_security.root_dir
    entries = test_node_registry.list_directory(root)
    dir_entry = next(e for e in entries if e.name == "dir_a")

    response = await client.get(f"/api/file/{dir_entry.node_id}")
    assert response.status_code == 422


async def test_MIMEタイプがContentTypeに反映される(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dir
    dir_a_entries = test_node_registry.list_directory(root / "dir_a")
    img_entry = next(e for e in dir_a_entries if e.name == "image.jpg")

    response = await client.get(f"/api/file/{img_entry.node_id}")
    assert "image/jpeg" in response.headers.get("content-type", "")


async def test_Rangeリクエストで206を返す(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dir
    entries = test_node_registry.list_directory(root)
    file_entry = next(e for e in entries if e.name == "file.txt")

    response = await client.get(
        f"/api/file/{file_entry.node_id}",
        headers={"Range": "bytes=0-2"},
    )
    assert response.status_code == 206
    assert response.text == "hel"


# --- アーカイブエントリ配信テスト ---

# 最小 JPEG (conftest.py と同一)
_MINIMAL_JPEG = bytes(
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
