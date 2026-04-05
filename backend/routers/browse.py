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
from pathlib import Path

from fastapi import APIRouter, Depends, HTTPException, Query, Request
from fastapi.responses import Response
from pydantic import BaseModel
from starlette.concurrency import run_in_threadpool

from backend.services.archive_service import ArchiveService
from backend.services.browse_cursor import (
    MAX_LIMIT,
    SortOrder,
    paginate,
)
from backend.services.dir_index import DirIndex
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


def get_dir_index() -> DirIndex | None:
    """DirIndex の DI スタブ (未設定時は None)."""
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


def _dir_index_to_entries(
    rows: list[dict[str, object]],
    parent_path_real: Path,
    registry: NodeRegistry,
    dir_index: DirIndex | None = None,
    parent_path_key: str = "",
) -> list[EntryMeta]:
    """DirIndex の SQL 結果を EntryMeta に変換する.

    - 各行のパスを validate_existing() で検証 (stale 行を除外)
    - register_resolved() で node_id を登録
    - ディレクトリエントリは child_count + preview_node_ids を DirIndex から取得
    """
    import mimetypes
    from typing import cast

    from backend.services.extensions import MIME_MAP

    entries: list[EntryMeta] = []
    dir_count = 0
    for row in rows:
        name = str(row["name"])
        kind = str(row["kind"])
        mtime_ns = cast(int, row["mtime_ns"]) if row["mtime_ns"] else 0
        child_path = parent_path_real / name

        # stale/削除済みファイルをスキップ
        if not child_path.exists():
            continue

        resolved = child_path.resolve()
        node_id = registry.register_resolved(resolved)

        if kind == "directory":
            child_count: int | None = None
            preview_ids: list[str] | None = None
            dir_count += 1
            # DirIndex から child_count と preview_node_ids を取得
            if dir_index is not None and dir_count <= 100:
                child_key = f"{parent_path_key}/{name}" if parent_path_key else name
                child_count = dir_index.child_count(child_key)
                previews = dir_index.preview_entries(child_key, limit=3)
                if previews:
                    ids: list[str] = []
                    for prev_row in previews:
                        prev_child = resolved / str(prev_row["name"])
                        if prev_child.exists():
                            prev_resolved = prev_child.resolve()
                            ids.append(registry.register_resolved(prev_resolved))
                    preview_ids = ids if ids else None
            entries.append(
                EntryMeta(
                    node_id=node_id,
                    name=name,
                    kind=kind,  # type: ignore[arg-type]
                    child_count=child_count,
                    modified_at=mtime_ns / 1e9 if mtime_ns else None,
                    preview_node_ids=preview_ids,
                )
            )
        else:
            dot_idx = name.rfind(".")
            ext = name[dot_idx:].lower() if dot_idx > 0 else ""
            mime = MIME_MAP.get(ext) or mimetypes.guess_type(name)[0]
            entries.append(
                EntryMeta(
                    node_id=node_id,
                    name=name,
                    kind=kind,  # type: ignore[arg-type]
                    size_bytes=(
                        cast(int, row["size_bytes"]) if row.get("size_bytes") else None
                    ),
                    mime_type=mime,
                    modified_at=mtime_ns / 1e9 if mtime_ns else None,
                )
            )
    return entries


