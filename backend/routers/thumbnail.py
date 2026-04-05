"""サムネイル配信 API.

GET /api/thumbnail/{node_id} — 画像/アーカイブ/PDF/動画のサムネイルを返す
POST /api/thumbnails/batch — 複数サムネイルを一括取得
- image → pyvips でリサイズ
- archive → 先頭画像エントリをサムネイル化
- pdf → pyvips (poppler 経由) で先頭ページをサムネイル化
- video → ffmpeg フレーム抽出 + pyvips リサイズ
- ディレクトリ / 画像なし → 422 / 404
"""

import asyncio
import base64
import hashlib
import logging
from pathlib import Path

import pyvips
from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import Response
from pydantic import BaseModel, Field
from starlette.concurrency import run_in_threadpool

from backend.errors import NodeNotFoundError
from backend.services.archive_service import ArchiveService
from backend.services.extensions import (
    IMAGE_EXTENSIONS,
    PDF_EXTENSIONS,
    VIDEO_EXTENSIONS,
)
from backend.services.node_registry import NodeRegistry
from backend.services.thumbnail_service import ThumbnailService
from backend.services.video_converter import VideoConverter

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


def get_video_converter() -> VideoConverter:
    """VideoConverter の DI スタブ."""
    msg = "VideoConverter が DI で設定されていません"
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
    video_converter: VideoConverter | None = None,
) -> tuple[bytes, str]:
    """node_id からサムネイル JPEG バイト列と ETag を生成する.

    4 つのケースを処理:
    - アーカイブ内エントリ → 抽出してサムネイル化
    - アーカイブファイル自体 → 先頭画像エントリのサムネイル
    - PDF → pyvips (poppler 経由) で先頭ページをサムネイル化
    - 動画 → ffmpeg でフレーム抽出 + pyvips リサイズ
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

    ext = path.suffix.lower()

    # PDF → pyvips (poppler 経由) で先頭ページをサムネイル化
    if ext in PDF_EXTENSIONS:
        st = path.stat()
        etag = _compute_etag(st.st_mtime_ns, node_id)
        cache_key = ThumbnailService.make_cache_key(node_id, st.st_mtime_ns)
        thumb_bytes = thumb_service.get_or_generate_bytes_from_path(path, cache_key)
        return thumb_bytes, etag

    # 動画 → ffmpeg でフレーム抽出 + pyvips リサイズ
    if ext in VIDEO_EXTENSIONS:
        if video_converter is None:
            raise HTTPException(
                status_code=422,
                detail={
                    "error": "動画サムネイルが利用できません",
                    "code": "NOT_SUPPORTED",
                },
            )
        return _generate_video_thumbnail(path, node_id, thumb_service, video_converter)

    # 画像以外はサムネイル非対応
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


def _generate_video_thumbnail(
    path: Path,
    node_id: str,
    thumb_service: ThumbnailService,
    video_converter: VideoConverter,
) -> tuple[bytes, str]:
    """動画のサムネイルを生成する (ffmpeg フレーム抽出 + pyvips リサイズ)."""
    st = path.stat()
    etag = _compute_etag(st.st_mtime_ns, node_id)
    cache_key = ThumbnailService.make_cache_key(node_id, st.st_mtime_ns)

    # キャッシュヒット
    cached_bytes = thumb_service.get_cached_bytes(cache_key)
    if cached_bytes is not None:
        return cached_bytes, etag

    # ffmpeg でフレーム抽出
    frame_bytes = video_converter.extract_frame(path)
    if frame_bytes is None:
        raise HTTPException(
            status_code=422,
            detail={
                "error": "動画のフレーム抽出に失敗しました",
                "code": "FRAME_EXTRACT_FAILED",
            },
        )

    # pyvips でリサイズ + キャッシュ
    thumb_bytes = thumb_service.get_or_generate_bytes(frame_bytes, cache_key)
    return thumb_bytes, etag


@router.get("/thumbnail/{node_id}")
async def serve_thumbnail(
    node_id: str,
    request: Request,
    registry: NodeRegistry = Depends(get_node_registry),
    archive_service: ArchiveService = Depends(get_archive_service),
    thumb_service: ThumbnailService = Depends(get_thumbnail_service),
    video_converter: VideoConverter = Depends(get_video_converter),
) -> Response:
    """画像/アーカイブ/PDF/動画のサムネイルを返す.

    - 画像ファイル → pyvips でリサイズ
    - アーカイブ → 先頭の画像エントリを抽出してサムネイル化
    - PDF → pyvips (poppler 経由) で先頭ページをサムネイル化
    - 動画 → ffmpeg でフレーム抽出 + pyvips リサイズ
    - ディレクトリ → 422 (フロントは preview_node_ids 経由で個別サムネイルを要求)
    """

    def _generate() -> tuple[bytes, str]:
        return _generate_thumbnail_bytes(
            node_id, registry, archive_service, thumb_service, video_converter
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


# --- バッチサムネイル API ---


class ThumbnailBatchRequest(BaseModel):
    """バッチサムネイルのリクエストボディ."""

    node_ids: list[str] = Field(max_length=50)


class ThumbnailResult(BaseModel):
    """バッチサムネイルの個別結果."""

    data: str | None = None
    etag: str | None = None
    error: str | None = None
    code: str | None = None


class ThumbnailBatchResponse(BaseModel):
    """バッチサムネイルのレスポンス."""

    thumbnails: dict[str, ThumbnailResult]


async def _generate_one_thumbnail(
    node_id: str,
    registry: NodeRegistry,
    archive_service: ArchiveService,
    thumb_service: ThumbnailService,
    video_converter: VideoConverter | None = None,
) -> ThumbnailResult:
    """1 つの node_id のサムネイルを生成し ThumbnailResult を返す."""
    try:
        thumb_bytes, etag = await run_in_threadpool(
            _generate_thumbnail_bytes,
            node_id,
            registry,
            archive_service,
            thumb_service,
            video_converter,
        )
        return ThumbnailResult(
            data=base64.b64encode(thumb_bytes).decode(),
            etag=f'"{etag}"',
        )
    except NodeNotFoundError:
        return ThumbnailResult(
            error="ファイルが見つかりません",
            code="NOT_FOUND",
        )
    except HTTPException as exc:
        detail: dict[str, str] = exc.detail if isinstance(exc.detail, dict) else {}
        return ThumbnailResult(
            error=detail.get("error", str(exc.detail)),
            code=detail.get("code", "UNKNOWN"),
        )
    except pyvips.Error:
        return ThumbnailResult(
            error="画像として認識できないデータです",
            code="INVALID_IMAGE",
        )


async def _generate_archive_group_thumbnails(
    archive_path: Path,
    entries: list[tuple[str, str]],
    archive_service: ArchiveService,
    thumb_service: ThumbnailService,
) -> dict[str, ThumbnailResult]:
    """同一アーカイブの複数エントリを一括展開してサムネイル生成する.

    entries: [(node_id, entry_name), ...]
    アーカイブを 1 回だけ開き、extract_entries_batch で一括展開する。
    """

    def _generate() -> dict[str, ThumbnailResult]:
        st = archive_path.stat()
        entry_names = [name for _, name in entries]
        extracted = archive_service.extract_entries_batch(archive_path, entry_names)

        results: dict[str, ThumbnailResult] = {}
        for node_id, entry_name in entries:
            etag = _compute_etag(st.st_mtime_ns, node_id)
            source_bytes = extracted.get(entry_name)
            if source_bytes is None:
                results[node_id] = ThumbnailResult(
                    error="アーカイブエントリが見つかりません",
                    code="NOT_FOUND",
                )
                continue
            try:
                cache_key = ThumbnailService.make_cache_key(node_id, st.st_mtime_ns)
                thumb_bytes = thumb_service.get_or_generate_bytes(
                    source_bytes, cache_key
                )
                results[node_id] = ThumbnailResult(
                    data=base64.b64encode(thumb_bytes).decode(),
                    etag=f'"{etag}"',
                )
            except pyvips.Error:
                results[node_id] = ThumbnailResult(
                    error="画像として認識できないデータです",
                    code="INVALID_IMAGE",
                )
        return results

    try:
        return await run_in_threadpool(_generate)
    except Exception as exc:
        # アーカイブ全体のエラー → 全エントリにエラーを返す
        error_msg = f"アーカイブ読み取り失敗: {exc}"
        return {
            nid: ThumbnailResult(error=error_msg, code="INVALID_ARCHIVE")
            for nid, _ in entries
        }


@router.post("/thumbnails/batch", response_model=ThumbnailBatchResponse)
async def serve_thumbnails_batch(
    body: ThumbnailBatchRequest,
    registry: NodeRegistry = Depends(get_node_registry),
    archive_service: ArchiveService = Depends(get_archive_service),
    thumb_service: ThumbnailService = Depends(get_thumbnail_service),
    video_converter: VideoConverter = Depends(get_video_converter),
) -> ThumbnailBatchResponse:
    """複数 node_id のサムネイルを一括取得する.

    - 最大 50 件の node_ids を受け付ける
    - 各 node_id ごとに成功/エラーを返す (全体は常に 200)
    - アーカイブエントリは archive_path ごとにグルーピングして一括展開
    - 非アーカイブエントリは個別に並列処理
    """
    # 重複排除 (順序保持)
    unique_ids = list(dict.fromkeys(body.node_ids))

    # アーカイブエントリをグルーピング、通常エントリは個別処理
    archive_groups: dict[Path, list[tuple[str, str]]] = {}
    regular_ids: list[str] = []

    for nid in unique_ids:
        entry = registry.resolve_archive_entry(nid)
        if entry is not None:
            archive_path, entry_name = entry
            archive_groups.setdefault(archive_path, []).append((nid, entry_name))
        else:
            regular_ids.append(nid)

    # 非アーカイブエントリの個別処理タスク
    regular_tasks = [
        _generate_one_thumbnail(
            nid, registry, archive_service, thumb_service, video_converter
        )
        for nid in regular_ids
    ]

    # アーカイブグループの一括処理タスク
    archive_tasks = [
        _generate_archive_group_thumbnails(
            arc_path, entries, archive_service, thumb_service
        )
        for arc_path, entries in archive_groups.items()
    ]

    # 並列実行: 通常エントリとアーカイブグループを別々に gather
    regular_results = await asyncio.gather(*regular_tasks)
    archive_results = await asyncio.gather(*archive_tasks)

    # 結果を統合
    thumbnails: dict[str, ThumbnailResult] = {}
    for nid, result in zip(regular_ids, regular_results, strict=True):
        thumbnails[nid] = result
    for group_result in archive_results:
        thumbnails.update(group_result)

    return ThumbnailBatchResponse(thumbnails=thumbnails)
