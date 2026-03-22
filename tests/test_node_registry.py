"""node_id レジストリのテスト."""

import os
from collections.abc import Generator
from pathlib import Path

import pytest

from backend.config import Settings
from backend.errors import NodeNotFoundError
from backend.services.node_registry import EntryKind, NodeRegistry
from backend.services.path_security import PathSecurity


@pytest.fixture
def root_dir(tmp_path: Path) -> Path:
    """テスト用ディレクトリ構造.

    tmp_path/
    ├── dir_a/
    │   ├── image.jpg (最小JPEG)
    │   └── sub/
    │       └── deep.txt
    ├── dir_b/
    │   └── video.mp4 (ダミー)
    ├── file.txt
    └── photo.png (ダミー)
    """
    (tmp_path / "dir_a").mkdir()
    (tmp_path / "dir_a" / "sub").mkdir()
    (tmp_path / "dir_b").mkdir()

    (tmp_path / "file.txt").write_text("hello")
    (tmp_path / "dir_a" / "sub" / "deep.txt").write_text("deep")

    # 最小 JPEG
    minimal_jpeg = bytes(
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
    (tmp_path / "dir_a" / "image.jpg").write_bytes(minimal_jpeg)
    (tmp_path / "dir_b" / "video.mp4").write_bytes(b"\x00" * 1024)
    (tmp_path / "photo.png").write_bytes(b"\x89PNG\r\n\x1a\n" + b"\x00" * 100)

    return tmp_path


@pytest.fixture
def registry(root_dir: Path) -> Generator[NodeRegistry]:
    """テスト用 NodeRegistry."""
    os.environ["ROOT_DIR"] = str(root_dir)
    os.environ.pop("ALLOW_SYMLINKS", None)
    settings = Settings()
    security = PathSecurity(settings)
    reg = NodeRegistry(security)
    yield reg
    os.environ.pop("ROOT_DIR", None)


def test_パスを登録してnode_idを返す(registry: NodeRegistry, root_dir: Path) -> None:
    node_id = registry.register(root_dir / "file.txt")
    assert isinstance(node_id, str)
    assert len(node_id) == 16


def test_同じパスに対して同じnode_idを返す(
    registry: NodeRegistry, root_dir: Path
) -> None:
    id1 = registry.register(root_dir / "file.txt")
    id2 = registry.register(root_dir / "file.txt")
    assert id1 == id2


def test_異なるパスに対して異なるnode_idを返す(
    registry: NodeRegistry, root_dir: Path
) -> None:
    id1 = registry.register(root_dir / "file.txt")
    id2 = registry.register(root_dir / "photo.png")
    assert id1 != id2


def test_node_idから元のパスを解決する(registry: NodeRegistry, root_dir: Path) -> None:
    node_id = registry.register(root_dir / "file.txt")
    resolved = registry.resolve(node_id)
    assert resolved == (root_dir / "file.txt").resolve()


def test_未登録のnode_idでNodeNotFoundError(registry: NodeRegistry) -> None:
    with pytest.raises(NodeNotFoundError):
        registry.resolve("nonexistent12345")


def test_ディレクトリ一覧でエントリを返す(
    registry: NodeRegistry, root_dir: Path
) -> None:
    entries = registry.list_directory(root_dir)
    assert len(entries) > 0
    names = [e.name for e in entries]
    assert "dir_a" in names
    assert "file.txt" in names


def test_ディレクトリ一覧でエントリがソートされている(
    registry: NodeRegistry, root_dir: Path
) -> None:
    entries = registry.list_directory(root_dir)
    names = [e.name for e in entries]
    # ディレクトリ部分とファイル部分がそれぞれ名前順
    dir_names = [n for n in names if n.startswith("dir_")]
    assert dir_names == sorted(dir_names, key=str.lower)


def test_ディレクトリ一覧でディレクトリが先にソートされる(
    registry: NodeRegistry, root_dir: Path
) -> None:
    entries = registry.list_directory(root_dir)
    kinds = [e.kind for e in entries]
    # ディレクトリが全てファイルより前に来る
    dir_done = False
    for kind in kinds:
        if kind != EntryKind.DIRECTORY:
            dir_done = True
        if dir_done and kind == EntryKind.DIRECTORY:
            pytest.fail("ディレクトリがファイルの後に出現しています")


def test_ファイルのkindが正しく判定される(
    registry: NodeRegistry, root_dir: Path
) -> None:
    entries = registry.list_directory(root_dir / "dir_a")
    entry_map = {e.name: e for e in entries}
    assert entry_map["image.jpg"].kind == EntryKind.IMAGE

    entries_b = registry.list_directory(root_dir / "dir_b")
    entry_map_b = {e.name: e for e in entries_b}
    assert entry_map_b["video.mp4"].kind == EntryKind.VIDEO


def test_ファイルサイズが正しく取得される(
    registry: NodeRegistry, root_dir: Path
) -> None:
    entries = registry.list_directory(root_dir)
    entry_map = {e.name: e for e in entries}
    assert entry_map["file.txt"].size_bytes == len("hello")


def test_MIMEタイプが正しく取得される(registry: NodeRegistry, root_dir: Path) -> None:
    entries = registry.list_directory(root_dir / "dir_a")
    entry_map = {e.name: e for e in entries}
    assert entry_map["image.jpg"].mime_type == "image/jpeg"


def test_ディレクトリのchild_countが正しい(
    registry: NodeRegistry, root_dir: Path
) -> None:
    entries = registry.list_directory(root_dir)
    entry_map = {e.name: e for e in entries}
    # dir_a には image.jpg + sub の 2 エントリ
    assert entry_map["dir_a"].child_count == 2


def test_親のnode_idが取得できる(registry: NodeRegistry, root_dir: Path) -> None:
    # dir_a を登録して親 ID を取得
    registry.register(root_dir / "dir_a")
    parent_id = registry.get_parent_node_id(root_dir / "dir_a")
    # 親は root_dir なので None (ルート直下の場合)
    # root_dir の1階層下なので parent は root → None
    assert parent_id is None


def test_ROOT_DIRの親はNone(registry: NodeRegistry, root_dir: Path) -> None:
    parent_id = registry.get_parent_node_id(root_dir)
    assert parent_id is None
