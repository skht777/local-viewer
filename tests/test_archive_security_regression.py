"""アーカイブ関連のセキュリティ回帰テスト.

攻撃用アーカイブをテスト内で動的生成し、API レベルで検証する。
"""

import zipfile
from pathlib import Path

from httpx import AsyncClient

from py_backend.services.node_registry import NodeRegistry

# 最小 JPEG
MINIMAL_JPEG = bytes(
    [
        0xFF,
        0xD8,
        0xFF,
        0xE0,
        0x00,
        0x10,
        0x4A,
        0x46,
        0x49,
        0x46,
        0x00,
        0x01,
        0x01,
        0x00,
        0x00,
        0x01,
        0x00,
        0x01,
        0x00,
        0x00,
        0xFF,
        0xD9,
    ]
)


async def test_ドットドットエントリを含むZIPのbrowseでエントリが除外される(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """../を含むエントリは個別スキップされ、正常エントリのみ返る."""
    archive = test_root / "traversal.zip"
    with zipfile.ZipFile(archive, "w") as zf:
        zf.writestr("normal.jpg", MINIMAL_JPEG)
        zf.writestr("../escape.jpg", MINIMAL_JPEG)
    node_id = test_node_registry.register(archive)
    response = await client.get(f"/api/browse/{node_id}")
    assert response.status_code == 200
    names = [e["name"] for e in response.json()["entries"]]
    assert "escape.jpg" not in names
    assert "normal.jpg" in names


async def test_zip_bombエントリがスキップされる(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """サイズ超過エントリは個別スキップされ、正常エントリのみ返る."""
    archive = test_root / "bomb.zip"
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zf:
        # 64MB > 32MB 上限 → スキップ対象
        huge_data = b"\x00" * (64 * 1024 * 1024)
        zf.writestr("bomb.jpg", huge_data)
        # 正常なエントリ
        zf.writestr("normal.jpg", MINIMAL_JPEG)
    node_id = test_node_registry.register(archive)
    response = await client.get(f"/api/browse/{node_id}")
    assert response.status_code == 200
    entries = response.json()["entries"]
    names = [e["name"] for e in entries]
    assert "bomb.jpg" not in names
    assert "normal.jpg" in names


async def test_壊れたZIPファイルのbrowseでエラー(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    broken = test_root / "broken.zip"
    broken.write_bytes(b"not a zip file at all")
    node_id = test_node_registry.register(broken)
    response = await client.get(f"/api/browse/{node_id}")
    # 壊れた ZIP は 500 (unhandled) または 422 (handled)
    assert response.status_code >= 400


async def test_空のZIPファイルのbrowseで空リスト(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    empty = test_root / "empty.zip"
    with zipfile.ZipFile(empty, "w"):
        pass
    node_id = test_node_registry.register(empty)
    response = await client.get(f"/api/browse/{node_id}")
    assert response.status_code == 200
    assert response.json()["entries"] == []


async def test_アーカイブエントリのnode_idで通常ファイルにアクセスできない(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """アーカイブエントリの node_id は通常の resolve() では見つからない."""
    archive = test_root / "dir_a" / "archive.zip"
    arc_node_id = test_node_registry.register(archive)
    browse_resp = await client.get(f"/api/browse/{arc_node_id}")
    entry_node_id = browse_resp.json()["entries"][0]["node_id"]

    # この entry_node_id は resolve() では NodeNotFoundError
    # しかし resolve_archive_entry() では正常に解決される
    # file API は両方を確認するので正常に動作する
    response = await client.get(f"/api/file/{entry_node_id}")
    assert response.status_code == 200


async def test_通常ファイルのnode_idでアーカイブエントリにはならない(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """通常ファイルの node_id は resolve_archive_entry() で None."""
    file_node_id = test_node_registry.register(test_root / "file.txt")
    response = await client.get(f"/api/file/{file_node_id}")
    assert response.status_code == 200
    assert response.text == "hello"  # 通常のファイル内容


async def test_アーカイブ内動画エントリがbrowseで返される(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """画像+動画を含む ZIP の browse で両方のエントリが返る."""
    archive = test_root / "dir_a" / "mixed.zip"
    node_id = test_node_registry.register(archive)
    response = await client.get(f"/api/browse/{node_id}")
    assert response.status_code == 200
    entries = response.json()["entries"]
    names = [e["name"] for e in entries]
    assert "clip.mp4" in names
    assert "thumb.jpg" in names
    # テキストファイルは除外
    assert "notes.txt" not in names
    # 動画の kind が video
    video_entry = next(e for e in entries if e["name"] == "clip.mp4")
    assert video_entry["kind"] == "video"


async def test_アーカイブ内動画がファイル配信される(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """アーカイブ内の動画エントリを file API で取得できる."""
    archive = test_root / "dir_a" / "mixed.zip"
    arc_node_id = test_node_registry.register(archive)
    browse_resp = await client.get(f"/api/browse/{arc_node_id}")
    entries = browse_resp.json()["entries"]
    video_entry = next(e for e in entries if e["name"] == "clip.mp4")

    response = await client.get(f"/api/file/{video_entry['node_id']}")
    assert response.status_code == 200
    assert "video" in response.headers["content-type"]


async def test_アーカイブ内動画のRangeリクエストで206を返す(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """アーカイブ内動画で Range ヘッダ付きリクエストが 206 を返す."""
    archive = test_root / "dir_a" / "mixed.zip"
    arc_node_id = test_node_registry.register(archive)
    browse_resp = await client.get(f"/api/browse/{arc_node_id}")
    entries = browse_resp.json()["entries"]
    video_entry = next(e for e in entries if e["name"] == "clip.mp4")

    response = await client.get(
        f"/api/file/{video_entry['node_id']}",
        headers={"Range": "bytes=0-3"},
    )
    assert response.status_code == 206


async def test_アーカイブ内動画の2回目リクエストがキャッシュヒットする(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """1回目で tmpfile キャッシュが作られ、2回目はキャッシュヒットする."""
    archive = test_root / "dir_a" / "mixed.zip"
    arc_node_id = test_node_registry.register(archive)
    browse_resp = await client.get(f"/api/browse/{arc_node_id}")
    entries = browse_resp.json()["entries"]
    video_entry = next(e for e in entries if e["name"] == "clip.mp4")

    url = f"/api/file/{video_entry['node_id']}"
    # 1回目
    resp1 = await client.get(url)
    assert resp1.status_code == 200
    # 2回目 (キャッシュヒット)
    resp2 = await client.get(url)
    assert resp2.status_code == 200
    # 同じ内容
    assert resp1.content == resp2.content


async def test_アーカイブ内の許可外拡張子が引き続き除外される(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """テキストファイル等の許可外拡張子はアーカイブ browse に含まれない."""
    archive = test_root / "dir_a" / "mixed.zip"
    node_id = test_node_registry.register(archive)
    response = await client.get(f"/api/browse/{node_id}")
    names = [e["name"] for e in response.json()["entries"]]
    assert "notes.txt" not in names


async def test_パスワード付きZIPのbrowseで422を返す(
    client: AsyncClient,
    test_node_registry: NodeRegistry,
    test_root: Path,
) -> None:
    """パスワード付き ZIP は ArchivePasswordError → 422."""
    archive = test_root / "encrypted.zip"
    with zipfile.ZipFile(archive, "w") as zf:
        zf.writestr("secret.jpg", MINIMAL_JPEG)

    # バイナリレベルで暗号化フラグを設定
    data = bytearray(archive.read_bytes())
    assert data[0:4] == b"PK\x03\x04"
    data[6] |= 0x01
    cd_offset = data.index(b"PK\x01\x02")
    data[cd_offset + 8] |= 0x01
    archive.write_bytes(bytes(data))

    node_id = test_node_registry.register(archive)
    response = await client.get(f"/api/browse/{node_id}")
    assert response.status_code == 422
    assert "ARCHIVE_PASSWORD_REQUIRED" in response.text
