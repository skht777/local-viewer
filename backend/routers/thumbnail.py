"""サムネイル配信 API.

GET /api/thumbnail/{node_id} — 画像/アーカイブのサムネイルを返す
- image → Pillow でリサイズ
- archive → 先頭画像エントリをサムネイル化
- ディレクトリ / 画像なし → 422 / 404
"""

import hashlib
import logging
from pathlib import Path

import pyvips
from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import Response
from starlette.concurrency import run_in_threadpool

from backend.services.archive_service import ArchiveService
from backend.services.extensions import IMAGE_EXTENSIONS
from backend.services.node_registry import NodeRegistry
from backend.services.thumbnail_service import ThumbnailService

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/api", tags=["thumbnail"])


def get_node_registry() -> NodeRegistry:
    """NodeRegistry の DI スタブ."""
    msg = "NodeRegistry が DI で設定されていません"
    raise RuntimeError(msg)


def get_archive_service() -> ArchiveService:
    """ArchiveService の DI スタブ."""
    msg = "ArchiveService が DI で設定されていません"
    raise RuntimeError(msg)


def get_thumbnail_service() -> ThumbnailService:
    """ThumbnailService の DI スタブ."""
    msg = "ThumbnailService が DI で設定されていません"
    raise RuntimeError(msg)


def _compute_etag(mtime_ns: int, node_id: str) -> str:
    """サムネイル用 ETag を生成する."""
    raw = f"thumb:{mtime_ns}:{node_id}"
    return hashlib.md5(raw.encode()).hexdigest()  # noqa: S324


# ?v= パラメータ付き → immutable 長期キャッシュ
# ?v= なし → 従来の ETag ベース短期キャッシュ (後方互換)
_CACHE_IMMUTABLE = "public, max-age=31536000, immutable"
_CACHE_DEFAULT = "private, max-age=3600"


def _cache_control(request: Request) -> str:
    """リクエストの ?v= パラメータ有無で Cache-Control を決定する."""
    return _CACHE_IMMUTABLE if request.query_params.get("v") else _CACHE_DEFAULT


def _generate_thumbnail_bytes(
    node_id: str,
    registry: NodeRegistry,
    archive_service: ArchiveService,
    thumb_service: ThumbnailService,
) -> tuple[bytes, str]:
    """node_id からサムネイル JPEG バイト列と ETag を生成する.

    3 つのケースを処理:
    - アーカイブ内エントリ → 抽出してサムネイル化
    - アーカイブファイル自体 → 先頭画像エントリのサムネイル
    - 通常の画像ファイル → パスからサムネイル生成

    CPU-bound のため run_in_threadpool で呼び出すこと。

    Raises:
        HTTPException: ファイル不在 (404)、非対応形式 (422)、破損データ (422)
        pyvips.Error: 画像デコード失敗 (呼び出し側で 422 に変換)
    """
    # アーカイブエントリかチェック
    archive_entry = registry.resolve_archive_entry(node_id)
    if archive_entry is not None:
        return _generate_archive_entry_thumbnail(
            archive_entry, node_id, archive_service, thumb_service
        )

    path = registry.resolve(node_id)

    if path.is_dir():
        raise HTTPException(
            status_code=422,
            detail={
                "error": "ディレクトリのサムネイルは非対応です",
                "code": "NOT_SUPPORTED",
            },
        )

    if not path.exists():
        raise HTTPException(
            status_code=404,
            detail={"error": "ファイルが見つかりません", "code": "NOT_FOUND"},
        )

    # アーカイブファイル → 先頭画像エントリのサムネイル
    if archive_service.is_supported(path):
        return _generate_archive_cover_thumbnail(
            path, node_id, archive_service, thumb_service
        )

    # 画像以外 (PDF/動画等) はサムネイル非対応
    ext = path.suffix.lower()
    if ext not in IMAGE_EXTENSIONS:
        raise HTTPException(
            status_code=422,
            detail={
                "error": "サムネイル非対応のファイル形式です",
                "code": "NOT_SUPPORTED",
            },
        )

    # 通常の画像ファイル → サムネイル
    st = path.stat()
    etag = _compute_etag(st.st_mtime_ns, node_id)
    cache_key = ThumbnailService.make_cache_key(node_id, st.st_mtime_ns)
    thumb_bytes = thumb_service.get_or_generate_bytes_from_path(path, cache_key)
    return thumb_bytes, etag


