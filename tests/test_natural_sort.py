"""natural_sort_key の単体テスト."""

import pytest

from backend.services.natural_sort import natural_sort_key


def _sort(names: list[str]) -> list[str]:
    """natural_sort_key でソートした結果を返すヘルパー."""
    return sorted(names, key=natural_sort_key)


class TestNaturalSortKey:
    """Windows Explorer 互換の自然順ソートキー."""

    def test_基本的な数値順でソートされる(self) -> None:
        assert _sort(["file1", "file10", "file2"]) == ["file1", "file2", "file10"]

    def test_複数の数値区間を正しくソートする(self) -> None:
        assert _sort(["ch2p10", "ch2p2", "ch10p1"]) == ["ch2p2", "ch2p10", "ch10p1"]

    def test_大文字小文字を無視してソートする(self) -> None:
        assert _sort(["FileB", "filea"]) == ["filea", "FileB"]

    def test_数値なしは辞書順と同一になる(self) -> None:
        assert _sort(["banana", "apple", "cherry"]) == ["apple", "banana", "cherry"]

    def test_日本語と数値の混在を正しくソートする(self) -> None:
        assert _sort(["第1巻", "第10巻", "第2巻"]) == ["第1巻", "第2巻", "第10巻"]

    def test_数値のみのファイル名をソートする(self) -> None:
        assert _sort(["10", "1", "20", "2"]) == ["1", "2", "10", "20"]

    def test_拡張子付きファイル名を正しくソートする(self) -> None:
        assert _sort(["img10.jpg", "img1.jpg", "img2.jpg"]) == [
            "img1.jpg",
            "img2.jpg",
            "img10.jpg",
        ]
