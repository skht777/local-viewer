"""検索 API.

GET  /api/search         — キーワード検索 (FTS5 trigram)
POST /api/index/rebuild  — インデックス再構築 (バックグラウンド)
"""

import asyncio
import logging
import time
from pathlib import Path

from fastapi import APIRouter, Depends, HTTPException, Query
from pydantic import BaseModel
from starlette.concurrency import run_in_threadpool

from backend.config import Settings, get_settings
from backend.errors import PathSecurityError
from backend.services.indexer import Indexer
from backend.services.node_registry import NodeRegistry
from backend.services.path_security import PathSecurity

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/api", tags=["search"])

# rebuild の排他制御
_last_rebuild_time: float = 0.0
_background_tasks: set[asyncio.Task[None]] = set()

# 有効な kind 値
_VALID_KINDS = frozenset({"directory", "image", "video", "pdf", "archive"})


# --- DI スタブ ---


def get_indexer() -> Indexer:
    """Indexer の DI スタブ."""
    msg = "Indexer が DI で設定されていません"
    raise RuntimeError(msg)


def get_node_registry() -> NodeRegistry:
    """NodeRegistry の DI スタブ."""
    msg = "NodeRegistry が DI で設定されていません"
    raise RuntimeError(msg)


def get_path_security() -> PathSecurity:
    """PathSecurity の DI スタブ."""
    msg = "PathSecurity が DI で設定されていません"
    raise RuntimeError(msg)


# --- レスポンスモデル ---


class SearchResultResponse(BaseModel):
    """検索結果の 1 件."""

    node_id: str
    parent_node_id: str | None
    name: str
    kind: str
    relative_path: str
    size_bytes: int | None


class SearchResponse(BaseModel):
    """検索 API のレスポンス."""

    results: list[SearchResultResponse]
    has_more: bool
    query: str
    is_stale: bool = False


# --- エンドポイント ---


@router.get("/search", response_model=SearchResponse)
async def search(
    q: str = Query(min_length=2, max_length=200),
    kind: str | None = None,
    limit: int = Query(default=50, ge=1, le=200),
    offset: int = Query(default=0, ge=0),
    indexer: Indexer = Depends(get_indexer),
    registry: NodeRegistry = Depends(get_node_registry),
    path_security: PathSecurity = Depends(get_path_security),
    settings: Settings = Depends(get_settings),
) -> SearchResponse:
    """キーワード検索.

    - q を FTS5 trigram クエリに変換 (2 文字は LIKE フォールバック)
    - 各結果に node_id を付与 (NodeRegistry 経由)
    - 削除済みファイルはスキップし、limit まで結果を埋める
    """
    # 前後空白を除去し、strip 後の文字数を再チェック
    q = q.strip()
    if len(q) < 2:
        raise HTTPException(status_code=422, detail="検索クエリは2文字以上必要です")

    if not indexer.is_ready:
        raise HTTPException(status_code=503, detail="インデックス構築中です")

    if kind and kind not in _VALID_KINDS:
        raise HTTPException(status_code=422, detail=f"不正な kind: {kind}")

    # mount_id → root_dir マッピング (NodeRegistry から取得)
    mount_map = registry.mount_id_map
    # 後方互換: mount_map が空なら root_dirs[0] を使う
    if not mount_map:
        mount_map = {"": path_security.root_dirs[0]}

    # 検索実行 (CPU バウンド)
    search_results = await run_in_threadpool(
        _resolve_search_results,
        indexer,
        registry,
        path_security,
        mount_map,
        q,
        kind,
        limit,
        offset,
    )

    search_results.is_stale = indexer.is_stale
    return search_results


