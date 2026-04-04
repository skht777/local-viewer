"""サムネイルプリウォームサービス.

browse レスポンス返却後にバックグラウンドでサムネイルを事前生成する。
- asyncio.Semaphore(4) でユーザーリクエスト用スレッドプールを圧迫しない
- kind=image/archive のエントリのみ対象
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
    from backend.services.archive_service import ArchiveService
    from backend.services.node_registry import EntryMeta, NodeRegistry
    from backend.services.thumbnail_service import ThumbnailService

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
    ) -> None:
        self._thumb_service = thumb_service
        self._archive_service = archive_service
        self._registry = registry
        self._semaphore = asyncio.Semaphore(_CONCURRENCY)
        self._pending: set[str] = set()
        self._lock = threading.Lock()

    async def warm(self, entries: list[EntryMeta]) -> None:
        """エントリのサムネイルをバックグラウンドで事前生成する.

        kind=image/archive のみ対象。キャッシュ済み・処理中はスキップ。
        """
        from backend.routers.thumbnail import _generate_thumbnail_bytes

        targets: list[str] = []
        for entry in entries:
            if entry.kind not in ("image", "archive"):
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
                    )
                except pyvips.Error, Exception:
                    logger.debug("プリウォーム失敗: %s", node_id)
                finally:
                    with self._lock:
                        self._pending.discard(node_id)

        await asyncio.gather(*(_warm_one(nid) for nid in targets))
