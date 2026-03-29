"""ファイルシステム変更監視.

- watchdog の Observer (inotify/kqueue) をデフォルトで使用
- NAS/bind mount 向けに PollingObserver にフォールバック
- 単一 BatchFlushWorker スレッドがイベントを一定間隔で flush
"""

from __future__ import annotations

import logging
import os
import threading
from pathlib import Path
from typing import TYPE_CHECKING

from watchdog.events import (
    DirMovedEvent,
    FileMovedEvent,
    FileSystemEvent,
    FileSystemEventHandler,
)
from watchdog.observers import Observer
from watchdog.observers.polling import PollingObserver

from backend.services.extensions import (
    ARCHIVE_EXTENSIONS,
    IMAGE_EXTENSIONS,
    PDF_EXTENSIONS,
    VIDEO_EXTENSIONS,
)
from backend.services.indexer import IndexEntry, _classify_by_extension

if TYPE_CHECKING:
    from backend.services.indexer import Indexer
    from backend.services.path_security import PathSecurity

logger = logging.getLogger(__name__)

# インデックス対象の拡張子
_TARGET_EXTENSIONS = (
    IMAGE_EXTENSIONS | VIDEO_EXTENSIONS | PDF_EXTENSIONS | ARCHIVE_EXTENSIONS
)


class BatchFlushWorker(threading.Thread):
    """pending イベントを一定間隔で flush するワーカー.

    - 同一パスへの連続イベントは最新の action で上書き
    - スレッド 1 本で全イベントを処理 (per-path Timer ではない)
    """

    def __init__(
        self,
        indexer: Indexer,
        path_security: PathSecurity,
        root_dir: Path,
        interval: float = 1.0,
    ) -> None:
        super().__init__(daemon=True, name="index-flush-worker")
        self._indexer = indexer
        self._path_security = path_security
        self._root_dir = root_dir
        self._interval = interval
        self._pending: dict[str, str] = {}  # path → action ("add" | "remove")
        self._lock = threading.Lock()
        self._stop_event = threading.Event()

    def enqueue(self, path: str, action: str) -> None:
        """イベントを pending に追加 (スレッドセーフ)."""
        with self._lock:
            self._pending[path] = action

    def run(self) -> None:
        """一定間隔で pending を flush する."""
        while not self._stop_event.is_set():
            self._stop_event.wait(self._interval)
            self._flush()

    def stop(self) -> None:
        """ワーカーを停止する."""
        self._stop_event.set()

    def _flush(self) -> None:
        """pending を取得してインデックスに反映する."""
        with self._lock:
            batch = dict(self._pending)
            self._pending.clear()

        for path_str, action in batch.items():
            try:
                self._process(path_str, action)
            except Exception:
                logger.debug("flush 処理失敗: %s %s", action, path_str, exc_info=True)

    def _process(self, path_str: str, action: str) -> None:
        """1 件のイベントを処理する."""
        p = Path(path_str)

        if action == "remove":
            try:
                rel = str(p.relative_to(self._root_dir))
            except ValueError:
                return
            self._indexer.remove_entry(rel)
            return

        # action == "add"
        if not p.exists():
            return

        try:
            self._path_security.validate(p)
        except Exception:
            return

        try:
            rel = str(p.relative_to(self._root_dir))
        except ValueError:
            return

        if p.is_dir():
            kind = "directory"
            size = None
        else:
            kind = _classify_by_extension(p.name)
            if kind == "other":
                return
            try:
                size = p.stat().st_size
            except OSError:
                return

        try:
            mtime = p.stat().st_mtime_ns
        except OSError:
            return

        self._indexer.add_entry(
            IndexEntry(
                relative_path=rel,
                name=p.name,
                kind=kind,
                size_bytes=size,
                mtime_ns=mtime,
            )
        )


