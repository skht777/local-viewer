"""DirIndex (ディレクトリリスティング専用インデックス) のテスト."""

import os
from pathlib import Path

import pytest

from backend.services.dir_index import DirIndex


@pytest.fixture
def dir_index(tmp_path: Path) -> DirIndex:
    """テスト用 DirIndex."""
    db_path = str(tmp_path / "test-dir-index.db")
    idx = DirIndex(db_path)
    idx.init_db()
    return idx


def test_DirIndexにエントリをingestするとSQLiteに保存される(
    dir_index: DirIndex,
) -> None:
    dir_index.add_entries(
        "mount1/dir_a",
        [
            ("image.jpg", "image", 1024, 1700000000_000000000),
            ("video.mp4", "video", 2048, 1700000001_000000000),
        ],
    )

    entries = dir_index.query_all("mount1/dir_a")
    assert len(entries) == 2
    names = {e["name"] for e in entries}
    assert names == {"image.jpg", "video.mp4"}


def test_DirIndexのquery_pageでdate_descソートが正しく返る(
    dir_index: DirIndex,
) -> None:
    dir_index.add_entries(
        "mount1/dir_a",
        [
            ("old.jpg", "image", 100, 1700000000_000000000),
            ("new.jpg", "image", 100, 1700000003_000000000),
            ("mid.jpg", "image", 100, 1700000001_000000000),
        ],
    )

    page = dir_index.query_page("mount1/dir_a", sort="date-desc", limit=2)
    assert len(page) == 2
    assert page[0]["name"] == "new.jpg"
    assert page[1]["name"] == "mid.jpg"


def test_DirIndexのquery_pageでlimitとcursorが動作する(
    dir_index: DirIndex,
) -> None:
    dir_index.add_entries(
        "mount1/dir_a",
        [
            (f"img_{i:03d}.jpg", "image", 100, 1700000000_000000000 + i)
            for i in range(10)
        ],
    )

    # 1ページ目
    page1 = dir_index.query_page(
        "mount1/dir_a", sort="name-asc", limit=3
    )
    assert len(page1) == 3
    assert page1[0]["name"] == "img_000.jpg"

    # 2ページ目 (cursor = 最後のエントリの sort_key)
    cursor = page1[-1]["sort_key"]
    page2 = dir_index.query_page(
        "mount1/dir_a", sort="name-asc", limit=3, cursor_sort_key=cursor
    )
    assert len(page2) == 3
    assert page2[0]["name"] == "img_003.jpg"


def test_DirIndexのquery_pageでname_ascが自然順で返る(
    dir_index: DirIndex,
) -> None:
    """file2 < file10 の自然順を sort_key で再現できる."""
    dir_index.add_entries(
        "mount1/dir_a",
        [
            ("file10.jpg", "image", 100, 1700000000_000000000),
            ("file2.jpg", "image", 100, 1700000001_000000000),
            ("file1.jpg", "image", 100, 1700000002_000000000),
        ],
    )

    page = dir_index.query_page("mount1/dir_a", sort="name-asc", limit=10)
    names = [e["name"] for e in page]
    assert names == ["file1.jpg", "file2.jpg", "file10.jpg"]


def test_DirIndexのsort_keyで数値ゼロ埋めが正しい(
    dir_index: DirIndex,
) -> None:
    from backend.services.dir_index import encode_sort_key

    key = encode_sort_key("file2.jpg")
    assert "0000000002" in key

    key10 = encode_sort_key("file10.jpg")
    # file2 < file10 in natural order
    assert key < key10


def test_child_countクエリが正しく返る(
    dir_index: DirIndex,
) -> None:
    dir_index.add_entries(
        "mount1/dir_a",
        [
            ("img1.jpg", "image", 100, 1700000000_000000000),
            ("img2.jpg", "image", 200, 1700000001_000000000),
            ("sub", "directory", None, 1700000002_000000000),
        ],
    )

    count = dir_index.child_count("mount1/dir_a")
    assert count == 3


def test_preview_entriesクエリが先頭3件を返す(
    dir_index: DirIndex,
) -> None:
    dir_index.add_entries(
        "mount1/dir_a",
        [
            ("a_img.jpg", "image", 100, 1700000000_000000000),
            ("b_img.jpg", "image", 100, 1700000001_000000000),
            ("c_img.jpg", "image", 100, 1700000002_000000000),
            ("d_img.jpg", "image", 100, 1700000003_000000000),
            ("doc.pdf", "pdf", 500, 1700000004_000000000),
        ],
    )

    previews = dir_index.preview_entries("mount1/dir_a", limit=3)
    assert len(previews) == 3
    assert previews[0]["name"] == "a_img.jpg"


def test_preview_entriesクエリがアーカイブとPDFを含む(
    dir_index: DirIndex,
) -> None:
    dir_index.add_entries(
        "mount1/mixed",
        [
            ("a_img.jpg", "image", 100, 1700000000_000000000),
            ("b_comic.zip", "archive", 5000, 1700000001_000000000),
            ("c_doc.pdf", "pdf", 500, 1700000002_000000000),
            ("d_subdir", "directory", 0, 1700000003_000000000),
            ("e_clip.mp4", "video", 10000, 1700000004_000000000),
        ],
    )

    previews = dir_index.preview_entries("mount1/mixed", limit=5)
    kinds = {p["kind"] for p in previews}
    # image, archive, pdf, video が含まれる
    assert "image" in kinds
    assert "archive" in kinds
    assert "pdf" in kinds
    assert "video" in kinds
    # directory は含まれない
    assert "directory" not in kinds


def test_ディレクトリ優先のnameソート(
    dir_index: DirIndex,
) -> None:
    """name-asc でディレクトリがファイルより前に来る."""
    dir_index.add_entries(
        "mount1/dir_a",
        [
            ("alpha.jpg", "image", 100, 1700000000_000000000),
            ("subdir", "directory", None, 1700000001_000000000),
            ("zulu.jpg", "image", 100, 1700000002_000000000),
        ],
    )

    page = dir_index.query_page("mount1/dir_a", sort="name-asc", limit=10)
    kinds = [e["kind"] for e in page]
    # ディレクトリが最初
    assert kinds[0] == "directory"
    assert kinds[1] == "image"
