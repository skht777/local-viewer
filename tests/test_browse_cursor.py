"""browse_cursor モジュールの単体テスト.

- encode_cursor / decode_cursor: HMAC 署名の生成・検証・改ざん検出
- sort_entries: 4 種類のソート順、ディレクトリ優先、null 末尾
- apply_cursor: カーソル位置復元、存在しないカーソル
- paginate: ソート + カーソル + limit の統合
"""

from __future__ import annotations

import base64
import json

import pytest

from py_backend.services.browse_cursor import (
    SortOrder,
    apply_cursor,
    decode_cursor,
    encode_cursor,
    paginate,
    sort_entries,
)
from py_backend.services.node_registry import EntryMeta


# ── ヘルパー ──


def _entry(
    name: str,
    kind: str = "image",
    node_id: str | None = None,
    modified_at: float | None = None,
    size_bytes: int | None = None,
) -> EntryMeta:
    return EntryMeta(
        node_id=node_id or f"id-{name}",
        name=name,
        kind=kind,
        modified_at=modified_at,
        size_bytes=size_bytes,
    )


# ── encode_cursor / decode_cursor ──


class TestEncodeDecode:
    """カーソルの encode/decode ラウンドトリップと署名検証."""

    def test_ラウンドトリップで元のデータを復元できる(self) -> None:
        entry = _entry("a.jpg", modified_at=100.0, size_bytes=1024)
        cursor = encode_cursor(SortOrder.NAME_ASC, entry, "etag-1")
        data = decode_cursor(cursor, SortOrder.NAME_ASC)
        assert data["id"] == "id-a.jpg"
        assert data["n"] == "a.jpg"
        assert data["m"] == 100.0
        assert data["sz"] == 1024

    def test_modified_atがNoneでもエンコードできる(self) -> None:
        entry = _entry("dir", kind="directory", modified_at=None)
        cursor = encode_cursor(SortOrder.DATE_DESC, entry, "")
        data = decode_cursor(cursor, SortOrder.DATE_DESC)
        assert data["m"] is None

    def test_改ざんされたカーソルでValueError(self) -> None:
        entry = _entry("a.jpg")
        cursor = encode_cursor(SortOrder.NAME_ASC, entry, "etag")
        # base64 デコード → JSON 改ざん → 再エンコード
        raw = json.loads(base64.urlsafe_b64decode(cursor).decode())
        raw["n"] = "tampered.jpg"
        tampered = base64.urlsafe_b64encode(
            json.dumps(raw, separators=(",", ":"), sort_keys=True).encode()
        ).decode()
        with pytest.raises(ValueError, match="署名が不正"):
            decode_cursor(tampered, SortOrder.NAME_ASC)

    def test_署名がないカーソルでValueError(self) -> None:
        payload = json.dumps({"s": "name-asc", "id": "x"}, separators=(",", ":"))
        cursor = base64.urlsafe_b64encode(payload.encode()).decode()
        with pytest.raises(ValueError, match="署名がありません"):
            decode_cursor(cursor, SortOrder.NAME_ASC)

    def test_不正なbase64でValueError(self) -> None:
        with pytest.raises(ValueError, match="不正なカーソルフォーマット"):
            decode_cursor("!!!invalid!!!", SortOrder.NAME_ASC)

    def test_ソート順が異なるカーソルでValueError(self) -> None:
        entry = _entry("a.jpg")
        cursor = encode_cursor(SortOrder.NAME_ASC, entry, "etag")
        with pytest.raises(ValueError, match="ソート順.*一致しません"):
            decode_cursor(cursor, SortOrder.DATE_DESC)


# ── sort_entries ──