def _resolve_search_results(
    indexer: Indexer,
    registry: NodeRegistry,
    path_security: PathSecurity,
    mount_map: dict[str, Path],
    query: str,
    kind: str | None,
    limit: int,
    offset: int,
) -> SearchResponse:
    """検索結果に node_id を付与して返す.

    relative_path の形式: "{mount_id}/{actual_path}" からマウントルートを解決。
    削除済みファイルをスキップしつつ limit まで結果を埋める。
    """
    collect_limit = limit + 1
    results: list[SearchResultResponse] = []
    db_offset = offset
    max_iterations = 5

    for _ in range(max_iterations):
        hits, has_more_in_db = indexer.search(
            query, kind=kind, limit=collect_limit * 2, offset=db_offset
        )
        if not hits:
            break

        for hit in hits:
            abs_path = _resolve_hit_path(hit.relative_path, mount_map)
            if abs_path is None:
                continue
            try:
                path_security.validate_existing(abs_path)
            except PathSecurityError, OSError:
                continue

            node_id = registry.register(abs_path)

            # 親ディレクトリの node_id
            parent_path = abs_path.parent
            root = path_security.find_root_for(parent_path)
            if root is not None and parent_path == root:
                parent_node_id = None
            else:
                try:
                    path_security.validate(parent_path)
                    parent_node_id = registry.register(parent_path)
                except PathSecurityError, OSError:
                    parent_node_id = None

            results.append(
                SearchResultResponse(
                    node_id=node_id,
                    parent_node_id=parent_node_id,
                    name=hit.name,
                    kind=hit.kind,
                    relative_path=hit.relative_path,
                    size_bytes=hit.size_bytes,
                )
            )

            if len(results) >= collect_limit:
                break

        db_offset += len(hits)

        if len(results) >= collect_limit or not has_more_in_db:
            break

    has_more = len(results) > limit

    return SearchResponse(
        results=results[:limit],
        has_more=has_more,
        query=query,
    )


def _resolve_hit_path(relative_path: str, mount_map: dict[str, Path]) -> Path | None:
    """relative_path から絶対パスを解決する.

    形式: "{mount_id}/{actual_relative}" → mount_map[mount_id] / actual_relative
    mount_id がない (legacy) 場合は最初のマウントを使用。
    """
    parts = relative_path.split("/", 1)
    if len(parts) == 2 and parts[0] in mount_map:
        return mount_map[parts[0]] / parts[1]
    # mount_id プレフィックスがない場合 (単一マウントの後方互換)
    if mount_map:
        first_root = next(iter(mount_map.values()))
        return first_root / relative_path
    return None


@router.post("/index/rebuild", status_code=202)
async def rebuild_index(
    indexer: Indexer = Depends(get_indexer),
    path_security: PathSecurity = Depends(get_path_security),
    registry: NodeRegistry = Depends(get_node_registry),
    settings: Settings = Depends(get_settings),
) -> dict[str, str]:
    """インデックス再構築をバックグラウンドで開始する.

    - 全マウントポイントを対象にリビルド
    - asyncio.create_task でバックグラウンド実行
    - Indexer.is_rebuilding で排他制御 (409)
    - 前回再構築から REBUILD_RATE_LIMIT_SECONDS 秒以内は 429
    """
    global _last_rebuild_time

    now = time.monotonic()
    if now - _last_rebuild_time < settings.rebuild_rate_limit_seconds:
        raise HTTPException(
            status_code=429,
            detail="再構築のレート制限中です",
        )

    if indexer.is_rebuilding:
        raise HTTPException(status_code=409, detail="再構築が既に実行中です")

    _last_rebuild_time = now

    # mount_id → root_dir マッピング (全マウント対象)
    mount_map = registry.mount_id_map
    if not mount_map:
        mount_map = {"": path_security.root_dirs[0]}

    # バックグラウンドで再構築 (参照保持で GC 防止)
    task = asyncio.create_task(_background_rebuild(indexer, mount_map, path_security))
    _background_tasks.add(task)
    task.add_done_callback(_background_tasks.discard)

    return {
        "status": "accepted",
        "message": "インデックス再構築をバックグラウンドで開始しました",
    }


async def _background_rebuild(
    indexer: Indexer,
    mounts: dict[str, Path],
    path_security: PathSecurity,
) -> None:
    """バックグラウンドで全マウントのインデックスを再構築する."""
    try:
        count = await run_in_threadpool(indexer.rebuild, mounts, path_security)
        logger.info("インデックス再構築完了: %d エントリ", count)
    except Exception:
        logger.exception("インデックス再構築に失敗しました")