@router.get("/browse/{node_id}", response_model=BrowseResponse)
async def browse_directory(
    node_id: str,
    request: Request,
    registry: NodeRegistry = Depends(get_node_registry),
    archive_service: ArchiveService = Depends(get_archive_service),
    warmer: ThumbnailWarmer | None = Depends(get_thumbnail_warmer),
    dir_index: DirIndex | None = Depends(get_dir_index),
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

    # DirIndex パス: ready + limit 指定時は SQL クエリで高速化
    # DirIndex が利用可能なら scandir/stat を完全にスキップ
    _used_dir_index = False
    if dir_index is not None and dir_index.is_ready and limit is not None:
        _dir_index_result = await _try_dir_index_query(
            dir_index, registry, path, sort, limit, cursor
        )
        if _dir_index_result is not None:
            page_entries, next_cursor_val, total_count, etag = _dir_index_result
            _used_dir_index = True

    # フォールバック: DirIndex が使えない場合は従来パス
    # name ソート + limit 指定時: DirEntry レベルでページ分だけ stat (遅延評価)
    # date ソート時: 全件 stat が必要 (stat 結果がソートキー)
    _is_name_sort = sort in (SortOrder.NAME_ASC, SortOrder.NAME_DESC)
    if not _used_dir_index and _is_name_sort and limit is not None:
        # カーソルから前ページ末尾の node_id を取得
        cursor_node_id = _extract_cursor_node_id(cursor, sort)

        # list_directory_page は DirEntry レベルでソート + ページ分のみ stat
        # name-desc 時は reverse=True でディレクトリ優先 + 名前降順
        _reverse = sort == SortOrder.NAME_DESC
        all_page_entries, total_count = await run_in_threadpool(
            registry.list_directory_page,
            path,
            limit + 1,
            cursor_node_id,
            reverse=_reverse,
        )

        # limit + 1 件取得 → 超過分で next_cursor 有無を判定
        has_next = len(all_page_entries) > limit
        page_entries = all_page_entries[:limit]

        # per-page ETag (ページ内エントリのみ)
        etag = _compute_etag(page_entries)
        if request.headers.get("if-none-match") == etag:
            return Response(
                status_code=304,
                headers={"ETag": etag, "Cache-Control": "private, no-cache"},
            )

        next_cursor_val = None
        if has_next and page_entries:
            from backend.services.browse_cursor import encode_cursor

            next_cursor_val = encode_cursor(sort, page_entries[-1], etag)
    elif not _used_dir_index:
        # date ソート or limit なし: 従来の全件取得 + ページネーション
        entries = await run_in_threadpool(registry.list_directory, path)

        # per-page ETag: limit 指定時はページ適用後、なし時はソート後の全件
        if limit is not None:
            page_entries, next_cursor_val, total_count = _apply_pagination(
                entries, sort, limit, cursor, ""
            )
            etag = _compute_etag(page_entries)
        else:
            # limit なしでもソートは適用 (name-desc, date-* 等)
            from backend.services.browse_cursor import sort_entries

            page_entries = sort_entries(entries, sort)
            total_count = len(entries)
            next_cursor_val = None
            etag = _compute_etag(page_entries)

        if request.headers.get("if-none-match") == etag:
            return Response(
                status_code=304,
                headers={"ETag": etag, "Cache-Control": "private, no-cache"},
            )

        # limit 指定時は etag を更新してカーソルに反映
        if limit is not None and next_cursor_val:
            from backend.services.browse_cursor import encode_cursor

            next_cursor_val = encode_cursor(sort, page_entries[-1], etag)

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
        next_cursor=next_cursor_val,
        total_count=total_count if limit is not None else None,
    )
    return Response(
        content=response.model_dump_json(),
        media_type="application/json",
        headers={"ETag": etag, "Cache-Control": "private, no-cache"},
    )


async def _try_dir_index_query(
    dir_index: DirIndex,
    registry: NodeRegistry,
    path: Path,
    sort: SortOrder,
    limit: int,
    cursor: str | None,
) -> tuple[list[EntryMeta], str | None, int, str] | None:
    """DirIndex を使って SQL クエリでページを取得する.

    DirIndex が stale (ディレクトリ mtime 不一致) の場合は None を返す。
    Returns: (page_entries, next_cursor, total_count, etag) or None
    """
    from backend.services.browse_cursor import encode_cursor

    # ディレクトリの実 mtime を確認
    try:
        dir_stat = path.stat()
    except OSError:
        return None

    # DirIndex に対応する parent_path を構築
    root = registry.path_security.find_root_for(path)
    if root is None:
        return None

    # mount_id は NodeRegistry から取得 (mount_id_map の逆引き)
    mount_id = ""
    for mid, mroot in registry.mount_id_map.items():
        if mroot == root:
            mount_id = mid
            break

    rel = str(path.relative_to(root))
    parent_path_key = f"{mount_id}/{rel}" if rel != "." else mount_id

    # DirIndex の mtime と比較 → 不一致なら stale
    cached_mtime = dir_index.get_dir_mtime(parent_path_key)
    if cached_mtime is None or cached_mtime != dir_stat.st_mtime_ns:
        return None

    # カーソルから sort_key を取得
    cursor_sort_key = None
    if cursor:
        try:
            from backend.services.browse_cursor import decode_cursor

            cursor_data = decode_cursor(cursor, sort)
            # cursor には node_id が入っているが、DirIndex は sort_key ベース
            # → DirIndex から該当 node_id の sort_key を取得
            cursor_name = str(cursor_data.get("n", ""))
            from backend.services.dir_index import encode_sort_key

            cursor_sort_key = encode_sort_key(cursor_name)
        except ValueError:
            return None  # カーソル不正 → フォールバック

    # SQL クエリ実行
    rows = await run_in_threadpool(
        dir_index.query_page,
        parent_path_key,
        sort.value,
        limit + 1,
        cursor_sort_key,
    )

    # EntryMeta に変換 (path_security 検証付き、ディレクトリは preview_node_ids 付き)
    all_entries = await run_in_threadpool(
        _dir_index_to_entries, rows, path, registry, dir_index, parent_path_key
    )

    total_count = await run_in_threadpool(dir_index.total_count, parent_path_key)

    # ページネーション
    has_next = len(all_entries) > limit
    page_entries = all_entries[:limit]

    etag = _compute_etag(page_entries)

    next_cursor_val = None
    if has_next and page_entries:
        next_cursor_val = encode_cursor(sort, page_entries[-1], etag)

    return page_entries, next_cursor_val, total_count, etag


