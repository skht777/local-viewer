"""ファイル配信 API.

GET /api/file/{node_id} — ファイル配信 (Range 対応, ETag/Cache-Control 付き)
"""

import hashlib
import mimetypes
from pathlib import Path, PurePosixPath

from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import FileResponse, Response
from starlette.concurrency import run_in_threadpool

from backend.services.archive_service import ArchiveService
from backend.services.node_registry import MIME_MAP, VIDEO_EXTENSIONS, NodeRegistry
from backend.services.temp_file_cache import TempFileCache

router = APIRouter(prefix="/api", tags=["file"])


def get_archive_service() -> ArchiveService:
    """ArchiveService の DI スタブ."""
    msg = "ArchiveService が DI で設定されていません"
    raise RuntimeError(msg)


def get_temp_file_cache() -> TempFileCache:
    """TempFileCache の DI スタブ."""
    msg = "TempFileCache が DI で設定されていません"
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
    temp_cache: TempFileCache = Depends(get_temp_file_cache),
) -> Response:
    """ファイルまたはアーカイブエントリを配信する.

    - アーカイブエントリの場合: キャッシュから取得 or 展開して配信
    - 通常ファイルの場合: FileResponse で配信 (Range は Starlette が自動処理)
    """
    # アーカイブエントリかチェック
    archive_entry = registry.resolve_archive_entry(node_id)
    if archive_entry is not None:
        return await _serve_archive_entry(
            archive_entry, request, archive_service, temp_cache
        )

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


def _entry_ext(entry_name: str) -> str:
    """エントリ名から拡張子を取得する."""
    dot_idx = entry_name.rfind(".")
    return entry_name[dot_idx:].lower() if dot_idx > 0 else ""


def _entry_mime(entry_name: str) -> str:
    """エントリ名から MIME タイプを推定する."""
    ext = _entry_ext(entry_name)
    return (
        MIME_MAP.get(ext)
        or mimetypes.guess_type(entry_name)[0]
        or "application/octet-stream"
    )


def _archive_etag(st_mtime_ns: int, entry_name: str) -> str:
    """アーカイブエントリの ETag を生成する."""
    raw = f"{st_mtime_ns}:{entry_name}"
    return hashlib.md5(raw.encode()).hexdigest()  # noqa: S324


async def _serve_archive_entry(
    archive_entry: tuple[Path, str],
    request: Request,
    archive_service: ArchiveService,
    temp_cache: TempFileCache,
) -> Response:
    """アーカイブエントリを配信する.

    - 動画エントリ: tmpfile 経由で FileResponse (Range 対応)
    - 画像エントリ: Response(content=bytes)
    """
    archive_path, entry_name = archive_entry
    st = archive_path.stat()
    etag = _archive_etag(st.st_mtime_ns, entry_name)

    # 条件付きリクエスト: If-None-Match → 304
    if_none_match = request.headers.get("if-none-match")
    if if_none_match and if_none_match.strip('"') == etag:
        return Response(status_code=304, headers={"ETag": f'"{etag}"'})

    # 動画エントリは tmpfile 経由で FileResponse (Range 対応)
    ext = _entry_ext(entry_name)
    if ext in VIDEO_EXTENSIONS:
        return await _serve_archive_video_entry(
            archive_path,
            entry_name,
            st.st_mtime_ns,
            etag,
            archive_service,
            temp_cache,
        )

    # 画像エントリ: メモリから Response
    data = await run_in_threadpool(
        archive_service.extract_entry, archive_path, entry_name
    )
    return Response(
        content=data,
        media_type=_entry_mime(entry_name),
        headers={
            "ETag": f'"{etag}"',
            "Cache-Control": "private, max-age=3600",
            "Content-Length": str(len(data)),
        },
    )


async def _serve_archive_video_entry(
    archive_path: Path,
    entry_name: str,
    mtime_ns: int,
    etag: str,
    archive_service: ArchiveService,
    temp_cache: TempFileCache,
) -> Response:
    """動画エントリを tmpfile 経由で配信する (Range 対応)."""
    key = temp_cache.make_key(archive_path, mtime_ns, entry_name)
    headers = {
        "ETag": f'"{etag}"',
        "Cache-Control": "private, max-age=3600",
    }

    # キャッシュヒット
    cached = temp_cache.get(key)
    if cached is not None:
        return FileResponse(
            path=cached,
            media_type=_entry_mime(entry_name),
            headers=headers,
        )

    # キャッシュミス: 展開してディスクに保存
    data = await run_in_threadpool(
        archive_service.extract_entry, archive_path, entry_name
    )
    suffix = PurePosixPath(entry_name).suffix
    path = temp_cache.put(key, data, suffix=suffix)
    del data  # メモリを早期解放

    return FileResponse(
        path=path,
        media_type=_entry_mime(entry_name),
        headers=headers,
    )
