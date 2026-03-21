"""ヘルスチェックエンドポイントのテスト."""

from httpx import AsyncClient


async def test_healthエンドポイントが200を返す(client: AsyncClient) -> None:
    response = await client.get("/api/health")
    assert response.status_code == 200


async def test_healthエンドポイントがok状態を返す(client: AsyncClient) -> None:
    response = await client.get("/api/health")
    assert response.json() == {"status": "ok"}


async def test_存在しないパスで404を返す(client: AsyncClient) -> None:
    response = await client.get("/api/nonexistent")
    assert response.status_code == 404


async def test_healthエンドポイントはGETのみ許可(client: AsyncClient) -> None:
    response = await client.post("/api/health")
    assert response.status_code == 405
