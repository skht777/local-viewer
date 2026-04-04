"""browse ページネーション + サーバーサイドソートのテスト."""

import io
from pathlib import Path

from httpx import AsyncClient
from PIL import Image

from backend.services.node_registry import NodeRegistry


def _create_many_files(root: Path, count: int = 10) -> Path:
    """テスト用にファイルの多いディレクトリを作成する."""
    d = root / "many_files"
    d.mkdir(exist_ok=True)

    buf = io.BytesIO()
    Image.new("RGB", (1, 1), color="red").save(buf, format="JPEG")
    jpeg = buf.getvalue()

    for i in range(count):
        (d / f"img_{i:03d}.jpg").write_bytes(jpeg)
    return d


async def test_limitパラメータでエントリ数が制限される(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    d = _create_many_files(test_root, count=10)
    node_id = test_node_registry.register(d)

    response = await client.get(f"/api/browse/{node_id}?limit=3")
    assert response.status_code == 200

    data = response.json()
    assert len(data["entries"]) == 3
    assert data["next_cursor"] is not None
    assert data["total_count"] == 10


async def test_next_cursorで次ページが取得できる(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    d = _create_many_files(test_root, count=10)
    node_id = test_node_registry.register(d)

    # 1 ページ目
    r1 = await client.get(f"/api/browse/{node_id}?limit=5")
    assert r1.status_code == 200
    d1 = r1.json()
    assert len(d1["entries"]) == 5
    cursor = d1["next_cursor"]
    assert cursor is not None

    # 2 ページ目
    r2 = await client.get(f"/api/browse/{node_id}?limit=5&cursor={cursor}")
    assert r2.status_code == 200
    d2 = r2.json()
    assert len(d2["entries"]) == 5

    # 重複なし
    ids1 = {e["node_id"] for e in d1["entries"]}
    ids2 = {e["node_id"] for e in d2["entries"]}
    assert ids1.isdisjoint(ids2)


async def test_最終ページでnext_cursorがnullになる(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    d = _create_many_files(test_root, count=5)
    node_id = test_node_registry.register(d)

    response = await client.get(f"/api/browse/{node_id}?limit=10")
    assert response.status_code == 200

    data = response.json()
    assert len(data["entries"]) == 5
    assert data["next_cursor"] is None


async def test_カーソル改ざんで400を返す(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    d = _create_many_files(test_root, count=5)
    node_id = test_node_registry.register(d)

    response = await client.get(
        f"/api/browse/{node_id}?limit=3&cursor=INVALID_CURSOR"
    )
    assert response.status_code == 400


async def test_ソートパラメータが適用される(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    d = _create_many_files(test_root, count=5)
    node_id = test_node_registry.register(d)

    # name-asc
    r_asc = await client.get(f"/api/browse/{node_id}?sort=name-asc")
    assert r_asc.status_code == 200
    names_asc = [e["name"] for e in r_asc.json()["entries"]]

    # name-desc
    r_desc = await client.get(f"/api/browse/{node_id}?sort=name-desc")
    assert r_desc.status_code == 200
    names_desc = [e["name"] for e in r_desc.json()["entries"]]

    assert names_asc == list(reversed(names_desc))


async def test_limitなしで全エントリが返る_後方互換(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    d = _create_many_files(test_root, count=10)
    node_id = test_node_registry.register(d)

    # limit なし → 全件返却
    response = await client.get(f"/api/browse/{node_id}")
    assert response.status_code == 200

    data = response.json()
    assert len(data["entries"]) == 10
    # 後方互換: next_cursor は null
    assert data.get("next_cursor") is None


async def test_name_descでディレクトリがファイルより前に来る(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """name-desc ソートでもディレクトリ優先が維持されることを検証."""
    # ディレクトリとファイルが混在するディレクトリを作成
    d = test_root / "mixed"
    d.mkdir()
    (d / "subdir_a").mkdir()
    (d / "subdir_b").mkdir()

    buf = io.BytesIO()
    Image.new("RGB", (1, 1), color="red").save(buf, format="JPEG")
    jpeg = buf.getvalue()
    (d / "alpha.jpg").write_bytes(jpeg)
    (d / "zulu.jpg").write_bytes(jpeg)

    node_id = test_node_registry.register(d)

    response = await client.get(f"/api/browse/{node_id}?sort=name-desc")
    assert response.status_code == 200

    entries = response.json()["entries"]
    kinds = [e["kind"] for e in entries]

    # ディレクトリが先、ファイルが後
    dir_indices = [i for i, k in enumerate(kinds) if k == "directory"]
    file_indices = [i for i, k in enumerate(kinds) if k != "directory"]
    assert all(di < fi for di in dir_indices for fi in file_indices), (
        f"ディレクトリがファイルより前に来るべき: {[(e['name'], e['kind']) for e in entries]}"
    )

    # ディレクトリ内は名前降順
    dir_names = [entries[i]["name"] for i in dir_indices]
    assert dir_names == sorted(dir_names, reverse=True)

    # ファイル内も名前降順
    file_names = [entries[i]["name"] for i in file_indices]
    assert file_names == sorted(file_names, reverse=True)
