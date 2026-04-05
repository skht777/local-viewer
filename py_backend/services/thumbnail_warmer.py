"""サムネイルプリウォームサービス.

browse レスポンス返却後にバックグラウンドでサムネイルを事前生成する。
- asyncio.Semaphore(4) でユーザーリクエスト用スレッドプールを圧迫しない
- kind=image/archive/pdf/video のエントリのみ対象
- ThumbnailService.is_cached() でキャッシュ済みをスキップ
- _pending で重複排除
"""

from __future__ import annotations

import asyncio
import logging
import threading
from typing import TYPE_CHECKING

import pyvips
from starlette.concurrency import run_in_threadpool

if TYPE_CHECKING:
    from py_backend.services.archive_service import ArchiveService
    from py_backend.services.node_registry import EntryMeta, NodeRegistry
    from py_backend.services.thumbnail_service import ThumbnailService
    from py_backend.services.video_converter import VideoConverter

logger = logging.getLogger(__name__)

# プリウォームの同時実行数上限
_CONCURRENCY = 4


class ThumbnailWarmer:
    """browse 後のサムネイルプリウォーム."""

    def __init__(
        self,
        thumb_service: ThumbnailService,
        archive_service: ArchiveService,
        registry: NodeRegistry,
        video_converter: VideoConverter | None = None,
    ) -> None:
        self._thumb_service = thumb_service
        self._archive_service = archive_service
        self._registry = registry
        self._video_converter = video_converter
        self._semaphore = asyncio.Semaphore(_CONCURRENCY)
        self._pending: set[str] = set()
        self._lock = threading.Lock()

    def _is_likely_cached(self, node_id: str, modified_at: float | None) -> bool:
        """modified_at から近似 mtime_ns を復元してキャッシュをチェックする.

        modified_at は秒精度 (float) のため mtime_ns の完全復元は不可。
        近似値でキャッシュキーを生成し、ヒットすればスキップする。
        ミスの場合は _generate_thumbnail_bytes 内で正確な
        mtime_ns を使って再チェックする。
        """
        if modified_at is None:
            return False
        approx_mtime_ns = int(modified_at * 1_000_000_000)
        cache_key = self._thumb_service.make_cache_key(node_id, approx_mtime_ns)
        return self._thumb_service.is_cached(cache_key)

    async def warm(self, entries: list[EntryMeta]) -> None:
        """エントリのサムネイルをバックグラウンドで事前生成する.

        kind=image/archive/pdf/video のみ対象。キャッシュ済み・処理中はスキップ。
        """
        from py_backend.routers.thumbnail import _generate_thumbnail_bytes

        targets: list[str] = []
        for entry in entries:
            if entry.kind not in ("image", "archive", "pdf", "video"):
                continue
            # キャッシュ済みならスレッドプール投入をスキップ
            if self._is_likely_cached(entry.node_id, entry.modified_at):
                continue
            with self._lock:
                if entry.node_id in self._pending:
                    continue
                self._pending.add(entry.node_id)
            targets.append(entry.node_id)

        async def _warm_one(node_id: str) -> None:
            async with self._semaphore:
                try:
                    await run_in_threadpool(
                        _generate_thumbnail_bytes,
                        node_id,
                        self._registry,
                        self._archive_service,
                        self._thumb_service,
                        self._video_converter,
                    )
                except pyvips.Error, Exception:
                    logger.debug("プリウォーム失敗: %s", node_id)
                finally:
                    with self._lock:
                        self._pending.discard(node_id)

        await asyncio.gather(*(_warm_one(nid) for nid in targets))
