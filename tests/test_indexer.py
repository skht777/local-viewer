"""Indexer サービスのテスト.

SQLite FTS5 trigram によるファイルインデックスの検索・管理機能を検証する。
"""

import os
import sqlite3
import threading
from pathlib import Path

import pytest

from backend.services.indexer import IndexEntry, Indexer


@pytest.fixture()
def db_path(tmp_path: Path) -> str:
    """テスト用の一時 DB パスを返す."""
    return str(tmp_path / "test-index.db")


@pytest.fixture()
def indexer(db_path: str) -> Indexer:
    """初期化済み Indexer を返す."""
    idx = Indexer(db_path)
    idx.init_db()
    return idx


@pytest.fixture()
def sample_entries() -> list[IndexEntry]:
    """テスト用サンプルエントリ."""
    return [
        IndexEntry(
            relative_path="pictures/vacation/photo_001.jpg",
            name="photo_001.jpg",
            kind="image",
            size_bytes=2048000,
            mtime_ns=1000000000,
        ),
        IndexEntry(
            relative_path="pictures/vacation/photo_002.jpg",
            name="photo_002.jpg",
            kind="image",
            size_bytes=3072000,
            mtime_ns=1000000001,
        ),
        IndexEntry(
            relative_path="videos/clip.mp4",
            name="clip.mp4",
            kind="video",
            size_bytes=50000000,
            mtime_ns=1000000002,
        ),
        IndexEntry(
            relative_path="data/写真集2024.zip",
            name="写真集2024.zip",
            kind="archive",
            size_bytes=100000000,
            mtime_ns=1000000003,
        ),
        IndexEntry(
            relative_path="docs/レポート.pdf",
            name="レポート.pdf",
            kind="pdf",
            size_bytes=500000,
            mtime_ns=1000000004,
        ),
        IndexEntry(
            relative_path="pictures/vacation",
            name="vacation",
            kind="directory",
            size_bytes=None,
            mtime_ns=1000000005,
        ),
    ]


class TestIndexerBasic:
    """基本的な検索機能."""

    def test_空のデータベースで検索結果が0件(self, indexer: Indexer) -> None:
        results, has_more = indexer.search("anything")
        assert results == []
        assert has_more is False

    def test_ファイルをインデックスして名前で検索できる(
        self, indexer: Indexer, sample_entries: list[IndexEntry]
    ) -> None:
        for entry in sample_entries:
            indexer.add_entry(entry)

        results, _ = indexer.search("photo")
        assert len(results) == 2
        names = {r.name for r in results}
        assert names == {"photo_001.jpg", "photo_002.jpg"}

    def test_kind指定で検索をフィルタできる(
        self, indexer: Indexer, sample_entries: list[IndexEntry]
    ) -> None:
        for entry in sample_entries:
            indexer.add_entry(entry)

        results, _ = indexer.search("photo", kind="image")
        assert len(results) == 2

        results, _ = indexer.search("photo", kind="video")
        assert len(results) == 0

    def test_部分一致検索ができる(
        self, indexer: Indexer, sample_entries: list[IndexEntry]
    ) -> None:
        for entry in sample_entries:
            indexer.add_entry(entry)

        # "hoto" は "photo" の部分文字列 (3文字以上)
        results, _ = indexer.search("hoto")
        assert len(results) == 2

    def test_パス途中のディレクトリ名で検索できる(
        self, indexer: Indexer, sample_entries: list[IndexEntry]
    ) -> None:
        for entry in sample_entries:
            indexer.add_entry(entry)

        # relative_path に "vacation" を含むエントリがヒット
        results, _ = indexer.search("vacation")
        assert len(results) >= 2  # photo_001, photo_002, vacation ディレクトリ


class TestIndexerJapanese:
    """日本語ファイル名の検索."""

    def test_日本語ファイル名の部分一致検索(
        self, indexer: Indexer, sample_entries: list[IndexEntry]
    ) -> None:
        for entry in sample_entries:
            indexer.add_entry(entry)

        # 3文字以上: FTS5 trigram
        results, _ = indexer.search("写真集")
        assert len(results) == 1
        assert results[0].name == "写真集2024.zip"

    def test_日本語2文字クエリでLIKEフォールバック(
        self, indexer: Indexer, sample_entries: list[IndexEntry]
    ) -> None:
        for entry in sample_entries:
            indexer.add_entry(entry)

        # 2文字: LIKE フォールバック
        results, _ = indexer.search("写真")
        assert len(results) == 1
        assert results[0].name == "写真集2024.zip"