def _generate_archive_entry_thumbnail(
    archive_entry: tuple[Path, str],
    node_id: str,
    archive_service: ArchiveService,
    thumb_service: ThumbnailService,
) -> tuple[bytes, str]:
    """アーカイブ内の画像エントリのサムネイル bytes と ETag を返す."""
    archive_path, entry_name = archive_entry
    st = archive_path.stat()
    etag = _compute_etag(st.st_mtime_ns, node_id)
    cache_key = ThumbnailService.make_cache_key(node_id, st.st_mtime_ns)
    source_bytes = archive_service.extract_entry(archive_path, entry_name)
    thumb_bytes = thumb_service.get_or_generate_bytes(source_bytes, cache_key)
    return thumb_bytes, etag


def _generate_archive_cover_thumbnail(
    archive_path: Path,
    node_id: str,
    archive_service: ArchiveService,
    thumb_service: ThumbnailService,
) -> tuple[bytes, str]:
    """アーカイブファイル自体のサムネイル (先頭画像エントリ) の bytes と ETag を返す."""
    st = archive_path.stat()
    etag = _compute_etag(st.st_mtime_ns, node_id)

    # アーカイブ内の先頭画像エントリを探す
    try:
        entries = archive_service.list_entries(archive_path)
    except Exception as exc:
        logger.warning("アーカイブ読み取り失敗: %s (%s)", archive_path, exc)
        raise HTTPException(
            status_code=422,
            detail={
                "error": f"アーカイブを読み取れません: {exc}",
                "code": "INVALID_ARCHIVE",
            },
        ) from exc

    first_image = None
    for entry in entries:
        name = entry.name
        dot_idx = name.rfind(".")
        ext = name[dot_idx:].lower() if dot_idx > 0 else ""
        if ext in IMAGE_EXTENSIONS:
            first_image = entry
            break

    if first_image is None:
        raise HTTPException(
            status_code=404,
            detail={
                "error": "アーカイブ内に画像が見つかりません",
                "code": "NO_IMAGE",
            },
        )

    cache_key = ThumbnailService.make_cache_key(node_id, st.st_mtime_ns)
    source_bytes = archive_service.extract_entry(archive_path, first_image.name)
    thumb_bytes = thumb_service.get_or_generate_bytes(source_bytes, cache_key)
    return thumb_bytes, etag


@router.get("/thumbnail/{node_id}")
async def serve_thumbnail(
    node_id: str,
    request: Request,
    registry: NodeRegistry = Depends(get_node_registry),
    archive_service: ArchiveService = Depends(get_archive_service),
    thumb_service: ThumbnailService = Depends(get_thumbnail_service),
) -> Response:
    """画像またはアーカイブのサムネイルを返す.

    - 画像ファイル → pyvips でリサイズ
    - アーカイブ → 先頭の画像エントリを抽出してサムネイル化
    - ディレクトリ → 422 (フロントは preview_node_ids 経由で個別画像を要求)
    """
    # ETag ベースの条件付きリクエスト (事前チェック)
    # _generate_thumbnail_bytes 内で ETag を計算するが、
    # 304 判定のためにここで先に ETag を取得する必要がある
    # → 生成後に判定する方式に統一 (stat は生成関数内で 1 回のみ)

    def _generate() -> tuple[bytes, str]:
        return _generate_thumbnail_bytes(
            node_id, registry, archive_service, thumb_service
        )

    try:
        thumb_bytes, etag = await run_in_threadpool(_generate)
    except pyvips.Error:
        raise HTTPException(
            status_code=422,
            detail={
                "error": "画像として認識できないデータです",
                "code": "INVALID_IMAGE",
            },
        ) from None

    # ETag 一致なら 304
    if_none_match = request.headers.get("if-none-match")
    if if_none_match and if_none_match.strip('"') == etag:
        return Response(status_code=304, headers={"ETag": f'"{etag}"'})

    return Response(
        content=thumb_bytes,
        media_type="image/jpeg",
        headers={
            "ETag": f'"{etag}"',
            "Cache-Control": _cache_control(request),
        },
    )
