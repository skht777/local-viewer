"""マウントポイント一覧 API.

GET /api/mounts — マウントポイント一覧 (TopPage 用)
管理操作は manage_mounts.sh で行う。Web API は読み取り専用。
"""

from fastapi import APIRouter, Depends
from pydantic import BaseModel
from starlette.concurrency import run_in_threadpool

from backend.services.node_registry import NodeRegistry

router = APIRouter(prefix="/api", tags=["mounts"])


def get_node_registry() -> NodeRegistry:
    """NodeRegistry の DI スタブ."""
    msg = "NodeRegistry が DI で設定されていません"
    raise RuntimeError(msg)


class MountEntryResponse(BaseModel):
    """マウントポイント1件のレスポンス."""

    mount_id: str
    name: str
    node_id: str
    child_count: int | None


class MountListResponse(BaseModel):
    """マウントポイント一覧レスポンス."""

    mounts: list[MountEntryResponse]


@router.get("/mounts", response_model=MountListResponse)
async def list_mounts(
    registry: NodeRegistry = Depends(get_node_registry),
) -> MountListResponse:
    """マウントポイント一覧を返す (TopPage 用).

    NodeRegistry.list_mount_roots() で各マウントの node_id, 名前, 子要素数を取得。
    """
    entries = await run_in_threadpool(registry.list_mount_roots)
    # EntryMeta → MountEntryResponse に変換
    # mount_id は node_id を流用 (簡易実装、将来的に分離可能)
    mounts = [
        MountEntryResponse(
            mount_id=e.node_id,
            name=e.name,
            node_id=e.node_id,
            child_count=e.child_count,
        )
        for e in entries
    ]
    return MountListResponse(mounts=mounts)
