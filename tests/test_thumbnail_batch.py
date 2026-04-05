"""バッチサムネイル API のテスト."""

import base64
import io
import zipfile
from pathlib import Path

from httpx import AsyncClient
from PIL import Image

from py_backend.services.node_registry import NodeRegistry


async def test_バッチサムネイルが複数画像で200を返しbase64データを含む(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    # dir_a の画像と dir_a/archive.zip のエントリを用意
    entries = test_node_registry.list_directory(test_root / "dir_a")
    image_entry = next(e for e in entries if e.name == "image.jpg")

    response = await client.post(
        "/api/thumbnails/batch",
        json={"node_ids": [image_entry.node_id]},
    )
    assert response.status_code == 200

    data = response.json()
    assert "thumbnails" in data
    thumb = data["thumbnails"][image_entry.node_id]
    assert "data" in thumb
    assert "etag" in thumb
    # base64 デコードして JPEG ヘッダを確認
    jpeg_bytes = base64.b64decode(thumb["data"])
    assert jpeg_bytes[:2] == b"\xff\xd8"


async def test_バッチサムネイルで不正なnode_idがcode付きエラーで返る(
    client: AsyncClient,
) -> None:
    response = await client.post(
        "/api/thumbnails/batch",
        json={"node_ids": ["nonexistent_id_12345"]},
    )
    assert response.status_code == 200

    data = response.json()
    thumb = data["thumbnails"]["nonexistent_id_12345"]
    assert "error" in thumb
    assert "code" in thumb
    assert thumb["code"] == "NOT_FOUND"


async def test_バッチサムネイルで51件以上は422を返す(
    client: AsyncClient,
) -> None:
    node_ids = [f"fake_id_{i:04d}" for i in range(51)]
    response = await client.post(
        "/api/thumbnails/batch",
        json={"node_ids": node_ids},
    )
    assert response.status_code == 422


async def test_バッチサムネイルで部分的エラーが混在する(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    # 有効な画像ノードと不正なノードを混在させる
    entries = test_node_registry.list_directory(test_root / "dir_a")
    image_entry = next(e for e in entries if e.name == "image.jpg")

    response = await client.post(
        "/api/thumbnails/batch",
        json={"node_ids": [image_entry.node_id, "nonexistent_id"]},
    )
    assert response.status_code == 200

    data = response.json()
    # 画像は成功
    assert "data" in data["thumbnails"][image_entry.node_id]
    # 不正 ID はエラー
    assert "error" in data["thumbnails"]["nonexistent_id"]
    assert "code" in data["thumbnails"]["nonexistent_id"]


async def test_バッチサムネイルでアーカイブエントリが処理される(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    from py_backend.services.archive_security import ArchiveEntryValidator
    from py_backend.services.archive_service import ArchiveService

    from py_backend.config import Settings

    settings = Settings()
    validator = ArchiveEntryValidator(settings)
    archive_service = ArchiveService(validator=validator)

    archive_path = test_root / "dir_a" / "archive.zip"
    archive_entries = archive_service.list_entries(archive_path)
    entry_metas = test_node_registry.list_archive_entries(
        archive_path, archive_entries
    )
    # 先頭の画像エントリの node_id を取得
    image_meta = next(e for e in entry_metas if e.name.endswith(".jpg"))

    response = await client.post(
        "/api/thumbnails/batch",
        json={"node_ids": [image_meta.node_id]},
    )
    assert response.status_code == 200

    data = response.json()
    thumb = data["thumbnails"][image_meta.node_id]
    assert "data" in thumb
    assert "etag" in thumb


async def test_空のnode_idsリストで空レスポンスが返る(
    client: AsyncClient,
) -> None:
    response = await client.post(
        "/api/thumbnails/batch",
        json={"node_ids": []},
    )
    assert response.status_code == 200
    data = response.json()
    assert data["thumbnails"] == {}
