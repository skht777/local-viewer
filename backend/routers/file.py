"""ファイル配信 API.

GET /api/file/{node_id} — ファイル配信 (Range 対応, ETag/Cache-Control 付き)
"""

import hashlib
import mimetypes
from pathlib import Path

from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import FileResponse, Response
from starlette.concurrency import run_in_threadpool

from backend.services.archive_service import ArchiveService
from backend.services.node_registry import MIME_MAP, NodeRegistry

router = APIRouter(prefix="/api", tags=["file"])


def get_archive_service() -> ArchiveService:
    """ArchiveService の DI スタブ."""
    msg = "ArchiveService が DI で設定されていません"
    raise RuntimeError(msg)


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
    archive_service: ArchiveService = Depends(get_archive_service),
) -> Response:
    """ファイルまたはアーカイブエントリを配信する.

    - アーカイブエントリの場合: キャッシュから取得 or 展開して配信
    - 通常ファイルの場合: FileResponse で配信 (Range は Starlette が自動処理)
    """
    # アーカイブエントリかチェック
    archive_entry = registry.resolve_archive_entry(node_id)
    if archive_entry is not None:
        return await _serve_archive_entry(archive_entry, request, archive_service)

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


async def _serve_archive_entry(
    archive_entry: tuple[Path, str],
    request: Request,
    archive_service: ArchiveService,
) -> Response:
    """アーカイブエントリを配信する.

    - run_in_threadpool で抽出 (キャッシュ付き)
    - ETag: md5(archive_mtime_ns + ":" + entry_name)
    - Cache-Control: private, max-age=3600
    """
    archive_path, entry_name = archive_entry

    # ETag 生成 (抽出前に計算可能)
    st = archive_path.stat()
    raw = f"{st.st_mtime_ns}:{entry_name}"
    etag = hashlib.md5(raw.encode()).hexdigest()  # noqa: S324

    # 条件付きリクエスト: If-None-Match → 304
    if_none_match = request.headers.get("if-none-match")
    if if_none_match and if_none_match.strip('"') == etag:
        return Response(status_code=304, headers={"ETag": f'"{etag}"'})

    # エントリ抽出 (CPU-bound → threadpool)
    data = await run_in_threadpool(
        archive_service.extract_entry, archive_path, entry_name
    )

    # MIME タイプ推定
    dot_idx = entry_name.rfind(".")
    ext = entry_name[dot_idx:].lower() if dot_idx > 0 else ""
    mime = (
        MIME_MAP.get(ext)
        or mimetypes.guess_type(entry_name)[0]
        or "application/octet-stream"
    )

    return Response(
        content=data,
        media_type=mime,
        headers={
            "ETag": f'"{etag}"',
            "Cache-Control": "private, max-age=3600",
            "Content-Length": str(len(data)),
        },
    )
