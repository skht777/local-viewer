"""検索 API のテスト.

GET /api/search — キーワード検索
POST /api/index/rebuild — インデックス再構築
"""

from pathlib import Path

from httpx import AsyncClient

from py_backend.services.indexer import IndexEntry, Indexer
from py_backend.services.node_registry import NodeRegistry


async def test_検索APIが結果を返す(
    search_client: AsyncClient,
    test_indexer: Indexer,
    test_root: Path,
) -> None:
    # test_root/photo.png は conftest で作成済み
    img = test_root / "photo.png"
    rel = str(img.relative_to(test_root))
    test_indexer.add_entry(
        IndexEntry(rel, "photo.png", "image", 100, img.stat().st_mtime_ns)
    )
    response = await search_client.get("/api/search", params={"q": "photo"})
    assert response.status_code == 200
    data = response.json()
    assert len(data["results"]) == 1
    assert data["results"][0]["name"] == "photo.png"
    assert data["query"] == "photo"


async def test_空クエリで422を返す(search_client: AsyncClient) -> None:
    response = await search_client.get("/api/search", params={"q": ""})
    assert response.status_code == 422


async def test_1文字クエリで422を返す(search_client: AsyncClient) -> None:
    response = await search_client.get("/api/search", params={"q": "a"})
    assert response.status_code == 422


async def test_200文字超クエリで422を返す(search_client: AsyncClient) -> None:
    response = await search_client.get("/api/search", params={"q": "a" * 201})
    assert response.status_code == 422


async def test_kind指定フィルタが動作する(
    search_client: AsyncClient,
    test_indexer: Indexer,
    test_root: Path,
) -> None:
    # photo.png (image) と dir_b/video.mp4 (video)
    img = test_root / "photo.png"
    vid = test_root / "dir_b" / "video.mp4"
    test_indexer.add_entry(
        IndexEntry(
            str(img.relative_to(test_root)),
            "photo.png",
            "image",
            100,
            img.stat().st_mtime_ns,
        )
    )
    test_indexer.add_entry(
        IndexEntry(
            str(vid.relative_to(test_root)),
            "video.mp4",
            "video",
            1024,
            vid.stat().st_mtime_ns,
        )
    )

    # image のみ
    response = await search_client.get(
        "/api/search", params={"q": "photo", "kind": "image"}
    )
    data = response.json()
    assert len(data["results"]) == 1
    assert data["results"][0]["kind"] == "image"


async def test_不正なkindで422を返す(search_client: AsyncClient) -> None:
    response = await search_client.get(
        "/api/search", params={"q": "test", "kind": "invalid"}
    )
    assert response.status_code == 422


async def test_limitとoffsetでページネーション(
    search_client: AsyncClient,
    test_indexer: Indexer,
    test_root: Path,
) -> None:
    # 実ファイルを 10 個作成してインデックス
    items_dir = test_root / "items"
    items_dir.mkdir()
    for i in range(10):
        fp = items_dir / f"item_{i:03d}.jpg"
        fp.write_bytes(b"\xff\xd8" * 10)
        test_indexer.add_entry(
            IndexEntry(
                str(fp.relative_to(test_root)),
                f"item_{i:03d}.jpg",
                "image",
                20,
                fp.stat().st_mtime_ns,
            )
        )

    r1 = await search_client.get("/api/search", params={"q": "item", "limit": 3})
    d1 = r1.json()
    assert len(d1["results"]) == 3
    assert d1["has_more"] is True

    r2 = await search_client.get("/api/search", params={"q": "item", "limit": 100})
    d2 = r2.json()
    assert len(d2["results"]) == 10
    assert d2["has_more"] is False


async def test_has_moreが正しく返る(
    search_client: AsyncClient,
    test_indexer: Indexer,
    test_root: Path,
) -> None:
    files_dir = test_root / "files"
    files_dir.mkdir()
    for i in range(5):
        fp = files_dir / f"file_{i}.jpg"
        fp.write_bytes(b"\xff\xd8" * 10)
        test_indexer.add_entry(
            IndexEntry(
                str(fp.relative_to(test_root)),
                f"file_{i}.jpg",
                "image",
                20,
                fp.stat().st_mtime_ns,
            )
        )

    r = await search_client.get("/api/search", params={"q": "file", "limit": 5})
    assert r.json()["has_more"] is False

    r = await search_client.get("/api/search", params={"q": "file", "limit": 4})
    assert r.json()["has_more"] is True


