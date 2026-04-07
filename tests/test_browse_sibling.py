"""sibling エンドポイントのテスト.

GET /api/browse/{parent_node_id}/sibling?current=...&direction=...&sort=...
- direction=next: 次の兄弟セット (directory/archive/pdf)
- direction=prev: 前の兄弟セット
- 見つからない場合: entry=None
- 親が非ディレクトリ: 422
"""

from pathlib import Path

import pytest
from httpx import AsyncClient

from backend.services.node_registry import NodeRegistry


@pytest.fixture
def sibling_root(test_root: Path) -> Path:
    """sibling テスト用の隔離ディレクトリ構造.

    test_root/sibling_parent/
    ├── alpha/          ← 兄弟 (directory)
    ├── beta/           ← 兄弟 (directory)
    ├── gamma/          ← 兄弟 (directory)
    ├── skip.jpg        ← 非候補 (image)
    └── skip.txt        ← 非候補 (other)
    """
    import io

    from PIL import Image

    parent = test_root / "sibling_parent"
    parent.mkdir(exist_ok=True)
    (parent / "alpha").mkdir(exist_ok=True)
    (parent / "beta").mkdir(exist_ok=True)
    (parent / "gamma").mkdir(exist_ok=True)

    _img = Image.new("RGB", (1, 1), color="red")
    _buf = io.BytesIO()
    _img.save(_buf, format="JPEG")
    (parent / "skip.jpg").write_bytes(_buf.getvalue())
    (parent / "skip.txt").write_text("text")

    return parent


async def test_nextで次の兄弟ディレクトリを返す(
    client: AsyncClient, test_node_registry: NodeRegistry, sibling_root: Path
) -> None:
    # sibling_parent 直下: alpha, beta, gamma (name-asc 順)
    parent_id = test_node_registry.register(sibling_root)
    current_id = test_node_registry.register(sibling_root / "alpha")
    resp = await client.get(
        f"/api/browse/{parent_id}/sibling",
        params={"current": current_id, "direction": "next"},
    )
    assert resp.status_code == 200
    data = resp.json()
    assert data["entry"] is not None
    assert data["entry"]["kind"] == "directory"
    assert data["entry"]["name"] == "beta"


async def test_prevで前の兄弟ディレクトリを返す(
    client: AsyncClient, test_node_registry: NodeRegistry, sibling_root: Path
) -> None:
    parent_id = test_node_registry.register(sibling_root)
    current_id = test_node_registry.register(sibling_root / "beta")
    resp = await client.get(
        f"/api/browse/{parent_id}/sibling",
        params={"current": current_id, "direction": "prev"},
    )
    assert resp.status_code == 200
    data = resp.json()
    assert data["entry"] is not None
    assert data["entry"]["name"] == "alpha"


async def test_最初の兄弟でprevはentryがNull(
    client: AsyncClient, test_node_registry: NodeRegistry, sibling_root: Path
) -> None:
    parent_id = test_node_registry.register(sibling_root)
    current_id = test_node_registry.register(sibling_root / "alpha")
    resp = await client.get(
        f"/api/browse/{parent_id}/sibling",
        params={"current": current_id, "direction": "prev"},
    )
    assert resp.status_code == 200
    assert resp.json()["entry"] is None


async def test_最後の兄弟でnextはentryがNull(
    client: AsyncClient, test_node_registry: NodeRegistry, sibling_root: Path
) -> None:
    parent_id = test_node_registry.register(sibling_root)
    current_id = test_node_registry.register(sibling_root / "gamma")
    resp = await client.get(
        f"/api/browse/{parent_id}/sibling",
        params={"current": current_id, "direction": "next"},
    )
    assert resp.status_code == 200
    assert resp.json()["entry"] is None


async def test_画像やテキストは兄弟候補に含まれない(
    client: AsyncClient, test_node_registry: NodeRegistry, sibling_root: Path
) -> None:
    """alpha → next は beta (skip.jpg/skip.txt はスキップ)."""
    parent_id = test_node_registry.register(sibling_root)
    current_id = test_node_registry.register(sibling_root / "alpha")
    resp = await client.get(
        f"/api/browse/{parent_id}/sibling",
        params={"current": current_id, "direction": "next"},
    )
    assert resp.status_code == 200
    entry = resp.json()["entry"]
    assert entry is not None
    assert entry["kind"] in ("directory", "archive", "pdf")


