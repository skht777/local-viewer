"""ディレクトリ閲覧 API のテスト."""

import zipfile
from pathlib import Path

from httpx import AsyncClient

from backend.services.dir_index import DirIndex
from backend.services.node_registry import NodeRegistry


async def test_ディレクトリのbrowseが200を返す(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    # まずルート一覧で dir_a の node_id を取得
    root = test_node_registry.path_security.root_dirs[0]
    entries = test_node_registry.list_directory(root)
    dir_a = next(e for e in entries if e.name == "dir_a")

    response = await client.get(f"/api/browse/{dir_a.node_id}")
    assert response.status_code == 200


async def test_ディレクトリのbrowseに子エントリが含まれる(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dirs[0]
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
    root = test_node_registry.path_security.root_dirs[0]
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
    root = test_node_registry.path_security.root_dirs[0]
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
    root = test_node_registry.path_security.root_dirs[0]
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
    test_node_registry: NodeRegistry,
) -> None:
    # ルートディレクトリの node_id を取得して browse
    root = test_node_registry.path_security.root_dirs[0]
    root_id = test_node_registry.register(root)
    response = await client.get(f"/api/browse/{root_id}")
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


# --- ancestors テスト ---


async def test_browseレスポンスにancestorsフィールドが含まれる(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dirs[0]
    entries = test_node_registry.list_directory(root)
    dir_a = next(e for e in entries if e.name == "dir_a")

    response = await client.get(f"/api/browse/{dir_a.node_id}")
    data = response.json()
    assert "ancestors" in data
    assert isinstance(data["ancestors"], list)


async def test_ルートディレクトリのbrowseでancestorsが空(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dirs[0]
    root_id = test_node_registry.register(root)

    response = await client.get(f"/api/browse/{root_id}")
    data = response.json()
    assert data["ancestors"] == []


async def test_ネストされたディレクトリのbrowseでancestorsが正しい(
    client: AsyncClient, test_node_registry: NodeRegistry
) -> None:
    root = test_node_registry.path_security.root_dirs[0]
    entries = test_node_registry.list_directory(root)
    dir_a = next(e for e in entries if e.name == "dir_a")
    nested_entries = test_node_registry.list_directory(root / "dir_a")
    nested = next(e for e in nested_entries if e.name == "nested")

    response = await client.get(f"/api/browse/{nested.node_id}")
    data = response.json()
    # nested の ancestors は [root, dir_a]
    assert len(data["ancestors"]) == 2
    root_id = test_node_registry.register(root)
    assert data["ancestors"][0]["node_id"] == root_id
    assert data["ancestors"][1]["node_id"] == dir_a.node_id


async def test_アーカイブのbrowseでancestorsが親ディレクトリまで含まれる(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    archive = test_root / "dir_a" / "archive.zip"
    node_id = test_node_registry.register(archive)
    response = await client.get(f"/api/browse/{node_id}")
    data = response.json()
    # archive.zip の ancestors は [root, dir_a]
    assert len(data["ancestors"]) == 2
    root_id = test_node_registry.register(test_root)
    dir_a_id = test_node_registry.register(test_root / "dir_a")
    assert data["ancestors"][0]["node_id"] == root_id
    assert data["ancestors"][1]["node_id"] == dir_a_id


async def test_browseレスポンスにmodified_atフィールドが含まれる(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    dir_node_id = test_node_registry.register(test_root / "dir_a")
    response = await client.get(f"/api/browse/{dir_node_id}")
    data = response.json()
    for entry in data["entries"]:
        # ディレクトリ・ファイルともに modified_at が数値で返る
        assert "modified_at" in entry
        assert isinstance(entry["modified_at"], float)


# --- preview_node_ids テスト ---


async def test_browseレスポンスにpreview_node_idsフィールドが含まれる(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    root_id = test_node_registry.register(test_root)
    response = await client.get(f"/api/browse/{root_id}")
    data = response.json()
    # ディレクトリエントリに preview_node_ids フィールドが存在する
    dir_a = next(e for e in data["entries"] if e["name"] == "dir_a")
    assert "preview_node_ids" in dir_a


async def test_DirIndexパスでpreview_node_idsにアーカイブが含まれる(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
    tmp_path: Path,
) -> None:
    """DirIndex パス経由の browse でディレクトリの preview_node_ids が返る."""
    from io import BytesIO

    from PIL import Image

    from backend.main import app
    from backend.routers import browse

    # アーカイブのみを含むディレクトリを作成
    archive_dir = test_root / "archive_only"
    archive_dir.mkdir()
    _img = Image.new("RGB", (1, 1), color="red")
    _buf = BytesIO()
    _img.save(_buf, format="JPEG")
    minimal_jpeg = _buf.getvalue()
    with zipfile.ZipFile(archive_dir / "comic.zip", "w") as zf:
        zf.writestr("page01.jpg", minimal_jpeg)

    # DirIndex を作成し、テストデータを投入
    db_path = str(tmp_path / "browse-dir-index.db")
    di = DirIndex(db_path)
    di.init_db()
    # ルートのエントリ (archive_only ディレクトリ)
    dir_mtime_ns = archive_dir.stat().st_mtime_ns
    root_mtime_ns = test_root.stat().st_mtime_ns
    di.add_entries("", [("archive_only", "directory", None, dir_mtime_ns)])
    di.set_dir_mtime("", root_mtime_ns)
    # archive_only ディレクトリ内のエントリ
    zip_stat = (archive_dir / "comic.zip").stat()
    di.add_entries(
        "archive_only",
        [("comic.zip", "archive", zip_stat.st_size, zip_stat.st_mtime_ns)],
    )
    di.set_dir_mtime("archive_only", dir_mtime_ns)
    di.mark_full_scan_done()

    # DirIndex を DI にセット
    app.dependency_overrides[browse.get_dir_index] = lambda: di

    try:
        root_id = test_node_registry.register(test_root)
        # limit 指定で DirIndex パスを発動
        response = await client.get(f"/api/browse/{root_id}?limit=100")
        data = response.json()
        archive_dir_entry = next(
            (e for e in data["entries"] if e["name"] == "archive_only"), None
        )
        assert archive_dir_entry is not None
        assert archive_dir_entry["preview_node_ids"] is not None
        assert len(archive_dir_entry["preview_node_ids"]) >= 1
        assert archive_dir_entry["child_count"] == 1
    finally:
        app.dependency_overrides.pop(browse.get_dir_index, None)


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


# --- ETag 304 テスト ---


async def test_browseでETagが返りIfNoneMatchで304を返す(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """browse エンドポイントが ETag を返し、If-None-Match で 304 を返す."""
    dir_node_id = test_node_registry.register(test_root / "dir_a")

    # 1回目: ETag 取得
    resp1 = await client.get(f"/api/browse/{dir_node_id}")
    assert resp1.status_code == 200
    etag = resp1.headers.get("etag")
    assert etag is not None

    # 2回目: If-None-Match で 304
    resp2 = await client.get(
        f"/api/browse/{dir_node_id}",
        headers={"If-None-Match": etag},
    )
    assert resp2.status_code == 304


# --- modified_at nullability テスト ---


async def test_ディレクトリエントリのmodified_atが数値である(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """仕様注記: 現行実装ではディレクトリも modified_at が数値で返る.

    spec-architecture では「ディレクトリ/アーカイブは null」とあるが、
    NodeRegistry.list_directory は stat() を実行するため数値が返る。
    この差異を明示的に記録するテスト。
    """
    root_id = test_node_registry.register(test_root)
    resp = await client.get(f"/api/browse/{root_id}")
    data = resp.json()
    dir_entry = next(e for e in data["entries"] if e["kind"] == "directory")
    # 現行動作: ディレクトリも modified_at が数値 (仕様とは異なる)
    assert dir_entry["modified_at"] is not None
    assert isinstance(dir_entry["modified_at"], float)


async def test_アーカイブ内エントリのmodified_atがNullである(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """アーカイブ内エントリの modified_at は None."""
    archive_path = test_root / "dir_a" / "archive.zip"
    archive_node_id = test_node_registry.register(archive_path)
    resp = await client.get(f"/api/browse/{archive_node_id}")
    data = resp.json()
    for entry in data["entries"]:
        assert entry["modified_at"] is None
