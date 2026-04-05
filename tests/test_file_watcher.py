"""File Watcher のテスト.

watchdog イベントを BatchFlushWorker 経由でインデックスに反映する。
"""

import time
from pathlib import Path
from unittest.mock import MagicMock

import pytest

from py_backend.services.file_watcher import BatchFlushWorker, IndexEventHandler


@pytest.fixture()
def mock_indexer() -> MagicMock:
    """モック Indexer."""
    m = MagicMock()
    m.add_entry = MagicMock()
    m.remove_entry = MagicMock()
    return m


@pytest.fixture()
def mock_path_security() -> MagicMock:
    """モック PathSecurity."""
    m = MagicMock()
    m.validate = MagicMock(side_effect=lambda p: p)
    return m


@pytest.fixture()
def worker(
    mock_indexer: MagicMock,
    mock_path_security: MagicMock,
    tmp_path: Path,
) -> BatchFlushWorker:
    """テスト用 BatchFlushWorker (flush 間隔短め)."""
    w = BatchFlushWorker(
        indexer=mock_indexer,
        path_security=mock_path_security,
        root_dir=tmp_path,
        interval=0.1,
    )
    return w


class TestBatchFlushWorker:
    """BatchFlushWorker の基本動作."""

    def test_workerの開始と停止(self, worker: BatchFlushWorker) -> None:
        worker.start()
        assert worker.is_alive()
        worker.stop()
        worker.join(timeout=2)
        assert not worker.is_alive()

    def test_enqueueしたイベントがflushで処理される(
        self,
        worker: BatchFlushWorker,
        mock_indexer: MagicMock,
        tmp_path: Path,
    ) -> None:
        # 実ファイルを作成 (インデックス対象の拡張子)
        fp = tmp_path / "test.mp4"
        fp.write_bytes(b"\x00" * 10)

        worker.start()
        worker.enqueue(str(fp), "add")
        time.sleep(0.3)  # flush 待ち
        worker.stop()
        worker.join(timeout=2)

        mock_indexer.add_entry.assert_called_once()

    def test_removeイベントでremove_entryが呼ばれる(
        self,
        worker: BatchFlushWorker,
        mock_indexer: MagicMock,
        tmp_path: Path,
    ) -> None:
        worker.start()
        worker.enqueue(str(tmp_path / "deleted.jpg"), "remove")
        time.sleep(0.3)
        worker.stop()
        worker.join(timeout=2)

        mock_indexer.remove_entry.assert_called_once()

    def test_同一パスの連続イベントは最新のみ処理(
        self,
        worker: BatchFlushWorker,
        mock_indexer: MagicMock,
        tmp_path: Path,
    ) -> None:
        fp = tmp_path / "rapid.mp4"
        fp.write_bytes(b"\x00" * 10)

        worker.start()
        # add → remove → add で最終的に add のみ
        worker.enqueue(str(fp), "add")
        worker.enqueue(str(fp), "remove")
        worker.enqueue(str(fp), "add")
        time.sleep(0.3)
        worker.stop()
        worker.join(timeout=2)

        # 最後の action = "add" のみ処理
        mock_indexer.add_entry.assert_called_once()
        mock_indexer.remove_entry.assert_not_called()

    def test_大量イベントでスレッド数が増えない(
        self,
        worker: BatchFlushWorker,
        tmp_path: Path,
    ) -> None:
        import threading

        # ファイル作成
        for i in range(100):
            (tmp_path / f"file_{i}.mp4").write_bytes(b"\x00" * 10)

        thread_count_before = threading.active_count()
        worker.start()

        for i in range(100):
            worker.enqueue(str(tmp_path / f"file_{i}.mp4"), "add")

        time.sleep(0.3)
        thread_count_during = threading.active_count()
        worker.stop()
        worker.join(timeout=2)

        # ワーカースレッド 1 本のみ増加 (per-path Timer ではない)
        assert thread_count_during - thread_count_before <= 2


