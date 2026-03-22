"""ファイル配信 API.

GET /api/file/{node_id} — ファイル配信 (Range 対応, ETag/Cache-Control 付き)
"""

import hashlib
from pathlib import Path

from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import FileResponse, Response

from backend.services.node_registry import NodeRegistry

router = APIRouter(prefix="/api", tags=["file"])


def get_node_registry() -> NodeRegistry:
    """NodeRegistry の DI スタブ.

    main.py の dependency_overrides で実インスタンスに差し替えられる。
    """
    msg = "NodeRegistry が DI で設定されていません"
    raise RuntimeError(msg)


def _compute_etag(path: Path) -> str:
    """ETag を生成する.

    ファイルの mtime + size + name からハッシュ生成。
    全内容のハッシュは大きなファイルで遅いため、メタ情報ベース。
    """
    st = path.stat()
    raw = f"{st.st_mtime_ns}:{st.st_size}:{path.name}"
    return hashlib.md5(raw.encode()).hexdigest()  # noqa: S324


@router.get("/file/{node_id}")
async def serve_file(
    node_id: str,
    request: Request,
    registry: NodeRegistry = Depends(get_node_registry),
) -> Response:
    """ファイルを配信する.

    - node_id からパスを解決
    - ディレクトリの場合は 422
    - ETag による条件付きリクエスト (If-None-Match → 304)
    - FileResponse で配信 (Range は Starlette が自動処理)
    - Cache-Control: private, max-age=3600
    """
    path = registry.resolve(node_id)

    if path.is_dir():
        raise HTTPException(
            status_code=422,
            detail={"error": "ディレクトリは配信できません", "code": "NOT_A_FILE"},
        )

    if not path.exists():
        raise HTTPException(
            status_code=404,
            detail={"error": "ファイルが見つかりません", "code": "NOT_FOUND"},
        )

    etag = _compute_etag(path)

    # 条件付きリクエスト: If-None-Match → 304
    if_none_match = request.headers.get("if-none-match")
    if if_none_match and if_none_match.strip('"') == etag:
        return Response(status_code=304, headers={"ETag": f'"{etag}"'})

    return FileResponse(
        path=path,
        headers={
            "ETag": f'"{etag}"',
            "Cache-Control": "private, max-age=3600",
        },
    )
