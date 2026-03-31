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
    os.environ["MOUNT_BASE_DIR"] = str(root_dir)
    os.environ.pop("ALLOW_SYMLINKS", None)
    settings = Settings()
    security = PathSecurity(settings)
    reg = NodeRegistry(security)
    yield reg
    os.environ.pop("MOUNT_BASE_DIR", None)


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


# --- アーカイブエントリ ---


def test_アーカイブエントリを登録してnode_idを返す(
    registry: NodeRegistry, root_dir: Path
) -> None:
    archive = root_dir / "test.zip"
    archive.touch()
    node_id = registry.register_archive_entry(archive, "image01.jpg")
    assert isinstance(node_id, str)
    assert len(node_id) == 16


def test_同じアーカイブエントリに対して同じnode_idを返す(
    registry: NodeRegistry, root_dir: Path
) -> None:
    archive = root_dir / "test.zip"
    archive.touch()
    id1 = registry.register_archive_entry(archive, "image01.jpg")
    id2 = registry.register_archive_entry(archive, "image01.jpg")
    assert id1 == id2


def test_アーカイブエントリのnode_idを解決できる(
    registry: NodeRegistry, root_dir: Path
) -> None:
    archive = root_dir / "test.zip"
    archive.touch()
    node_id = registry.register_archive_entry(archive, "page01.jpg")
    result = registry.resolve_archive_entry(node_id)
    assert result is not None
    assert result[0] == archive.resolve()
    assert result[1] == "page01.jpg"


def test_is_archive_entryが正しく判定する(
    registry: NodeRegistry, root_dir: Path
) -> None:
    archive = root_dir / "test.zip"
    archive.touch()
    arc_id = registry.register_archive_entry(archive, "img.jpg")
    file_id = registry.register(root_dir / "dir_a" / "image.jpg")

    assert registry.is_archive_entry(arc_id) is True
    assert registry.is_archive_entry(file_id) is False


def test_list_archive_entriesでEntryMetaリストを返す(
    registry: NodeRegistry, root_dir: Path
) -> None:
    from backend.services.archive_reader import ArchiveEntry

    archive = root_dir / "test.zip"
    archive.touch()
    entries = [
        ArchiveEntry(
            name="img01.jpg",
            size_compressed=100,
            size_uncompressed=200,
            is_dir=False,
        ),
        ArchiveEntry(
            name="img02.png",
            size_compressed=150,
            size_uncompressed=300,
            is_dir=False,
        ),
    ]
    result = registry.list_archive_entries(archive, entries)
    assert len(result) == 2
    assert result[0].name == "img01.jpg"
    assert result[0].kind == "image"
    assert result[0].node_id
    assert result[1].name == "img02.png"
    assert result[1].kind == "image"


# --- 複数ルート対応テスト ---


@pytest.fixture
def multi_roots(tmp_path: Path) -> tuple[Path, Path]:
    """複数ルート用テストディレクトリ."""
    root_a = tmp_path / "root_a"
    root_b = tmp_path / "root_b"
    root_a.mkdir()
    root_b.mkdir()
    (root_a / "file_a.txt").write_text("a")
    (root_b / "file_b.txt").write_text("b")
    # 両ルートに同名ファイルを配置
    (root_a / "common.txt").write_text("from_a")
    (root_b / "common.txt").write_text("from_b")
    return root_a, root_b


@pytest.fixture
def multi_registry(multi_roots: tuple[Path, Path]) -> NodeRegistry:
    """複数ルートの NodeRegistry."""
    security = PathSecurity(list(multi_roots))
    return NodeRegistry(security)


def test_複数ルートでNodeRegistryを初期化できる(
    multi_registry: NodeRegistry,
) -> None:
    assert multi_registry.path_security is not None
    assert len(multi_registry.path_security.root_dirs) == 2


def test_複数ルートでルートAのファイルを登録できる(
    multi_registry: NodeRegistry, multi_roots: tuple[Path, Path]
) -> None:
    root_a, _ = multi_roots
    node_id = multi_registry.register(root_a / "file_a.txt")
    assert isinstance(node_id, str)
    assert len(node_id) == 16
    resolved = multi_registry.resolve(node_id)
    assert resolved == (root_a / "file_a.txt").resolve()


