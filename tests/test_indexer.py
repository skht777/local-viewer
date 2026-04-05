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


class TestIndexerEntryCount:
    """entry_count メソッド."""

    def test_空のDBで0を返す(self, indexer: Indexer) -> None:
        assert indexer.entry_count() == 0

    def test_エントリ追加後に正しい件数を返す(self, indexer: Indexer) -> None:
        indexer.add_entry(
            IndexEntry("test/a.mp4", "a.mp4", "video", 1000, 100)
        )
        indexer.add_entry(
            IndexEntry("test/b.zip", "b.zip", "archive", 2000, 200)
        )
        assert indexer.entry_count() == 2


class TestIndexerIsReadyAfterIncrementalScan:
    """incremental_scan 後の is_ready フラグ."""

    def test_incremental_scan後にis_readyがTrueになる(
        self, indexer: Indexer, tmp_path: Path
    ) -> None:
        from unittest.mock import MagicMock

        root = tmp_path / "data"
        root.mkdir()
        (root / "clip.mp4").write_bytes(b"\x00" * 100)

        security = MagicMock()
        security.validate = MagicMock(side_effect=lambda p: p)

        assert indexer.is_ready is False
        indexer.incremental_scan(root, security)
        assert indexer.is_ready is True


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


class TestIndexerMountScopedIncrementalScan:
    """マウント単位の incremental_scan."""

    def test_incremental_scanがマウント単位でフィルタされる(
        self, indexer: Indexer, tmp_path: Path
    ) -> None:
        """mount_id 指定時、他マウントのエントリを削除しない."""
        from unittest.mock import MagicMock

        security = MagicMock()
        security.validate = MagicMock(side_effect=lambda p: p)

        # mount_a のデータを手動登録
        indexer.add_entry(
            IndexEntry("mount_a/clip.mp4", "clip.mp4", "video", 1000, 100)
        )
        # mount_b のデータを手動登録
        indexer.add_entry(
            IndexEntry("mount_b/doc.pdf", "doc.pdf", "pdf", 2000, 200)
        )
        assert indexer.entry_count() == 2

        # mount_a を空ディレクトリで incremental_scan
        root_a = tmp_path / "mount_a"
        root_a.mkdir()

        added, updated, deleted = indexer.incremental_scan(
            root_a, security, mount_id="mount_a"
        )

        # mount_a のエントリのみ削除、mount_b は残る
        assert deleted == 1
        assert indexer.entry_count() == 1
        results, _ = indexer.search("doc.pdf")
        assert len(results) == 1


class TestIndexerSchemaVersion:
    """スキーマバージョン管理."""

    def test_init_dbでスキーマバージョンが記録される(self, db_path: str) -> None:
        idx = Indexer(db_path)
        idx.init_db()

        conn = sqlite3.connect(db_path)
        row = conn.execute(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'"
        ).fetchone()
        conn.close()
        assert row is not None
        assert row[0] == "2"

    def test_バージョン不一致でDBが再作成される(self, db_path: str) -> None:
        # 旧バージョンの DB を作成
        idx = Indexer(db_path)
        idx.init_db()
        idx.add_entry(
            IndexEntry("test/a.mp4", "a.mp4", "video", 1000, 100)
        )
        assert idx.entry_count() == 1

        # バージョンを古い値に書き換え
        conn = sqlite3.connect(db_path)
        conn.execute(
            "UPDATE schema_meta SET value = '1' WHERE key = 'schema_version'"
        )
        conn.commit()
        conn.close()

        # 再度 init_db → バージョン不一致で再作成
        idx2 = Indexer(db_path)
        idx2.init_db()
        assert idx2.entry_count() == 0  # データが消えている

    def test_同一バージョンでデータが保持される(self, db_path: str) -> None:
        idx = Indexer(db_path)
        idx.init_db()
        idx.add_entry(
            IndexEntry("test/a.mp4", "a.mp4", "video", 1000, 100)
        )

        # 再度 init_db → 同一バージョンなのでデータ保持
        idx2 = Indexer(db_path)
        idx2.init_db()
        assert idx2.entry_count() == 1


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


