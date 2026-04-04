"""ディレクトリ閲覧 API.

GET /api/browse          — ルート一覧 (ROOT_DIR 直下)
GET /api/browse/{node_id} — ディレクトリ/アーカイブ一覧

ETag + 304 で未変更時の転送を省略する。
カーソルベースのページネーション + サーバーサイドソートに対応。
サムネイルプリウォーム: レスポンス返却後にバックグラウンド生成。
"""

import asyncio
import hashlib
import logging
import zipfile

from fastapi import APIRouter, Depends, HTTPException, Query, Request
from fastapi.responses import Response
from starlette.concurrency import run_in_threadpool

from backend.services.archive_service import ArchiveService
from backend.services.browse_cursor import (
    MAX_LIMIT,
    SortOrder,
    paginate,
)
from backend.services.node_registry import BrowseResponse, EntryMeta, NodeRegistry
from backend.services.thumbnail_warmer import ThumbnailWarmer

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/api", tags=["browse"])


def get_node_registry() -> NodeRegistry:
    """NodeRegistry の DI スタブ.

    main.py の dependency_overrides で実インスタンスに差し替えられる。
    """
    msg = "NodeRegistry が DI で設定されていません"
    raise RuntimeError(msg)


def get_archive_service() -> ArchiveService:
    """ArchiveService の DI スタブ."""
    msg = "ArchiveService が DI で設定されていません"
    raise RuntimeError(msg)


def get_thumbnail_warmer() -> ThumbnailWarmer | None:
    """ThumbnailWarmer の DI スタブ (未設定時は None)."""
    return None


def _compute_etag(entries: list[EntryMeta]) -> str:
    """entries の内容から ETag を生成する.

    node_id, name, kind, size_bytes, child_count を安定順で連結しハッシュ化。
    entries が変わらなければ同じ ETag を返す。
    """
    content = "|".join(
        f"{e.node_id},{e.name},{e.kind},{e.size_bytes},{e.child_count},{e.modified_at}"
        for e in entries
    )
    return hashlib.md5(content.encode()).hexdigest()  # noqa: S324


# プリウォームの fire-and-forget タスク参照 (GC 防止)
_prewarm_tasks: set[asyncio.Task[None]] = set()


def _schedule_prewarm(
    warmer: ThumbnailWarmer | None,
    entries: list[EntryMeta],
) -> None:
    """プリウォームタスクをスケジュールする (fire-and-forget)."""
    if warmer is None:
        return

    async def _run() -> None:
        try:
            await warmer.warm(entries)
        except Exception:
            logger.debug("プリウォームタスクでエラー", exc_info=True)

    task = asyncio.create_task(_run())
    _prewarm_tasks.add(task)
    task.add_done_callback(_prewarm_tasks.discard)


@router.get("/browse/{node_id}", response_model=BrowseResponse)
async def browse_directory(
    node_id: str,
    request: Request,
    registry: NodeRegistry = Depends(get_node_registry),
    archive_service: ArchiveService = Depends(get_archive_service),
    warmer: ThumbnailWarmer | None = Depends(get_thumbnail_warmer),
    sort: SortOrder = Query(SortOrder.NAME_ASC),
    limit: int | None = Query(None, ge=1, le=MAX_LIMIT),
    cursor: str | None = Query(None),
) -> BrowseResponse | Response:
    """指定ディレクトリまたはアーカイブの一覧を返す.

    node_id → パス解決 → ディレクトリ or アーカイブ一覧 → レスポンス。
    アーカイブの場合: 中身をエントリとして返す。
    ディレクトリでもアーカイブでもない場合は 422。

    ページネーション: limit + cursor でカーソルベースのページング。
    limit 省略時は全件返却 (後方互換)。
    """
    path = registry.resolve(node_id)

    # アーカイブファイルの場合: 中身をエントリとして返す
    if path.is_file() and archive_service.is_supported(path):
        try:
            archive_entries = await run_in_threadpool(
                archive_service.list_entries, path
            )
        except (zipfile.BadZipFile, OSError) as exc:
            logger.warning("アーカイブ読み取り失敗: %s (%s)", path, exc)
            raise HTTPException(
                status_code=422,
                detail={
                    "error": f"アーカイブを読み取れません: {exc}",
                    "code": "INVALID_ARCHIVE",
                },
            ) from exc
        entries = registry.list_archive_entries(path, archive_entries)

        etag = _compute_etag(entries)
        if request.headers.get("if-none-match") == etag:
            return Response(
                status_code=304,
                headers={
                    "ETag": etag,
                    "Cache-Control": "private, no-cache",
                },
            )

        # ページネーション適用
        page_entries, next_cursor, total_count = _apply_pagination(
            entries, sort, limit, cursor, etag
        )

        parent_node_id = registry.get_parent_node_id(path)
        ancestors = await run_in_threadpool(registry.get_ancestors, path)

        # プリウォーム: ページ内エントリのサムネイルをバックグラウンド生成
        _schedule_prewarm(warmer, page_entries)

        response = BrowseResponse(
            current_node_id=node_id,
            current_name=path.name,
            parent_node_id=parent_node_id,
            ancestors=ancestors,
            entries=page_entries,
            next_cursor=next_cursor,
            total_count=total_count if limit is not None else None,
        )
        return Response(
            content=response.model_dump_json(),
            media_type="application/json",
            headers={
                "ETag": etag,
                "Cache-Control": "private, no-cache",
            },
        )

    if not path.is_dir():
        raise HTTPException(
            status_code=422,
            detail={
                "error": "ディレクトリではありません",
                "code": "NOT_A_DIRECTORY",
            },
        )

    entries = await run_in_threadpool(registry.list_directory, path)

    etag = _compute_etag(entries)
    if request.headers.get("if-none-match") == etag:
        return Response(
            status_code=304,
            headers={"ETag": etag, "Cache-Control": "private, no-cache"},
        )

    # ページネーション適用
    page_entries, next_cursor, total_count = _apply_pagination(
        entries, sort, limit, cursor, etag
    )

    parent_node_id = registry.get_parent_node_id(path)
    ancestors = await run_in_threadpool(registry.get_ancestors, path)

    # プリウォーム: ページ内エントリのサムネイルをバックグラウンド生成
    _schedule_prewarm(warmer, page_entries)

    response = BrowseResponse(
        current_node_id=node_id,
        current_name=path.name,
        parent_node_id=parent_node_id,
        ancestors=ancestors,
        entries=page_entries,
        next_cursor=next_cursor,
        total_count=total_count if limit is not None else None,
    )
    return Response(
        content=response.model_dump_json(),
        media_type="application/json",
        headers={"ETag": etag, "Cache-Control": "private, no-cache"},
    )


def _apply_pagination(
    entries: list[EntryMeta],
    sort: SortOrder,
    limit: int | None,
    cursor: str | None,
    etag: str,
) -> tuple[list[EntryMeta], str | None, int]:
    """ページネーションを適用する.

    カーソル検証エラー時は HTTPException(400) を送出。
    """
    try:
        return paginate(entries, sort, limit, cursor, etag)
    except ValueError as exc:
        raise HTTPException(
            status_code=400,
            detail={"error": str(exc), "code": "INVALID_CURSOR"},
        ) from exc
