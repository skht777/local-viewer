"""ディレクトリ閲覧 API.

GET /api/browse          — ルート一覧 (ROOT_DIR 直下)
GET /api/browse/{node_id} — ディレクトリ一覧

ETag + 304 で未変更時の転送を省略する。
"""

import hashlib

from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import Response
from starlette.concurrency import run_in_threadpool

from backend.services.node_registry import BrowseResponse, EntryMeta, NodeRegistry

router = APIRouter(prefix="/api", tags=["browse"])


def get_node_registry() -> NodeRegistry:
    """NodeRegistry の DI スタブ.

    main.py の dependency_overrides で実インスタンスに差し替えられる。
    """
    msg = "NodeRegistry が DI で設定されていません"
    raise RuntimeError(msg)


def _compute_etag(entries: list[EntryMeta]) -> str:
    """entries の内容から ETag を生成する.

    node_id, name, kind, size_bytes, child_count を安定順で連結しハッシュ化。
    entries が変わらなければ同じ ETag を返す。
    """
    content = "|".join(
        f"{e.node_id},{e.name},{e.kind},{e.size_bytes},{e.child_count}" for e in entries
    )
    return hashlib.md5(content.encode()).hexdigest()  # noqa: S324


@router.get("/browse", response_model=BrowseResponse)
async def browse_root(
    request: Request,
    registry: NodeRegistry = Depends(get_node_registry),
) -> BrowseResponse | Response:
    """ルートディレクトリ一覧を返す.

    ROOT_DIR 直下のエントリをメタ情報付きで返す。
    ETag が一致すれば 304 Not Modified を返す。
    """
    root = registry.path_security.root_dir
    entries = await run_in_threadpool(registry.list_directory, root)

    etag = _compute_etag(entries)
    if request.headers.get("if-none-match") == etag:
        return Response(
            status_code=304,
            headers={"ETag": etag, "Cache-Control": "private, no-cache"},
        )

    response = BrowseResponse(
        current_node_id=None,
        current_name="root",
        parent_node_id=None,
        entries=entries,
    )
    return Response(
        content=response.model_dump_json(),
        media_type="application/json",
        headers={"ETag": etag, "Cache-Control": "private, no-cache"},
    )


@router.get("/browse/{node_id}", response_model=BrowseResponse)
async def browse_directory(
    node_id: str,
    request: Request,
    registry: NodeRegistry = Depends(get_node_registry),
) -> BrowseResponse | Response:
    """指定ディレクトリの一覧を返す.

    node_id → パス解決 → ディレクトリ一覧 → レスポンス。
    ディレクトリでない場合は 422。ETag が一致すれば 304。
    """
    path = registry.resolve(node_id)

    if not path.is_dir():
        raise HTTPException(
            status_code=422,
            detail={"error": "ディレクトリではありません", "code": "NOT_A_DIRECTORY"},
        )

    entries = await run_in_threadpool(registry.list_directory, path)

    etag = _compute_etag(entries)
    if request.headers.get("if-none-match") == etag:
        return Response(
            status_code=304,
            headers={"ETag": etag, "Cache-Control": "private, no-cache"},
        )

    parent_node_id = registry.get_parent_node_id(path)
    response = BrowseResponse(
        current_node_id=node_id,
        current_name=path.name,
        parent_node_id=parent_node_id,
        entries=entries,
    )
    return Response(
        content=response.model_dump_json(),
        media_type="application/json",
        headers={"ETag": etag, "Cache-Control": "private, no-cache"},
    )
