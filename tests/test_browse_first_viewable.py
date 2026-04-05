"""first-viewable エンドポイントのテスト.

GET /api/browse/{node_id}/first-viewable
- 優先順位: archive > pdf > image > directory (再帰降下)
- 再帰深度制限: 最大 10 レベル
- 空ディレクトリ: entry=None
- sort パラメータ反映
"""

from pathlib import Path

import pytest
from httpx import AsyncClient


@pytest.fixture
def first_viewable_root(test_root: Path) -> Path:
    """first-viewable テスト用の追加ディレクトリ構造.

    test_root/
    ├── dir_a/
    │   ├── image.jpg       ← 画像
    │   ├── archive.zip     ← アーカイブ (優先)
    │   └── nested/
    │       └── deep.txt
    ├── dir_b/
    │   └── video.mp4
    ├── empty_dir/          ← 空ディレクトリ
    ├── nested_only/        ← ディレクトリのみ (再帰降下テスト)
    │   └── sub/
    │       └── inner_image.jpg
    ├── pdf_dir/            ← PDF のみ
    │   └── doc.pdf
    ├── file.txt
    └── photo.png
    """
    # 空ディレクトリ
    (test_root / "empty_dir").mkdir(exist_ok=True)

    # ネストされたディレクトリ (再帰降下テスト)
    nested = test_root / "nested_only" / "sub"
    nested.mkdir(parents=True, exist_ok=True)
    # Pillow の最小 JPEG を再利用
    jpeg_bytes = (test_root / "dir_a" / "image.jpg").read_bytes()
    (nested / "inner_image.jpg").write_bytes(jpeg_bytes)

    # PDF ディレクトリ
    pdf_dir = test_root / "pdf_dir"
    pdf_dir.mkdir(exist_ok=True)
    # 最小 PDF (pdfjs では開けないがバイナリとしては有効)
    (pdf_dir / "doc.pdf").write_bytes(b"%PDF-1.4 minimal")

    return test_root


async def test_画像のあるディレクトリで画像エントリを返す(
    client: AsyncClient, test_node_registry: "NodeRegistry", first_viewable_root: Path
) -> None:
    """dir_a にはアーカイブと画像がある → archive が優先される."""
    from backend.services.node_registry import NodeRegistry

    node_id = test_node_registry.register(first_viewable_root / "dir_a")
    resp = await client.get(f"/api/browse/{node_id}/first-viewable")
    assert resp.status_code == 200
    data = resp.json()
    assert data["entry"] is not None
    assert data["entry"]["kind"] == "archive"  # archive > image
    assert data["parent_node_id"] == node_id


async def test_PDFのみのディレクトリでPDFエントリを返す(
    client: AsyncClient, test_node_registry: "NodeRegistry", first_viewable_root: Path
) -> None:
    from backend.services.node_registry import NodeRegistry

    node_id = test_node_registry.register(first_viewable_root / "pdf_dir")
    resp = await client.get(f"/api/browse/{node_id}/first-viewable")
    assert resp.status_code == 200
    data = resp.json()
    assert data["entry"] is not None
    assert data["entry"]["kind"] == "pdf"


async def test_空ディレクトリでentryがNullを返す(
    client: AsyncClient, test_node_registry: "NodeRegistry", first_viewable_root: Path
) -> None:
    node_id = test_node_registry.register(first_viewable_root / "empty_dir")
    resp = await client.get(f"/api/browse/{node_id}/first-viewable")
    assert resp.status_code == 200
    data = resp.json()
    assert data["entry"] is None


async def test_ネストされたディレクトリを再帰探索して画像を返す(
    client: AsyncClient, test_node_registry: "NodeRegistry", first_viewable_root: Path
) -> None:
    """nested_only/ → sub/ → inner_image.jpg と再帰降下する."""
    node_id = test_node_registry.register(first_viewable_root / "nested_only")
    resp = await client.get(f"/api/browse/{node_id}/first-viewable")
    assert resp.status_code == 200
    data = resp.json()
    assert data["entry"] is not None
    assert data["entry"]["kind"] == "image"
    assert data["entry"]["name"] == "inner_image.jpg"


async def test_ファイルのnode_idで空レスポンスを返す(
    client: AsyncClient, test_node_registry: "NodeRegistry", first_viewable_root: Path
) -> None:
    """ディレクトリではないノードでは探索を中断し空レスポンス."""
    node_id = test_node_registry.register(first_viewable_root / "file.txt")
    resp = await client.get(f"/api/browse/{node_id}/first-viewable")
    assert resp.status_code == 200
    data = resp.json()
    assert data["entry"] is None


async def test_存在しないnode_idで404を返す(client: AsyncClient) -> None:
    resp = await client.get("/api/browse/nonexistent/first-viewable")
    assert resp.status_code == 404


async def test_sortパラメータが反映される(
    client: AsyncClient, test_node_registry: "NodeRegistry", first_viewable_root: Path
) -> None:
    """sort=name-desc でも正常にレスポンスが返る."""
    node_id = test_node_registry.register(first_viewable_root / "dir_a")
    resp = await client.get(
        f"/api/browse/{node_id}/first-viewable", params={"sort": "name-desc"}
    )
    assert resp.status_code == 200
    data = resp.json()
    assert data["entry"] is not None