def test_複数ルートでルートBのファイルを登録できる(
    multi_registry: NodeRegistry, multi_roots: tuple[Path, Path]
) -> None:
    _, root_b = multi_roots
    node_id = multi_registry.register(root_b / "file_b.txt")
    resolved = multi_registry.resolve(node_id)
    assert resolved == (root_b / "file_b.txt").resolve()


def test_異なるルートの同名ファイルに異なるnode_idを返す(
    multi_registry: NodeRegistry, multi_roots: tuple[Path, Path]
) -> None:
    root_a, root_b = multi_roots
    id_a = multi_registry.register(root_a / "common.txt")
    id_b = multi_registry.register(root_b / "common.txt")
    assert id_a != id_b


def test_list_mount_rootsでマウントポイント一覧を返す(
    multi_registry: NodeRegistry, multi_roots: tuple[Path, Path]
) -> None:
    root_a, root_b = multi_roots
    mount_names = {root_a.resolve(): "Root A", root_b.resolve(): "Root B"}
    entries = multi_registry.list_mount_roots(mount_names)
    assert len(entries) == 2
    names = {e.name for e in entries}
    assert names == {"Root A", "Root B"}
    # 全エントリに node_id が付与されている
    for e in entries:
        assert len(e.node_id) == 16
        assert e.kind == "directory"


def test_list_mount_rootsでマウント名がない場合はディレクトリ名を使う(
    multi_registry: NodeRegistry, multi_roots: tuple[Path, Path]
) -> None:
    entries = multi_registry.list_mount_roots({})
    names = {e.name for e in entries}
    assert "root_a" in names
    assert "root_b" in names


# --- ancestors テスト ---


def test_ルートディレクトリのancestorsが空リストを返す(
    registry: NodeRegistry, root_dir: Path
) -> None:
    ancestors = registry.get_ancestors(root_dir)
    assert ancestors == []


def test_ルート直下ディレクトリのancestorsがルートのみを含む(
    registry: NodeRegistry, root_dir: Path
) -> None:
    ancestors = registry.get_ancestors(root_dir / "dir_a")
    assert len(ancestors) == 1
    # ルートの node_id が含まれる
    root_id = registry.register(root_dir)
    assert ancestors[0].node_id == root_id


def test_深い階層のancestorsが全祖先を含む(
    registry: NodeRegistry, root_dir: Path
) -> None:
    # root_dir / dir_a / sub の ancestors は [root, dir_a]
    ancestors = registry.get_ancestors(root_dir / "dir_a" / "sub")
    assert len(ancestors) == 2
    root_id = registry.register(root_dir)
    dir_a_id = registry.register(root_dir / "dir_a")
    assert ancestors[0].node_id == root_id
    assert ancestors[1].node_id == dir_a_id


def test_ancestorsのルートエントリ名がmount_namesを反映する(
    root_dir: Path,
) -> None:
    os.environ["MOUNT_BASE_DIR"] = str(root_dir)
    settings = Settings()
    security = PathSecurity(settings)
    mount_names = {root_dir.resolve(): "テスト用マウント"}
    reg = NodeRegistry(security, mount_names=mount_names)

    ancestors = reg.get_ancestors(root_dir / "dir_a")
    assert ancestors[0].name == "テスト用マウント"
    os.environ.pop("MOUNT_BASE_DIR", None)


def test_ancestorsの順序がルートから親へ正しい(
    registry: NodeRegistry, root_dir: Path
) -> None:
    ancestors = registry.get_ancestors(root_dir / "dir_a" / "sub")
    # [root, dir_a] の順
    assert ancestors[0].name == root_dir.name
    assert ancestors[1].name == "dir_a"


def test_list_archive_entriesで画像のみがkind_imageになる(
    registry: NodeRegistry, root_dir: Path
) -> None:
    from backend.services.archive_reader import ArchiveEntry

    archive = root_dir / "test.zip"
    archive.touch()
    entries = [
        ArchiveEntry(
            name="video.mp4",
            size_compressed=100,
            size_uncompressed=200,
            is_dir=False,
        ),
    ]
    result = registry.list_archive_entries(archive, entries)
    assert len(result) == 1
    assert result[0].kind == "video"