class TestIndexEventHandler:
    """IndexEventHandler の基本動作."""

    def test_createdイベントでpendingに追加される(self, tmp_path: Path) -> None:
        worker = MagicMock()
        handler = IndexEventHandler(
            worker=worker,
            root_dir=tmp_path,
        )

        from watchdog.events import FileCreatedEvent

        fp = tmp_path / "new.mp4"
        fp.write_bytes(b"\x00" * 10)
        handler.on_created(FileCreatedEvent(str(fp)))

        worker.enqueue.assert_called_once_with(str(fp), "add")

    def test_deletedイベントでpendingに追加される(self, tmp_path: Path) -> None:
        worker = MagicMock()
        handler = IndexEventHandler(worker=worker, root_dir=tmp_path)

        from watchdog.events import FileDeletedEvent

        handler.on_deleted(FileDeletedEvent(str(tmp_path / "gone.mp4")))
        worker.enqueue.assert_called_once_with(
            str(tmp_path / "gone.mp4"), "remove"
        )

    def test_movedイベントで削除と追加がpendingに追加される(
        self, tmp_path: Path
    ) -> None:
        worker = MagicMock()
        handler = IndexEventHandler(worker=worker, root_dir=tmp_path)

        from watchdog.events import FileMovedEvent

        src = str(tmp_path / "old.mp4")
        dest = str(tmp_path / "new.mp4")
        handler.on_moved(FileMovedEvent(src, dest))

        assert worker.enqueue.call_count == 2
        worker.enqueue.assert_any_call(src, "remove")
        worker.enqueue.assert_any_call(dest, "add")

    def test_隠しファイルが無視される(self, tmp_path: Path) -> None:
        worker = MagicMock()
        handler = IndexEventHandler(worker=worker, root_dir=tmp_path)

        from watchdog.events import FileCreatedEvent

        handler.on_created(FileCreatedEvent(str(tmp_path / ".hidden")))
        worker.enqueue.assert_not_called()

    def test_画像ファイルのイベントが無視される(self, tmp_path: Path) -> None:
        worker = MagicMock()
        handler = IndexEventHandler(worker=worker, root_dir=tmp_path)

        from watchdog.events import FileCreatedEvent

        handler.on_created(FileCreatedEvent(str(tmp_path / "photo.jpg")))
        handler.on_created(FileCreatedEvent(str(tmp_path / "pic.png")))
        worker.enqueue.assert_not_called()

    def test_対象外拡張子のファイルが無視される(self, tmp_path: Path) -> None:
        worker = MagicMock()
        handler = IndexEventHandler(worker=worker, root_dir=tmp_path)

        from watchdog.events import FileCreatedEvent

        handler.on_created(FileCreatedEvent(str(tmp_path / "readme.txt")))
        worker.enqueue.assert_not_called()

    def test_ディレクトリ作成でpendingに追加される(self, tmp_path: Path) -> None:
        worker = MagicMock()
        handler = IndexEventHandler(worker=worker, root_dir=tmp_path)

        from watchdog.events import DirCreatedEvent

        d = tmp_path / "newdir"
        d.mkdir()
        handler.on_created(DirCreatedEvent(str(d)))
        worker.enqueue.assert_called_once_with(str(d), "add")


class TestFileWatcher:
    """FileWatcher の起動制御."""

    def test_二重startでエラーにならない(
        self,
        mock_indexer: MagicMock,
        mock_path_security: MagicMock,
        tmp_path: Path,
    ) -> None:
        from py_backend.services.file_watcher import FileWatcher

        watcher = FileWatcher(
            indexer=mock_indexer,
            path_security=mock_path_security,
            root_dir=tmp_path,
        )
        watcher.start()
        assert watcher.is_running
        # 2回目の start はエラーなく無視される
        watcher.start()
        assert watcher.is_running
        watcher.stop()