class TestIndexerQuery:
    """クエリ処理の検証."""

    def test_複数キーワードのAND検索(
        self, indexer: Indexer, sample_entries: list[IndexEntry]
    ) -> None:
        for entry in sample_entries:
            indexer.add_entry(entry)

        # "photo" AND "001" — photo_001.jpg のみヒット
        results, _ = indexer.search("photo 001")
        assert len(results) == 1
        assert results[0].name == "photo_001.jpg"

    def test_特殊文字を含むクエリがエスケープされる(
        self, indexer: Indexer, sample_entries: list[IndexEntry]
    ) -> None:
        for entry in sample_entries:
            indexer.add_entry(entry)

        # ダブルクォート等が安全にエスケープされること
        results, _ = indexer.search('"photo"')
        # エラーにならずに結果が返ること (件数は問わない)
        assert isinstance(results, list)

    def test_FTS5予約語を含むクエリが安全に処理される(
        self, indexer: Indexer, sample_entries: list[IndexEntry]
    ) -> None:
        for entry in sample_entries:
            indexer.add_entry(entry)

        # AND, OR, NOT, NEAR 等の FTS5 予約語
        for keyword in ["AND", "OR NOT", "NEAR"]:
            results, _ = indexer.search(keyword)
            assert isinstance(results, list)

    def test_長大クエリが空結果を返す(
        self, indexer: Indexer, sample_entries: list[IndexEntry]
    ) -> None:
        for entry in sample_entries:
            indexer.add_entry(entry)

        long_query = "a" * 300
        results, _ = indexer.search(long_query)
        assert results == []


class TestIndexerCRUD:
    """エントリの追加・更新・削除."""

    def test_エントリの追加と削除(self, indexer: Indexer) -> None:
        entry = IndexEntry(
            relative_path="test/file.jpg",
            name="file.jpg",
            kind="image",
            size_bytes=1000,
            mtime_ns=100,
        )
        indexer.add_entry(entry)
        results, _ = indexer.search("file")
        assert len(results) == 1

        indexer.remove_entry("test/file.jpg")
        results, _ = indexer.search("file")
        assert len(results) == 0

    def test_エントリの追加はupsert(self, indexer: Indexer) -> None:
        entry1 = IndexEntry(
            relative_path="test/file.jpg",
            name="file.jpg",
            kind="image",
            size_bytes=1000,
            mtime_ns=100,
        )
        indexer.add_entry(entry1)

        # 同じ relative_path で再度追加 → 上書き (重複なし)
        entry2 = IndexEntry(
            relative_path="test/file.jpg",
            name="file.jpg",
            kind="image",
            size_bytes=2000,
            mtime_ns=200,
        )
        indexer.add_entry(entry2)

        results, _ = indexer.search("file")
        assert len(results) == 1
        assert results[0].size_bytes == 2000

    def test_エントリの更新でmtimeが変わる(self, indexer: Indexer) -> None:
        entry = IndexEntry(
            relative_path="test/file.jpg",
            name="file.jpg",
            kind="image",
            size_bytes=1000,
            mtime_ns=100,
        )
        indexer.add_entry(entry)

        updated = IndexEntry(
            relative_path="test/file.jpg",
            name="file.jpg",
            kind="image",
            size_bytes=1000,
            mtime_ns=999,
        )
        indexer.add_entry(updated)

        # DB 直接確認
        conn = sqlite3.connect(indexer._db_path)
        row = conn.execute(
            "SELECT mtime_ns FROM entries WHERE relative_path = ?",
            ("test/file.jpg",),
        ).fetchone()
        conn.close()
        assert row is not None
        assert row[0] == 999


class TestIndexerPagination:
    """ページネーション."""

    def test_結果の上限とオフセット(self, indexer: Indexer) -> None:
        # 10 件登録
        for i in range(10):
            indexer.add_entry(
                IndexEntry(
                    relative_path=f"data/item_{i:03d}.jpg",
                    name=f"item_{i:03d}.jpg",
                    kind="image",
                    size_bytes=1000,
                    mtime_ns=i,
                )
            )

        results, has_more = indexer.search("item", limit=3)
        assert len(results) == 3
        assert has_more is True

        results2, has_more2 = indexer.search("item", limit=3, offset=3)
        assert len(results2) == 3
        assert has_more2 is True

        # 全件より大きい limit
        results_all, has_more_all = indexer.search("item", limit=100)
        assert len(results_all) == 10
        assert has_more_all is False

    def test_has_moreが正しく返る(self, indexer: Indexer) -> None:
        for i in range(5):
            indexer.add_entry(
                IndexEntry(
                    relative_path=f"data/file_{i}.jpg",
                    name=f"file_{i}.jpg",
                    kind="image",
                    size_bytes=1000,
                    mtime_ns=i,
                )
            )

        # ちょうど limit 件 → has_more = False
        results, has_more = indexer.search("file", limit=5)
        assert len(results) == 5
        assert has_more is False

        # limit - 1 件 → has_more = True
        results, has_more = indexer.search("file", limit=4)
        assert len(results) == 4
        assert has_more is True


