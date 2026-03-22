"""ファイル配信 API のテスト."""

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