class IndexEventHandler(FileSystemEventHandler):
    """watchdog イベントを BatchFlushWorker の pending に蓄積する.

    - 隠しファイル (.xxx) はスキップ
    - 対象外拡張子のファイルはスキップ
    - ディレクトリイベントは常に処理
    """

    def __init__(self, worker: BatchFlushWorker, root_dir: Path) -> None:
        super().__init__()
        self._worker = worker
        self._root_dir = root_dir

    def on_created(self, event: FileSystemEvent) -> None:
        """ファイル/ディレクトリ作成."""
        src = str(event.src_path)
        if not self._should_process_path(src, event.is_directory):
            return
        self._worker.enqueue(src, "add")

    def on_deleted(self, event: FileSystemEvent) -> None:
        """ファイル/ディレクトリ削除."""
        src = str(event.src_path)
        if not self._should_process_path(src, event.is_directory):
            return
        self._worker.enqueue(src, "remove")

    def on_moved(self, event: FileSystemEvent) -> None:
        """ファイル/ディレクトリ移動."""
        if isinstance(event, (FileMovedEvent, DirMovedEvent)):
            src = str(event.src_path)
            dest = str(event.dest_path)
            if self._should_process_path(src, event.is_directory):
                self._worker.enqueue(src, "remove")
            if self._should_process_path(dest, event.is_directory):
                self._worker.enqueue(dest, "add")

    def _should_process_path(self, path: str, is_directory: bool) -> bool:
        """パスがインデックス対象か判定する."""
        name = os.path.basename(path)

        # 隠しファイル/ディレクトリをスキップ
        if name.startswith("."):
            return False

        # ディレクトリは常に対象
        if is_directory:
            return True

        # 拡張子チェック
        dot_idx = name.rfind(".")
        if dot_idx <= 0:
            return False
        ext = name[dot_idx:].lower()
        return ext in _TARGET_EXTENSIONS


class FileWatcher:
    """ファイルシステム変更を監視してインデックスを更新する.

    mounts: (mount_id, root_dir) のリスト。各マウントを個別に監視。
    単一ルートの場合は root_dir パラメータで後方互換。
    """

    def __init__(
        self,
        indexer: Indexer,
        root_dir: Path | None = None,
        path_security: PathSecurity | None = None,
        mode: str = "auto",
        poll_interval: int = 30,
        flush_interval: float = 1.0,
        *,
        mounts: list[tuple[str, Path]] | None = None,
    ) -> None:
        self._indexer = indexer
        self._path_security = path_security
        self._mode = mode
        self._poll_interval = poll_interval
        self._flush_interval = flush_interval
        # 複数マウント対応: mounts が指定されればそれを使用
        if mounts is not None:
            self._mounts = mounts
        elif root_dir is not None:
            self._mounts = [("", root_dir)]
        else:
            msg = "root_dir または mounts が必要です"
            raise ValueError(msg)
        # mypy: Observer は変数として扱われるため Any を使用
        self._observer: Observer | PollingObserver | None = None  # type: ignore[valid-type]
        self._workers: list[BatchFlushWorker] = []

    def start(self) -> None:
        """監視を開始する (Observer + BatchFlushWorker per mount)."""
        actual_mode = self._detect_mode() if self._mode == "auto" else self._mode
        if actual_mode == "polling":
            self._observer = PollingObserver(timeout=self._poll_interval)
            logger.info("FileWatcher: polling モード (間隔 %ds)", self._poll_interval)
        else:
            self._observer = Observer()
            logger.info("FileWatcher: native モード (inotify)")

        for mount_id, root_dir in self._mounts:
            worker = BatchFlushWorker(
                indexer=self._indexer,
                path_security=self._path_security,
                root_dir=root_dir,
                interval=self._flush_interval,
            )
            handler = IndexEventHandler(
                worker=worker,
                root_dir=root_dir,
            )
            self._observer.schedule(handler, str(root_dir), recursive=True)
            worker.start()
            self._workers.append(worker)
            logger.info(
                "FileWatcher: 監視開始 %s (%s)",
                root_dir,
                mount_id or "default",
            )

        self._observer.start()

    def stop(self) -> None:
        """監視を停止する."""
        if self._observer:
            self._observer.stop()
            self._observer.join(timeout=5)
            self._observer = None
        for worker in self._workers:
            worker.stop()
            worker.join(timeout=5)
        self._workers.clear()

    @property
    def is_running(self) -> bool:
        """監視中か."""
        return self._observer is not None and self._observer.is_alive()

    def _detect_mode(self) -> str:
        """ファイルシステム種別を検出して最適なモードを返す.

        /proc/mounts を読んで nfs/cifs/fuse なら polling。
        """
        try:
            mounts = Path("/proc/mounts").read_text()
            root_str = str(self._mounts[0][1])
            for line in mounts.splitlines():
                parts = line.split()
                if len(parts) >= 3 and root_str.startswith(parts[1]):
                    fs_type = parts[2].lower()
                    if fs_type in ("nfs", "nfs4", "cifs", "fuse", "9p"):
                        return "polling"
        except OSError, ValueError:
            pass
        return "native"
