"""サムネイル API のテスト."""

import io
import zipfile
from pathlib import Path

from httpx import AsyncClient
from PIL import Image

from backend.services.node_registry import NodeRegistry


async def test_画像ファイルのサムネイルが200で返る(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    entries = test_node_registry.list_directory(test_root / "dir_a")
    image_entry = next(e for e in entries if e.name == "image.jpg")

    response = await client.get(f"/api/thumbnail/{image_entry.node_id}")
    assert response.status_code == 200
    assert response.headers["content-type"] == "image/jpeg"


async def test_アーカイブのサムネイルが先頭画像から生成される(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    archive = test_root / "dir_a" / "archive.zip"
    node_id = test_node_registry.register(archive)

    response = await client.get(f"/api/thumbnail/{node_id}")
    assert response.status_code == 200
    assert response.headers["content-type"] == "image/jpeg"


async def test_画像がないアーカイブで404を返す(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    # 画像を含まない ZIP を作成
    no_image_zip = test_root / "no_images.zip"
    with zipfile.ZipFile(no_image_zip, "w") as zf:
        zf.writestr("readme.txt", "no images here")
    node_id = test_node_registry.register(no_image_zip)

    response = await client.get(f"/api/thumbnail/{node_id}")
    assert response.status_code == 404


async def test_ETagヘッダが返る(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    entries = test_node_registry.list_directory(test_root / "dir_a")
    image_entry = next(e for e in entries if e.name == "image.jpg")

    response = await client.get(f"/api/thumbnail/{image_entry.node_id}")
    assert "etag" in response.headers


async def test_IfNoneMatchで304を返す(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    entries = test_node_registry.list_directory(test_root / "dir_a")
    image_entry = next(e for e in entries if e.name == "image.jpg")

    # 1回目: ETag を取得
    response1 = await client.get(f"/api/thumbnail/{image_entry.node_id}")
    etag = response1.headers["etag"]

    # 2回目: If-None-Match で 304
    response2 = await client.get(
        f"/api/thumbnail/{image_entry.node_id}",
        headers={"if-none-match": etag},
    )
    assert response2.status_code == 304


async def test_存在しないnode_idで404を返す(client: AsyncClient) -> None:
    response = await client.get("/api/thumbnail/nonexistent12345")
    assert response.status_code == 404


async def test_ディレクトリnode_idで422を返す(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    dir_id = test_node_registry.register(test_root / "dir_a")
    response = await client.get(f"/api/thumbnail/{dir_id}")
    assert response.status_code == 422


async def test_不正なnode_idフォーマットで404を返す(client: AsyncClient) -> None:
    response = await client.get("/api/thumbnail/../../etc/passwd")
    assert response.status_code == 404


async def test_壊れた画像ファイルのサムネイルで422を返す(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    # 画像拡張子だが Pillow で認識できないデータ
    broken_path = test_root / "broken.jpg"
    broken_path.write_bytes(b"\xff\xd8\xff\xd9")  # SOI + EOI のみ
    node_id = test_node_registry.register(broken_path)

    response = await client.get(f"/api/thumbnail/{node_id}")
    assert response.status_code == 422


async def test_有効なPDFファイルのサムネイルが200で返る(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    # Pillow で有効な PDF を生成
    _img = Image.new("RGB", (10, 10), color="blue")
    _buf = io.BytesIO()
    _img.save(_buf, format="PDF")
    pdf_path = test_root / "valid.pdf"
    pdf_path.write_bytes(_buf.getvalue())
    node_id = test_node_registry.register(pdf_path)

    response = await client.get(f"/api/thumbnail/{node_id}")
    assert response.status_code == 200
    assert response.headers["content-type"] == "image/jpeg"


async def test_壊れたPDFファイルのサムネイルで422を返す(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    # 不正な��イト列の PDF → pyvips でデコード失敗 → 422
    pdf_path = test_root / "broken.pdf"
    pdf_path.write_bytes(b"%PDF-1.4 dummy")
    node_id = test_node_registry.register(pdf_path)

    response = await client.get(f"/api/thumbnail/{node_id}")
    assert response.status_code == 422


async def test_動画ファイルのサムネイルで422を返す(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    entries = test_node_registry.list_directory(test_root / "dir_b")
    video_entry = next(e for e in entries if e.name == "video.mp4")

    response = await client.get(f"/api/thumbnail/{video_entry.node_id}")
    assert response.status_code == 422


async def test_壊れた画像のアーカイブサムネイルで422を返す(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    # 画像拡張子だが無効なデータを含む ZIP
    bad_zip = test_root / "bad_images.zip"
    with zipfile.ZipFile(bad_zip, "w") as zf:
        zf.writestr("broken.jpg", b"\xff\xd8\xff\xd9")  # SOI + EOI のみ
    node_id = test_node_registry.register(bad_zip)

    response = await client.get(f"/api/thumbnail/{node_id}")
    assert response.status_code == 422


async def test_vパラメータ付きリクエストでimmutableヘッダが返る(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    entries = test_node_registry.list_directory(test_root / "dir_a")
    image_entry = next(e for e in entries if e.name == "image.jpg")

    response = await client.get(f"/api/thumbnail/{image_entry.node_id}?v=12345")
    assert response.status_code == 200
    cc = response.headers["cache-control"]
    assert "immutable" in cc
    assert "max-age=31536000" in cc
    assert "public" in cc


async def test_vパラメータなしリクエストで従来のキャッシュヘッダが返る(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    entries = test_node_registry.list_directory(test_root / "dir_a")
    image_entry = next(e for e in entries if e.name == "image.jpg")

    response = await client.get(f"/api/thumbnail/{image_entry.node_id}")
    assert response.status_code == 200
    cc = response.headers["cache-control"]
    assert "private" in cc
    assert "max-age=3600" in cc
    assert "immutable" not in cc


async def test_サムネイルが300px以内にリサイズされている(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    # 大きめの画像を作成
    large_img = Image.new("RGB", (1000, 800), color="blue")
    buf = io.BytesIO()
    large_img.save(buf, format="JPEG")
    large_path = test_root / "large.jpg"
    large_path.write_bytes(buf.getvalue())
    node_id = test_node_registry.register(large_path)

    response = await client.get(f"/api/thumbnail/{node_id}")
    assert response.status_code == 200
    result_img = Image.open(io.BytesIO(response.content))
    assert max(result_img.size) <= 300
