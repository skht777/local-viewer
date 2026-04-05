"""マウントポイント一覧 API のテスト."""

from httpx import AsyncClient

from py_backend.services.node_registry import NodeRegistry


async def test_マウント一覧が200を返す(client: AsyncClient) -> None:
    response = await client.get("/api/mounts")
    assert response.status_code == 200


async def test_マウント一覧にmountsフィールドが含まれる(
    client: AsyncClient,
) -> None:
    response = await client.get("/api/mounts")
    data = response.json()
    assert "mounts" in data
    assert isinstance(data["mounts"], list)


async def test_マウントエントリにnode_idとnameが含まれる(
    client: AsyncClient,
) -> None:
    response = await client.get("/api/mounts")
    data = response.json()
    assert len(data["mounts"]) >= 1
    mount = data["mounts"][0]
    assert "node_id" in mount
    assert "name" in mount
    assert "child_count" in mount
    assert len(mount["node_id"]) == 16