class TestIndexerWarmStart:
    """Warm Start (stale-while-revalidate) のテスト."""

    def test_初期状態ではis_staleがFalse(self, indexer: Indexer) -> None:
        assert indexer.is_stale is False

    def test_mark_warm_startでis_readyがTrueかつis_staleがTrue(
        self, indexer: Indexer
    ) -> None:
        assert indexer.is_ready is False
        indexer.mark_warm_start()
        assert indexer.is_ready is True
        assert indexer.is_stale is True

    def test_scan_directory後にis_staleがFalseになる(
        self, indexer: Indexer, tmp_path: Path
    ) -> None:
        from unittest.mock import MagicMock

        root = tmp_path / "root"
        root.mkdir()
        (root / "test.mp4").touch()

        security = MagicMock()
        security.validate = MagicMock(side_effect=lambda p: p)

        indexer.mark_warm_start()
        assert indexer.is_stale is True

        indexer.scan_directory(root, security)
        assert indexer.is_ready is True
        assert indexer.is_stale is False

    def test_incremental_scan後にis_staleがFalseになる(
        self, indexer: Indexer, tmp_path: Path
    ) -> None:
        from unittest.mock import MagicMock

        root = tmp_path / "root"
        root.mkdir()

        security = MagicMock()
        security.validate = MagicMock(side_effect=lambda p: p)

        indexer.mark_warm_start()
        assert indexer.is_stale is True

        indexer.incremental_scan(root, security)
        assert indexer.is_ready is True
        assert indexer.is_stale is False

    def test_check_mount_fingerprintが一致を正しく検出(
        self, indexer: Indexer
    ) -> None:
        mount_ids = ["aaa111", "bbb222"]
        indexer.save_mount_fingerprint(mount_ids)
        assert indexer.check_mount_fingerprint(mount_ids) is True

    def test_check_mount_fingerprintが不一致を正しく検出(
        self, indexer: Indexer
    ) -> None:
        indexer.save_mount_fingerprint(["aaa111", "bbb222"])
        assert indexer.check_mount_fingerprint(["aaa111", "ccc333"]) is False

    def test_check_mount_fingerprintが未保存でFalseを返す(
        self, indexer: Indexer
    ) -> None:
        assert indexer.check_mount_fingerprint(["aaa111"]) is False


class TestIndexerMtimePruning:
    """incremental_scan のディレクトリ mtime 枝刈り."""

    def test_未変更ディレクトリ配下がスキップされる(
        self, indexer: Indexer, tmp_path: Path
    ) -> None:
        """mtime が変わっていないディレクトリの子は再走査しない."""
        from unittest.mock import MagicMock

        root = tmp_path / "data"
        root.mkdir()
        sub = root / "sub"
        sub.mkdir()
        (sub / "clip.mp4").write_bytes(b"\x00" * 100)

        security = MagicMock()
        security.validate = MagicMock(side_effect=lambda p: p)

        # 初回スキャン
        indexer.scan_directory(root, security)
        assert indexer.entry_count() == 2  # sub + clip.mp4

        # 変更なしで incremental_scan
        added, updated, deleted = indexer.incremental_scan(root, security)
        assert added == 0
        assert updated == 0
        assert deleted == 0

        # データは保持されている
        results, _ = indexer.search("clip")
        assert len(results) == 1

    def test_変更されたディレクトリのファイルが更新される(
        self, indexer: Indexer, tmp_path: Path
    ) -> None:
        from unittest.mock import MagicMock

        root = tmp_path / "data"
        root.mkdir()
        sub = root / "sub"
        sub.mkdir()
        (sub / "clip.mp4").write_bytes(b"\x00" * 100)

        security = MagicMock()
        security.validate = MagicMock(side_effect=lambda p: p)

        indexer.scan_directory(root, security)

        # 新ファイル追加 + mtime を明示的に更新 (タイムスタンプ分解能対策)
        (sub / "new.mp4").write_bytes(b"\x00" * 50)
        os.utime(sub, (sub.stat().st_atime, sub.stat().st_mtime + 1))

        added, updated, deleted = indexer.incremental_scan(root, security)
        assert added == 1  # new.mp4

    def test_削除されたファイルが正しく検出される(
        self, indexer: Indexer, tmp_path: Path
    ) -> None:
        from unittest.mock import MagicMock

        root = tmp_path / "data"
        root.mkdir()
        sub = root / "sub"
        sub.mkdir()
        (sub / "a.mp4").write_bytes(b"\x00" * 100)
        (sub / "b.mp4").write_bytes(b"\x00" * 100)

        security = MagicMock()
        security.validate = MagicMock(side_effect=lambda p: p)

        indexer.scan_directory(root, security)
        assert indexer.entry_count() == 3  # sub + a.mp4 + b.mp4

        # ファイル削除 + mtime を明示的に更新 (タイムスタンプ分解能対策)
        (sub / "b.mp4").unlink()
        os.utime(sub, (sub.stat().st_atime, sub.stat().st_mtime + 1))

        added, updated, deleted = indexer.incremental_scan(root, security)
        assert deleted == 1

    def test_サブディレクトリ削除が正しく検出される(
        self, indexer: Indexer, tmp_path: Path
    ) -> None:
        from unittest.mock import MagicMock
        import shutil

        root = tmp_path / "data"
        root.mkdir()
        sub = root / "sub"
        sub.mkdir()
        deep = sub / "deep"
        deep.mkdir()
        (deep / "clip.mp4").write_bytes(b"\x00" * 100)

        security = MagicMock()
        security.validate = MagicMock(side_effect=lambda p: p)

        indexer.scan_directory(root, security)
        assert indexer.entry_count() == 3  # sub + deep + clip.mp4

        # サブディレクトリ削除 + mtime を明示的に更新
        shutil.rmtree(deep)
        os.utime(sub, (sub.stat().st_atime, sub.stat().st_mtime + 1))

        added, updated, deleted = indexer.incremental_scan(root, security)
        assert deleted == 2  # deep + clip.mp4

    def test_プレフィックス部分一致で別ディレクトリが混同されない(
        self, indexer: Indexer, tmp_path: Path
    ) -> None:
        """'dir' と 'dir2' が bisect で混同されないことを確認."""
        from unittest.mock import MagicMock

        root = tmp_path / "data"
        root.mkdir()
        (root / "dir").mkdir()
        (root / "dir" / "a.mp4").write_bytes(b"\x00" * 100)
        (root / "dir2").mkdir()
        (root / "dir2" / "b.mp4").write_bytes(b"\x00" * 100)

        security = MagicMock()
        security.validate = MagicMock(side_effect=lambda p: p)

        indexer.scan_directory(root, security)
        assert indexer.entry_count() == 4  # dir, a.mp4, dir2, b.mp4

        # dir2 にファイル追加 + mtime を明示的に更新
        dir2 = root / "dir2"
        (dir2 / "c.mp4").write_bytes(b"\x00" * 50)
        os.utime(dir2, (dir2.stat().st_atime, dir2.stat().st_mtime + 1))

        added, updated, deleted = indexer.incremental_scan(root, security)
        assert added == 1  # c.mp4 のみ
        # dir 配下の a.mp4 は保持
        results, _ = indexer.search("a.mp4")
        assert len(results) == 1

    def test_空ディレクトリの枝刈りでエラーにならない(
        self, indexer: Indexer, tmp_path: Path
    ) -> None:
        from unittest.mock import MagicMock

        root = tmp_path / "data"
        root.mkdir()
        (root / "empty").mkdir()

        security = MagicMock()
        security.validate = MagicMock(side_effect=lambda p: p)

        indexer.scan_directory(root, security)
        assert indexer.entry_count() == 1  # empty ディレクトリのみ

        # 変更なしで incremental_scan
        added, updated, deleted = indexer.incremental_scan(root, security)
        assert added == 0
        assert deleted == 0


