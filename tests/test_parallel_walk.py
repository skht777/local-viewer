"""parallel_walk モジュールのテスト.

BFS レベル単位の並列ディレクトリ走査を検証する。
"""

import os
from pathlib import Path

import pytest

from backend.services.parallel_walk import WalkEntry, parallel_walk


@pytest.fixture()
def tree(tmp_path: Path) -> Path:
    """3 階層のテスト用ディレクトリツリーを作成する."""
    root = tmp_path / "root"
    root.mkdir()
    # level 1
    (root / "a").mkdir()
    (root / "b").mkdir()
    # level 2
    (root / "a" / "a1").mkdir()
    # files
    (root / "f1.txt").write_bytes(b"hello")
    (root / "a" / "f2.mp4").write_bytes(b"\x00" * 100)
    (root / "a" / "a1" / "f3.zip").write_bytes(b"PK" * 50)
    (root / "b" / "f4.pdf").write_bytes(b"%PDF" * 10)
    return root


class TestParallelWalk:
    """parallel_walk の基本動作."""

    def test_全ディレクトリを走査する(self, tree: Path) -> None:
        entries = list(parallel_walk(tree, workers=2))
        visited_dirs = {e.path for e in entries}
        assert tree in visited_dirs
        assert tree / "a" in visited_dirs
        assert tree / "b" in visited_dirs
        assert tree / "a" / "a1" in visited_dirs

    def test_全ファイルが検出される(self, tree: Path) -> None:
        all_files: list[str] = []
        for entry in parallel_walk(tree, workers=2):
            all_files.extend(name for name, _, _ in entry.files)
        assert sorted(all_files) == ["f1.txt", "f2.mp4", "f3.zip", "f4.pdf"]

    def test_WalkEntryにディレクトリ自体のmtime_nsが含まれる(self, tree: Path) -> None:
        entries = list(parallel_walk(tree, workers=2))
        for entry in entries:
            assert isinstance(entry.mtime_ns, int)
            assert entry.mtime_ns > 0

    def test_subdirsにmtime_nsが含まれる(self, tree: Path) -> None:
        root_entry = next(e for e in parallel_walk(tree) if e.path == tree)
        for name, mtime_ns in root_entry.subdirs:
            assert isinstance(mtime_ns, int)
            assert mtime_ns > 0

    def test_filesにsize_bytesとmtime_nsが含まれる(self, tree: Path) -> None:
        root_entry = next(e for e in parallel_walk(tree) if e.path == tree)
        for name, size, mtime_ns in root_entry.files:
            if name == "f1.txt":
                assert size == 5
                assert mtime_ns > 0

    def test_隠しディレクトリがスキップされる(self, tmp_path: Path) -> None:
        root = tmp_path / "root"
        root.mkdir()
        (root / ".hidden").mkdir()
        (root / ".hidden" / "secret.txt").write_bytes(b"x")
        (root / "visible").mkdir()
        (root / "visible" / "ok.txt").write_bytes(b"y")

        entries = list(parallel_walk(root))
        visited_dirs = {e.path for e in entries}
        assert root / ".hidden" not in visited_dirs
        assert root / "visible" in visited_dirs

    def test_隠しファイルがスキップされる(self, tmp_path: Path) -> None:
        root = tmp_path / "root"
        root.mkdir()
        (root / ".hidden_file").write_bytes(b"x")
        (root / "visible_file").write_bytes(b"y")

        entries = list(parallel_walk(root))
        root_entry = next(e for e in entries if e.path == root)
        file_names = [name for name, _, _ in root_entry.files]
        assert ".hidden_file" not in file_names
        assert "visible_file" in file_names

    def test_workers_1でも正しく動作する(self, tree: Path) -> None:
        entries = list(parallel_walk(tree, workers=1))
        assert len(entries) == 4  # root, a, b, a/a1

    def test_空ディレクトリが正しく処理される(self, tmp_path: Path) -> None:
        root = tmp_path / "root"
        root.mkdir()
        (root / "empty").mkdir()

        entries = list(parallel_walk(root))
        empty_entry = next(e for e in entries if e.path == root / "empty")
        assert empty_entry.files == []
        assert empty_entry.subdirs == []

    def test_path_validatorでプルーニングが動作する(
        self, tmp_path: Path
    ) -> None:
        root = tmp_path / "root"
        root.mkdir()
        (root / "allowed").mkdir()
        (root / "allowed" / "ok.txt").write_bytes(b"y")
        (root / "denied").mkdir()
        (root / "denied" / "no.txt").write_bytes(b"n")

        # denied ディレクトリを拒否するバリデータ
        def validator(path: Path) -> bool:
            return "denied" not in path.name

        entries = list(parallel_walk(root, path_validator=validator))
        visited_dirs = {e.path for e in entries}
        assert root / "allowed" in visited_dirs
        assert root / "denied" not in visited_dirs