def _extract_cursor_node_id(cursor: str | None, sort: SortOrder) -> str | None:
    """カーソルから前ページ末尾の node_id を抽出する."""
    if cursor is None:
        return None
    from backend.services.browse_cursor import decode_cursor

    try:
        data = decode_cursor(cursor, sort)
        return str(data.get("id", ""))
    except ValueError as exc:
        raise HTTPException(
            status_code=400,
            detail={"error": str(exc), "code": "INVALID_CURSOR"},
        ) from exc


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


# --- first-viewable エンドポイント ---


class FirstViewableResponse(BaseModel):
    """first-viewable API のレスポンス."""

    entry: EntryMeta | None = None
    parent_node_id: str | None = None


@router.get(
    "/browse/{node_id}/first-viewable",
    response_model=FirstViewableResponse,
)
async def first_viewable(
    node_id: str,
    registry: NodeRegistry = Depends(get_node_registry),
    dir_index: DirIndex | None = Depends(get_dir_index),
    sort: SortOrder = Query(SortOrder.NAME_ASC),
) -> FirstViewableResponse:
    """ディレクトリ内の最初の閲覧対象を再帰的に探索する.

    優先順位: archive > pdf > image > directory (再帰降下)
    DirIndex ready 時は SQL クエリ、未 ready 時は scandir。
    最大 10 レベルまで再帰。
    """
    max_depth = 10
    current_id = node_id

    for _ in range(max_depth):
        current_path = registry.resolve(current_id)
        if not current_path.is_dir():
            break

        # DirIndex パス: SQL で最初の閲覧対象を探す
        entry_meta = await _find_first_viewable_from_index(
            dir_index, registry, current_path, sort
        )

        if entry_meta is None:
            # フォールバック: scandir
            entries = await run_in_threadpool(registry.list_directory, current_path)
            from backend.services.browse_cursor import sort_entries

            sorted_entries = sort_entries(entries, sort)
            entry_meta = _select_first_viewable(sorted_entries)

        if entry_meta is None:
            return FirstViewableResponse()

        if entry_meta.kind in ("image", "archive", "pdf"):
            return FirstViewableResponse(
                entry=entry_meta,
                parent_node_id=current_id,
            )

        # directory → 再帰降下
        current_id = entry_meta.node_id

    return FirstViewableResponse()


async def _find_first_viewable_from_index(
    dir_index: DirIndex | None,
    registry: NodeRegistry,
    directory: Path,
    sort: SortOrder,
) -> EntryMeta | None:
    """DirIndex から最初の閲覧対象を探す."""
    if dir_index is None or not dir_index.is_ready:
        return None

    root = registry.path_security.find_root_for(directory)
    if root is None:
        return None

    mount_id = ""
    for mid, mroot in registry.mount_id_map.items():
        if mroot == root:
            mount_id = mid
            break

    rel = str(directory.relative_to(root))
    parent_key = f"{mount_id}/{rel}" if rel != "." else mount_id

    # archive > pdf > image の優先順位で SQL クエリ
    for kind in ("archive", "pdf", "image"):
        rows = await run_in_threadpool(dir_index.query_page, parent_key, sort.value, 1)
        # kind フィルタ付きクエリ (DirIndex に追加予定。暫定: 全件取得後フィルタ)
        filtered = [r for r in rows if r["kind"] == kind]
        if filtered:
            entries = await run_in_threadpool(
                _dir_index_to_entries, filtered[:1], directory, registry
            )
            if entries:
                return entries[0]

    # 閲覧対象なし → directory を探す (再帰降下用)
    rows = await run_in_threadpool(dir_index.query_page, parent_key, sort.value, 1)
    dir_rows = [r for r in rows if r["kind"] == "directory"]
    if dir_rows:
        entries = await run_in_threadpool(
            _dir_index_to_entries, dir_rows[:1], directory, registry
        )
        if entries:
            return entries[0]

    return None


