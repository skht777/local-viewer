"""ディレクトリ閲覧 API のテスト."""

from pathlib import Path

from httpx import AsyncClient

from backend.services.node_registry import NodeRegistry


async def test_ルート一覧が200を返す(client: AsyncClient) -> None:
    response = await client.get("/api/browse")
    assert response.status_code == 200


async def test_ルート一覧にエントリが含まれる(client: AsyncClient) -> None:
    response = await client.get("/api/browse")
    data = response.json()
    assert len(data["entries"]) > 0


async def test_ルート一覧のcurrent_node_idがNone(client: AsyncClient) -> None:
    response = await client.get("/api/browse")
    data = response.json()
    assert data["current_node_id"] is None


async def test_ディレクトリのbrowseが200を返す(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    # まずルート一覧で dir_a の node_id を取得
    root = test_node_registry.path_security.root_dir
    entries = test_node_registry.list_directory(root)
    dir_a = next(e for e in entries if e.name == "dir_a")

    response = await client.get(f"/api/browse/{dir_a.node_id}")
    assert response.status_code == 200


async def test_ディレクトリのbrowseに子エントリが含まれる(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dir
    entries = test_node_registry.list_directory(root)
    dir_a = next(e for e in entries if e.name == "dir_a")

    response = await client.get(f"/api/browse/{dir_a.node_id}")
    data = response.json()
    names = [e["name"] for e in data["entries"]]
    assert "image.jpg" in names
    assert "nested" in names


async def test_ディレクトリのbrowseでparent_node_idが返る(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dir
    entries = test_node_registry.list_directory(root)
    dir_a = next(e for e in entries if e.name == "dir_a")

    # dir_a/nested の node_id を取得
    nested_entries = test_node_registry.list_directory(root / "dir_a")
    nested = next(e for e in nested_entries if e.name == "nested")

    response = await client.get(f"/api/browse/{nested.node_id}")
    data = response.json()
    # nested の親は dir_a
    assert data["parent_node_id"] == dir_a.node_id


async def test_ファイルのnode_idでbrowseすると422(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dir
    entries = test_node_registry.list_directory(root)
    file_entry = next(e for e in entries if e.name == "file.txt")

    response = await client.get(f"/api/browse/{file_entry.node_id}")
    assert response.status_code == 422


async def test_存在しないnode_idで404を返す(client: AsyncClient) -> None:
    response = await client.get("/api/browse/nonexistent12345")
    assert response.status_code == 404


async def test_エントリのメタ情報が正しい(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dir
    entries = test_node_registry.list_directory(root)
    dir_a = next(e for e in entries if e.name == "dir_a")

    response = await client.get(f"/api/browse/{dir_a.node_id}")
    data = response.json()
    entry_map = {e["name"]: e for e in data["entries"]}

    assert entry_map["image.jpg"]["kind"] == "image"
    assert entry_map["image.jpg"]["mime_type"] == "image/jpeg"
    assert entry_map["image.jpg"]["size_bytes"] > 0


async def test_エントリがディレクトリ優先でソートされている(
    client: AsyncClient,
) -> None:
    response = await client.get("/api/browse")
    data = response.json()
    kinds = [e["kind"] for e in data["entries"]]

    # ディレクトリが全てファイルより前に来る
    dir_done = False
    for kind in kinds:
        if kind != "directory":
            dir_done = True
        assert not (dir_done and kind == "directory"), (
            "ディレクトリがファイルの後に出現"
        )


# --- アーカイブ browse テスト ---


async def test_アーカイブのnode_idでbrowseするとエントリ一覧を返す(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    archive = test_root / "dir_a" / "archive.zip"
    node_id = test_node_registry.register(archive)
    response = await client.get(f"/api/browse/{node_id}")
    assert response.status_code == 200
    data = response.json()
    assert len(data["entries"]) == 2  # page01.jpg, page02.jpg (readme.txt は除外)


async def test_アーカイブのbrowseでparent_node_idが正しい(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    archive = test_root / "dir_a" / "archive.zip"
    node_id = test_node_registry.register(archive)
    parent_id = test_node_registry.register(test_root / "dir_a")
    response = await client.get(f"/api/browse/{node_id}")
    data = response.json()
    assert data["parent_node_id"] == parent_id


async def test_アーカイブ内の画像エントリのkindがimageである(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    archive = test_root / "dir_a" / "archive.zip"
    node_id = test_node_registry.register(archive)
    response = await client.get(f"/api/browse/{node_id}")
    data = response.json()
    for entry in data["entries"]:
        assert entry["kind"] == "image"


async def test_アーカイブ内の非画像エントリが除外される(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    archive = test_root / "dir_a" / "archive.zip"
    node_id = test_node_registry.register(archive)
    response = await client.get(f"/api/browse/{node_id}")
    data = response.json()
    names = [e["name"] for e in data["entries"]]
    assert "readme.txt" not in names


async def test_既存のディレクトリbrowseが引き続き動作する(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    dir_node_id = test_node_registry.register(test_root / "dir_a")
    response = await client.get(f"/api/browse/{dir_node_id}")
    assert response.status_code == 200
    data = response.json()
    # dir_a には image.jpg, nested/, archive.zip がある
    assert len(data["entries"]) >= 2