async def test_検索結果にnode_idとparent_node_idが含まれる(
    search_client: AsyncClient,
    test_indexer: Indexer,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    # test_root 配下に実ファイルを作成
    img = test_root / "dir_a" / "image.jpg"
    rel = str(img.relative_to(test_root))
    test_indexer.add_entry(
        IndexEntry(rel, "image.jpg", "image", 100, img.stat().st_mtime_ns)
    )

    response = await search_client.get("/api/search", params={"q": "image"})
    data = response.json()
    assert len(data["results"]) >= 1
    result = data["results"][0]
    assert "node_id" in result
    assert result["node_id"] != ""
    assert "parent_node_id" in result


async def test_存在しないファイルの結果がスキップされる(
    search_client: AsyncClient,
    test_indexer: Indexer,
) -> None:
    # DB にはあるがファイルシステムに存在しないエントリ
    test_indexer.add_entry(
        IndexEntry("nonexistent/ghost.jpg", "ghost.jpg", "image", 1000, 100)
    )

    response = await search_client.get("/api/search", params={"q": "ghost"})
    data = response.json()
    # ファイルが存在しないためスキップされ、結果は 0 件
    assert len(data["results"]) == 0


async def test_インデックス未構築で503を返す(
    search_client: AsyncClient,
    test_indexer: Indexer,
) -> None:
    # is_ready を False に戻す (初期状態)
    test_indexer._is_ready = False

    response = await search_client.get("/api/search", params={"q": "test"})
    assert response.status_code == 503


async def test_パス途中のディレクトリ名でヒットする(
    search_client: AsyncClient,
    test_indexer: Indexer,
    test_root: Path,
) -> None:
    img = test_root / "dir_a" / "image.jpg"
    rel = str(img.relative_to(test_root))
    test_indexer.add_entry(
        IndexEntry(rel, "image.jpg", "image", 100, img.stat().st_mtime_ns)
    )

    # "dir_a" はパスの途中にある
    response = await search_client.get("/api/search", params={"q": "dir_a"})
    data = response.json()
    assert len(data["results"]) >= 1


async def test_rebuildが202を返す(search_client: AsyncClient) -> None:
    response = await search_client.post("/api/index/rebuild")
    assert response.status_code == 202
    data = response.json()
    assert data["status"] == "accepted"


async def test_rebuildのレート制限で429を返す(
    search_client: AsyncClient,
) -> None:
    # 1 回目は成功
    r1 = await search_client.post("/api/index/rebuild")
    assert r1.status_code == 202

    # 2 回目は 429 (60 秒以内)
    r2 = await search_client.post("/api/index/rebuild")
    assert r2.status_code == 429


async def test_warm_start状態でis_staleがTrueのレスポンスを返す(
    search_client: AsyncClient,
    test_indexer: Indexer,
    test_root: Path,
) -> None:
    test_indexer._is_stale = True

    img = test_root / "photo.png"
    rel = str(img.relative_to(test_root))
    test_indexer.add_entry(
        IndexEntry(rel, "photo.png", "image", 100, img.stat().st_mtime_ns)
    )

    response = await search_client.get("/api/search", params={"q": "photo"})
    assert response.status_code == 200
    data = response.json()
    assert data["is_stale"] is True


async def test_スキャン完了後にis_staleがFalseのレスポンスを返す(
    search_client: AsyncClient,
    test_indexer: Indexer,
    test_root: Path,
) -> None:
    img = test_root / "photo.png"
    rel = str(img.relative_to(test_root))
    test_indexer.add_entry(
        IndexEntry(rel, "photo.png", "image", 100, img.stat().st_mtime_ns)
    )

    response = await search_client.get("/api/search", params={"q": "photo"})
    assert response.status_code == 200
    data = response.json()
    assert data["is_stale"] is False