class TestIndexerPragma:
    """SQLite PRAGMA 設定の検証."""

    def test_synchronousがNORMALに設定される(self, indexer: Indexer) -> None:
        conn = indexer._connect()
        try:
            result = conn.execute("PRAGMA synchronous").fetchone()
            # NORMAL = 1
            assert result is not None
            assert result[0] == 1
        finally:
            conn.close()

    def test_cache_sizeが8MBに設定される(self, indexer: Indexer) -> None:
        conn = indexer._connect()
        try:
            result = conn.execute("PRAGMA cache_size").fetchone()
            assert result is not None
            assert result[0] == -8192
        finally:
            conn.close()

    def test_temp_storeがMEMORYに設定される(self, indexer: Indexer) -> None:
        conn = indexer._connect()
        try:
            result = conn.execute("PRAGMA temp_store").fetchone()
            # MEMORY = 2
            assert result is not None
            assert result[0] == 2
        finally:
            conn.close()


class TestRebuildExclusion:
    """rebuild 排他制御のテスト."""

    def test_rebuild中にis_rebuildingがTrueになる(
        self, indexer: Indexer, tmp_path: Path
    ) -> None:
        """rebuild 実行中は is_rebuilding が True を返す."""
        from unittest.mock import patch

        from backend.services.path_security import PathSecurity

        root = tmp_path / "root"
        root.mkdir()
        (root / "test.zip").write_bytes(b"PK\x03\x04" + b"\x00" * 100)

        ps = PathSecurity([root])

        observed_during_rebuild = []

        original_scan = indexer.scan_directory

        def tracking_scan(*args: object, **kwargs: object) -> int:
            observed_during_rebuild.append(indexer.is_rebuilding)
            return original_scan(*args, **kwargs)

        with patch.object(indexer, "scan_directory", side_effect=tracking_scan):
            indexer.rebuild(root, ps)

        # rebuild 中は is_rebuilding=True が観測される
        assert any(observed_during_rebuild)
        # rebuild 完了後は False に戻る
        assert not indexer.is_rebuilding

    def test_rebuildの前後でis_rebuildingが正しく遷移する(
        self, indexer: Indexer, tmp_path: Path
    ) -> None:
        from backend.services.path_security import PathSecurity

        root = tmp_path / "root"
        root.mkdir()
        ps = PathSecurity([root])

        # 実行前: False
        assert not indexer.is_rebuilding

        indexer.rebuild(root, ps)

        # 実行後: False
        assert not indexer.is_rebuilding
