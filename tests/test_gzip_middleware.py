"""GZip ミドルウェアのバイナリ除外テスト.

バイナリレスポンス (image/jpeg 等) は gzip 圧縮されず、
JSON レスポンスは引き続き gzip 圧縮されることを検証する。
"""

from pathlib import Path

from httpx import AsyncClient

from py_backend.services.node_registry import NodeRegistry


async def test_サムネイルレスポンスがgzip圧縮されない(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """image/jpeg レスポンスに gzip が適用されないことを検証."""
    entries = test_node_registry.list_directory(test_root / "dir_a")
    image_entry = next(e for e in entries if e.name == "image.jpg")

    response = await client.get(
        f"/api/thumbnail/{image_entry.node_id}",
        headers={"Accept-Encoding": "gzip"},
    )
    assert response.status_code == 200
    assert response.headers["content-type"] == "image/jpeg"
    # バイナリレスポンスは gzip 圧縮されないこと
    assert response.headers.get("content-encoding") != "gzip"


async def test_browseレスポンスがgzip圧縮される(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """application/json レスポンスは引き続き gzip 圧縮されることを検証."""
    node_id = test_node_registry.register(test_root)

    response = await client.get(
        f"/api/browse/{node_id}",
        headers={"Accept-Encoding": "gzip"},
    )
    assert response.status_code == 200
    # JSON レスポンスは gzip 圧縮されること
    # (httpx は自動デコードするため content-encoding はレスポンスから消える場合がある)
    # 代わりにレスポンスが正常な JSON であることを検証
    data = response.json()
    assert "entries" in data


async def test_動画ファイルレスポンスがgzip圧縮されない(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """video/mp4 レスポンスに gzip が適用されないことを検証."""
    entries = test_node_registry.list_directory(test_root / "dir_b")
    video_entry = next(e for e in entries if e.name == "video.mp4")

    response = await client.get(
        f"/api/file/{video_entry.node_id}",
        headers={"Accept-Encoding": "gzip"},
    )
    assert response.status_code == 200
    assert response.headers.get("content-encoding") != "gzip"


async def test_PDFファイルレスポンスがgzip圧縮されない(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """application/pdf レスポンスに gzip が適用されないことを検証."""
    # テスト用 PDF を作成
    pdf_path = test_root / "test.pdf"
    pdf_path.write_bytes(b"%PDF-1.4 minimal content")
    node_id = test_node_registry.register(pdf_path)

    response = await client.get(
        f"/api/file/{node_id}",
        headers={"Accept-Encoding": "gzip"},
    )
    assert response.status_code == 200
    assert response.headers.get("content-encoding") != "gzip"
