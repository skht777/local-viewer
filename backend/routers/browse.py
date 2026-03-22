"""ディレクトリ閲覧 API.

GET /api/browse          — ルート一覧 (ROOT_DIR 直下)
GET /api/browse/{node_id} — ディレクトリ一覧
"""

from fastapi import APIRouter, Depends, HTTPException

from backend.services.node_registry import BrowseResponse, NodeRegistry

router = APIRouter(prefix="/api", tags=["browse"])


def get_node_registry() -> NodeRegistry:
    """NodeRegistry の DI スタブ.

    main.py の dependency_overrides で実インスタンスに差し替えられる。
    """
    msg = "NodeRegistry が DI で設定されていません"
    raise RuntimeError(msg)


@router.get("/browse", response_model=BrowseResponse)
async def browse_root(
    registry: NodeRegistry = Depends(get_node_registry),
) -> BrowseResponse:
    """ルートディレクトリ一覧を返す.

    ROOT_DIR 直下のエントリをメタ情報付きで返す。
    """
    root = registry.path_security.root_dir
    entries = registry.list_directory(root)
    return BrowseResponse(
        current_node_id=None,
        current_name="root",
        parent_node_id=None,
        entries=entries,
    )


@router.get("/browse/{node_id}", response_model=BrowseResponse)
async def browse_directory(
    node_id: str,
    registry: NodeRegistry = Depends(get_node_registry),
) -> BrowseResponse:
    """指定ディレクトリの一覧を返す.

    node_id → パス解決 → ディレクトリ一覧 → レスポンス。
    ディレクトリでない場合は 422。
    """
    path = registry.resolve(node_id)

    if not path.is_dir():
        raise HTTPException(
            status_code=422,
            detail={"error": "ディレクトリではありません", "code": "NOT_A_DIRECTORY"},
        )

    entries = registry.list_directory(path)
    parent_node_id = registry.get_parent_node_id(path)
    return BrowseResponse(
        current_node_id=node_id,
        current_name=path.name,
        parent_node_id=parent_node_id,
        entries=entries,
    )