def _select_first_viewable(entries: list[EntryMeta]) -> EntryMeta | None:
    """ソート済みエントリから最初の閲覧対象を選ぶ.

    優先順位: archive > pdf > image > directory (再帰降下用)
    """
    for kind in ("archive", "pdf", "image"):
        for e in entries:
            if e.kind == kind:
                return e
    # 閲覧対象なし → directory を探す
    for e in entries:
        if e.kind == "directory":
            return e
    return None


# --- sibling エンドポイント ---


class SiblingResponse(BaseModel):
    """sibling API のレスポンス."""

    entry: EntryMeta | None = None


@router.get(
    "/browse/{parent_node_id}/sibling",
    response_model=SiblingResponse,
)
async def find_sibling(
    parent_node_id: str,
    current: str = Query(..., description="現在のエントリの node_id"),
    direction: str = Query(..., description="next or prev"),
    registry: NodeRegistry = Depends(get_node_registry),
    dir_index: DirIndex | None = Depends(get_dir_index),
    sort: SortOrder = Query(SortOrder.NAME_ASC),
) -> SiblingResponse:
    """次または前の兄弟セット (directory/archive/pdf) を返す.

    DirIndex ready 時は SQL クエリで 1 エントリのみ取得。
    """
    parent_path = registry.resolve(parent_node_id)
    if not parent_path.is_dir():
        raise HTTPException(
            status_code=422,
            detail={
                "error": "親がディレクトリではありません",
                "code": "NOT_A_DIRECTORY",
            },
        )

    # 現在エントリのパスを取得
    current_path = registry.resolve(current)
    current_name = current_path.name

    # DirIndex パス
    if dir_index is not None and dir_index.is_ready:
        entry = await _find_sibling_from_index(
            dir_index, registry, parent_path, current_name, direction, sort
        )
        if entry is not None:
            return SiblingResponse(entry=entry)

    # フォールバック: 全件取得して検索
    entries = await run_in_threadpool(registry.list_directory, parent_path)
    from backend.services.browse_cursor import sort_entries

    sorted_entries = sort_entries(entries, sort)
    candidates = [
        e for e in sorted_entries if e.kind in ("directory", "archive", "pdf")
    ]
    current_idx = next(
        (i for i, e in enumerate(candidates) if e.node_id == current), -1
    )
    if current_idx < 0:
        return SiblingResponse()

    if direction == "next" and current_idx + 1 < len(candidates):
        return SiblingResponse(entry=candidates[current_idx + 1])
    if direction == "prev" and current_idx > 0:
        return SiblingResponse(entry=candidates[current_idx - 1])

    return SiblingResponse()


async def _find_sibling_from_index(
    dir_index: DirIndex,
    registry: NodeRegistry,
    parent_path: Path,
    current_name: str,
    direction: str,
    sort: SortOrder,
) -> EntryMeta | None:
    """DirIndex から次/前の兄弟セットを探す."""
    root = registry.path_security.find_root_for(parent_path)
    if root is None:
        return None

    mount_id = ""
    for mid, mroot in registry.mount_id_map.items():
        if mroot == root:
            mount_id = mid
            break

    rel = str(parent_path.relative_to(root))
    parent_key = f"{mount_id}/{rel}" if rel != "." else mount_id

    from backend.services.dir_index import encode_sort_key

    current_sort_key = encode_sort_key(current_name)

    # 次/前の兄弟 (kind=directory/archive/pdf) を SQL で取得
    conn = dir_index._connect()
    try:
        if direction == "next":
            rows = conn.execute(
                """
                SELECT * FROM dir_entries
                WHERE parent_path = ?
                  AND kind IN ('directory', 'archive', 'pdf')
                  AND sort_key > ?
                ORDER BY sort_key ASC
                LIMIT 1
                """,
                (parent_key, current_sort_key),
            ).fetchall()
        else:
            rows = conn.execute(
                """
                SELECT * FROM dir_entries
                WHERE parent_path = ?
                  AND kind IN ('directory', 'archive', 'pdf')
                  AND sort_key < ?
                ORDER BY sort_key DESC
                LIMIT 1
                """,
                (parent_key, current_sort_key),
            ).fetchall()
    finally:
        conn.close()

    if not rows:
        return None

    entries = await run_in_threadpool(
        _dir_index_to_entries, [dict(rows[0])], parent_path, registry
    )
    return entries[0] if entries else None