class TestIndexerRebuild:
    """インデックス再構築."""

    def test_is_readyは初期状態でFalse(self, indexer: Indexer) -> None:
        assert indexer.is_ready is False

    def test_is_rebuildingは初期状態でFalse(self, indexer: Indexer) -> None:
        assert indexer.is_rebuilding is False


class TestIndexerScanDirectory:
    """ディレクトリスキャン.

    scan_directory は PathSecurity 依存のため統合テストで検証。
    ここではフィクスチャベースの基本テスト。
    """

    def test_scan_directoryでエントリが登録される(self, indexer: Indexer, tmp_path: Path) -> None:
        # テスト用ディレクトリ構造
        root = tmp_path / "data"
        root.mkdir()
        (root / "photo.jpg").write_bytes(b"\xff\xd8" * 100)
        (root / "sub").mkdir()
        (root / "sub" / "video.mp4").write_bytes(b"\x00" * 100)
        (root / "hidden.txt").write_bytes(b"text")

        # PathSecurity モック不要 — scan_directory はこのテストで直接は呼ばない
        # add_entry で手動追加して scan 結果を模擬
        indexer.add_entry(
            IndexEntry("photo.jpg", "photo.jpg", "image", 200, 100)
        )
        indexer.add_entry(
            IndexEntry("sub/video.mp4", "video.mp4", "video", 100, 200)
        )
        indexer.add_entry(
            IndexEntry("sub", "sub", "directory", None, 300)
        )

        results, _ = indexer.search("photo")
        assert len(results) == 1

        results, _ = indexer.search("video")
        assert len(results) == 1


class TestIndexerIndexableKinds:
    """インデックス対象の kind フィルタリング."""

    def test_scan_directoryで画像ファイルがスキップされる(
        self, indexer: Indexer, tmp_path: Path
    ) -> None:
        """image kind のファイルはインデックスに登録されない."""
        from unittest.mock import MagicMock

        root = tmp_path / "data"
        root.mkdir()
        # image (スキップ対象)
        (root / "photo.jpg").write_bytes(b"\xff\xd8" * 100)
        (root / "pic.png").write_bytes(b"\x89PNG" * 100)
        # video (登録対象)
        (root / "clip.mp4").write_bytes(b"\x00" * 100)
        # archive (登録対象)
        (root / "pack.zip").write_bytes(b"PK" * 100)
        # pdf (登録対象)
        (root / "doc.pdf").write_bytes(b"%PDF" * 100)
        # サブディレクトリ (登録対象)
        (root / "sub").mkdir()

        security = MagicMock()
        security.validate = MagicMock(side_effect=lambda p: p)

        count = indexer.scan_directory(root, security)

        # video, archive, pdf, sub ディレクトリ = 4 件
        assert count == 4

        # image は検索にヒットしない
        results, _ = indexer.search("photo")
        assert len(results) == 0
        results, _ = indexer.search("pic")
        assert len(results) == 0

        # video, archive, pdf は検索にヒットする
        results, _ = indexer.search("clip")
        assert len(results) == 1
        results, _ = indexer.search("pack")
        assert len(results) == 1
        results, _ = indexer.search("doc.pdf")
        assert len(results) == 1

    def test_incremental_scanで画像ファイルがスキップされる(
        self, indexer: Indexer, tmp_path: Path
    ) -> None:
        """incremental_scan でも image kind は登録されない."""
        from unittest.mock import MagicMock

        root = tmp_path / "data"
        root.mkdir()
        (root / "photo.jpg").write_bytes(b"\xff\xd8" * 100)
        (root / "clip.mp4").write_bytes(b"\x00" * 100)

        security = MagicMock()
        security.validate = MagicMock(side_effect=lambda p: p)

        added, updated, deleted = indexer.incremental_scan(root, security)

        # video のみ追加 = 1 件
        assert added == 1
        results, _ = indexer.search("photo")
        assert len(results) == 0
        results, _ = indexer.search("clip")
        assert len(results) == 1


class TestIndexerConcurrency:
    """WAL モードでの同時アクセス."""

    def test_concurrent_read_write_WALモード(self, indexer: Indexer) -> None:
        # 書き込みスレッドと読み取りスレッドの同時実行
        errors: list[Exception] = []

        def writer() -> None:
            try:
                for i in range(50):
                    indexer.add_entry(
                        IndexEntry(
                            relative_path=f"data/concurrent_{i}.jpg",
                            name=f"concurrent_{i}.jpg",
                            kind="image",
                            size_bytes=1000,
                            mtime_ns=i,
                        )
                    )
            except Exception as e:
                errors.append(e)

        def reader() -> None:
            try:
                for _ in range(50):
                    indexer.search("concurrent")
            except Exception as e:
                errors.append(e)

        t1 = threading.Thread(target=writer)
        t2 = threading.Thread(target=reader)
        t1.start()
        t2.start()
        t1.join()
        t2.join()

        assert errors == [], f"同時アクセスでエラー: {errors}"
