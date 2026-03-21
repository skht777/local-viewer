"""テスト共通フィクスチャ."""

from collections.abc import AsyncGenerator

import pytest
from httpx import ASGITransport, AsyncClient

from backend.main import app


@pytest.fixture
async def client() -> AsyncGenerator[AsyncClient]:
    """FastAPI TestClient (httpx AsyncClient).

    テストごとにインスタンスを生成する。
    """
    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as ac:
        yield ac