async def test_親が非ディレクトリで422を返す(
    client: AsyncClient, test_node_registry: NodeRegistry, sibling_root: Path
) -> None:
    file_id = test_node_registry.register(sibling_root / "skip.txt")
    current_id = test_node_registry.register(sibling_root / "alpha")
    resp = await client.get(
        f"/api/browse/{file_id}/sibling",
        params={"current": current_id, "direction": "next"},
    )
    assert resp.status_code == 422


async def test_存在しないparent_node_idで404を返す(client: AsyncClient) -> None:
    resp = await client.get(
        "/api/browse/nonexistent/sibling",
        params={"current": "dummy", "direction": "next"},
    )
    assert resp.status_code == 404


@pytest.fixture
def mixed_sibling_root(test_root: Path) -> Path:
    """ディレクトリとアーカイブが混在する sibling テスト用構造.

    test_root/mixed_parent/
    ├── dir_z/          ← ディレクトリ (名前がアーカイブより後)
    ├── apple.zip       ← アーカイブ (名前がディレクトリより前)
    └── skip.jpg        ← 非候補 (image)
    """
    import io
    import zipfile as zf

    from PIL import Image

    parent = test_root / "mixed_parent"
    parent.mkdir(exist_ok=True)
    (parent / "dir_z").mkdir(exist_ok=True)

    _img = Image.new("RGB", (1, 1), color="red")
    _buf = io.BytesIO()
    _img.save(_buf, format="JPEG")
    jpeg = _buf.getvalue()

    # skip.jpg
    (parent / "skip.jpg").write_bytes(jpeg)

    # apple.zip (画像入り)
    zip_path = parent / "apple.zip"
    with zf.ZipFile(zip_path, "w") as z:
        z.writestr("page01.jpg", jpeg)

    return parent


async def test_ディレクトリの次にアーカイブがある場合にnextを返す(
    client: AsyncClient, test_node_registry: NodeRegistry, mixed_sibling_root: Path
) -> None:
    """name-asc: dir_z (最後のディレクトリ) → next は apple.zip (最初のアーカイブ)."""
    parent_id = test_node_registry.register(mixed_sibling_root)
    current_id = test_node_registry.register(mixed_sibling_root / "dir_z")
    resp = await client.get(
        f"/api/browse/{parent_id}/sibling",
        params={"current": current_id, "direction": "next"},
    )
    assert resp.status_code == 200
    entry = resp.json()["entry"]
    assert entry is not None
    assert entry["name"] == "apple.zip"
    assert entry["kind"] == "archive"


async def test_アーカイブの前にディレクトリがある場合にprevを返す(
    client: AsyncClient, test_node_registry: NodeRegistry, mixed_sibling_root: Path
) -> None:
    """name-asc: apple.zip → prev は dir_z."""
    parent_id = test_node_registry.register(mixed_sibling_root)
    current_id = test_node_registry.register(mixed_sibling_root / "apple.zip")
    resp = await client.get(
        f"/api/browse/{parent_id}/sibling",
        params={"current": current_id, "direction": "prev"},
    )
    assert resp.status_code == 200
    entry = resp.json()["entry"]
    assert entry is not None
    assert entry["name"] == "dir_z"
    assert entry["kind"] == "directory"


async def test_sortパラメータが反映される(
    client: AsyncClient, test_node_registry: NodeRegistry, sibling_root: Path
) -> None:
    """sort=name-desc: gamma > beta > alpha → beta の next は alpha."""
    parent_id = test_node_registry.register(sibling_root)
    current_id = test_node_registry.register(sibling_root / "beta")
    resp = await client.get(
        f"/api/browse/{parent_id}/sibling",
        params={
            "current": current_id,
            "direction": "next",
            "sort": "name-desc",
        },
    )
    assert resp.status_code == 200
    entry = resp.json()["entry"]
    assert entry is not None
    assert entry["name"] == "alpha"