class TestSortEntries:
    """sort_entries の 4 ソート順とエッジケース."""

    @pytest.fixture()
    def entries(self) -> list[EntryMeta]:
        return [
            _entry("file10.jpg", modified_at=300.0),
            _entry("subdir", kind="directory"),
            _entry("file2.jpg", modified_at=100.0),
            _entry("archive.zip", kind="archive", modified_at=200.0),
        ]

    def test_name_ascでディレクトリが先頭に来る(
        self, entries: list[EntryMeta]
    ) -> None:
        result = sort_entries(entries, SortOrder.NAME_ASC)
        assert result[0].kind == "directory"

    def test_name_ascで自然順ソートされる(self, entries: list[EntryMeta]) -> None:
        result = sort_entries(entries, SortOrder.NAME_ASC)
        # ディレクトリ以外: archive.zip, file2.jpg, file10.jpg (自然順)
        non_dirs = [e for e in result if e.kind != "directory"]
        assert [e.name for e in non_dirs] == [
            "archive.zip",
            "file2.jpg",
            "file10.jpg",
        ]

    def test_name_descでディレクトリが先頭かつ名前が降順(
        self, entries: list[EntryMeta]
    ) -> None:
        result = sort_entries(entries, SortOrder.NAME_DESC)
        assert result[0].kind == "directory"
        non_dirs = [e for e in result if e.kind != "directory"]
        assert [e.name for e in non_dirs] == [
            "file10.jpg",
            "file2.jpg",
            "archive.zip",
        ]

    def test_date_descで新しい順に並ぶ(self, entries: list[EntryMeta]) -> None:
        result = sort_entries(entries, SortOrder.DATE_DESC)
        dates = [e.modified_at for e in result if e.modified_at is not None]
        assert dates == sorted(dates, reverse=True)

    def test_date_ascで古い順に並ぶ(self, entries: list[EntryMeta]) -> None:
        result = sort_entries(entries, SortOrder.DATE_ASC)
        dates = [e.modified_at for e in result if e.modified_at is not None]
        assert dates == sorted(dates)

    def test_dateソートでmodified_atがNoneのエントリが末尾(self) -> None:
        entries = [
            _entry("a.jpg", modified_at=None),
            _entry("b.jpg", modified_at=100.0),
        ]
        result = sort_entries(entries, SortOrder.DATE_DESC)
        assert result[-1].modified_at is None

    def test_dateソートでディレクトリ優先なし(self) -> None:
        entries = [
            _entry("dir", kind="directory", modified_at=None),
            _entry("a.jpg", modified_at=100.0),
        ]
        # date ソートではディレクトリ優先なし → modified_at=None は末尾
        result = sort_entries(entries, SortOrder.DATE_DESC)
        assert result[0].name == "a.jpg"

    def test_空リストで空リストを返す(self) -> None:
        assert sort_entries([], SortOrder.NAME_ASC) == []


# ── apply_cursor ──


class TestApplyCursor:
    """apply_cursor のカーソル位置復元."""

    def test_カーソル位置の次から返す(self) -> None:
        entries = [_entry("a"), _entry("b"), _entry("c")]
        result = apply_cursor(entries, {"id": "id-b"})
        assert [e.name for e in result] == ["c"]

    def test_カーソルが先頭の場合は残り全件を返す(self) -> None:
        entries = [_entry("a"), _entry("b"), _entry("c")]
        result = apply_cursor(entries, {"id": "id-a"})
        assert [e.name for e in result] == ["b", "c"]

    def test_カーソルが末尾の場合は空リストを返す(self) -> None:
        entries = [_entry("a"), _entry("b")]
        result = apply_cursor(entries, {"id": "id-b"})
        assert result == []

    def test_存在しないカーソルIDでは先頭から全件を返す(self) -> None:
        entries = [_entry("a"), _entry("b")]
        result = apply_cursor(entries, {"id": "nonexistent"})
        assert [e.name for e in result] == ["a", "b"]


# ── paginate ──


class TestPaginate:
    """paginate の統合テスト."""

    @pytest.fixture()
    def entries(self) -> list[EntryMeta]:
        return [_entry(f"file{i}.jpg", modified_at=float(i)) for i in range(5)]

    def test_limitなしで全件返却しnext_cursorがNone(
        self, entries: list[EntryMeta]
    ) -> None:
        page, next_cursor, total = paginate(entries, SortOrder.NAME_ASC)
        assert len(page) == 5
        assert next_cursor is None
        assert total == 5

    def test_limitで件数が制限される(self, entries: list[EntryMeta]) -> None:
        page, next_cursor, total = paginate(
            entries, SortOrder.NAME_ASC, limit=2
        )
        assert len(page) == 2
        assert next_cursor is not None
        assert total == 5

    def test_next_cursorで次ページを取得できる(
        self, entries: list[EntryMeta]
    ) -> None:
        page1, cursor1, _ = paginate(entries, SortOrder.NAME_ASC, limit=2)
        page2, cursor2, _ = paginate(
            entries, SortOrder.NAME_ASC, limit=2, cursor=cursor1
        )
        # 重複なく連続している
        all_names = [e.name for e in page1] + [e.name for e in page2]
        assert len(all_names) == len(set(all_names))

    def test_最終ページでnext_cursorがNone(self, entries: list[EntryMeta]) -> None:
        _, cursor1, _ = paginate(entries, SortOrder.NAME_ASC, limit=3)
        page2, cursor2, _ = paginate(
            entries, SortOrder.NAME_ASC, limit=3, cursor=cursor1
        )
        assert cursor2 is None
        assert len(page2) == 2

    def test_全ページ走査で全エントリを網羅する(
        self, entries: list[EntryMeta]
    ) -> None:
        all_entries: list[EntryMeta] = []
        cursor = None
        while True:
            page, cursor, _ = paginate(
                entries, SortOrder.NAME_ASC, limit=2, cursor=cursor
            )
            all_entries.extend(page)
            if cursor is None:
                break
        assert len(all_entries) == 5

    def test_不正なカーソルでValueErrorが送出される(
        self, entries: list[EntryMeta]
    ) -> None:
        with pytest.raises(ValueError):
            paginate(entries, SortOrder.NAME_ASC, limit=2, cursor="invalid")
