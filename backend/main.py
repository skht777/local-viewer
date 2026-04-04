"""Local Content Viewer -- FastAPI entry point."""

from __future__ import annotations

import asyncio
import logging
from collections.abc import AsyncGenerator
from contextlib import asynccontextmanager
from pathlib import Path as FilePath
from typing import TYPE_CHECKING

from fastapi import FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.middleware.gzip import GZipMiddleware
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles
from starlette.datastructures import MutableHeaders
from starlette.types import ASGIApp, Message, Receive, Scope, Send

from backend.config import init_settings
from backend.errors import (
    NodeNotFoundError,
    PathSecurityError,
    archive_password_error_handler,
    archive_security_error_handler,
    node_not_found_error_handler,
    path_security_error_handler,
)
from backend.routers import browse, file, mounts, search, thumbnail
from backend.services.archive_security import (
    ArchivePasswordError,
    ArchiveSecurityError,
)
from backend.services.archive_service import ArchiveService
from backend.services.node_registry import NodeRegistry
from backend.services.path_security import PathSecurity
from backend.services.temp_file_cache import TempFileCache
from backend.services.thumbnail_service import ThumbnailService
from backend.services.video_converter import VideoConverter

if TYPE_CHECKING:
    from backend.services.file_watcher import FileWatcher
    from backend.services.indexer import Indexer

logger = logging.getLogger(__name__)

# サービスインスタンス (lifespan で初期化)
_node_registry: NodeRegistry | None = None
_archive_service: ArchiveService | None = None
_temp_file_cache: TempFileCache | None = None
_video_converter: VideoConverter | None = None
_thumbnail_service: ThumbnailService | None = None
_indexer: Indexer | None = None
_file_watcher: FileWatcher | None = None


def get_node_registry() -> NodeRegistry:
    """NodeRegistry の DI 用依存関数."""
    if _node_registry is None:
        msg = "NodeRegistry が初期化されていません"
        raise RuntimeError(msg)
    return _node_registry


def get_archive_service() -> ArchiveService:
    """ArchiveService の DI 用依存関数."""
    if _archive_service is None:
        msg = "ArchiveService が初期化されていません"
        raise RuntimeError(msg)
    return _archive_service


def get_temp_file_cache() -> TempFileCache:
    """TempFileCache の DI 用依存関数."""
    if _temp_file_cache is None:
        msg = "TempFileCache が初期化されていません"
        raise RuntimeError(msg)
    return _temp_file_cache


def get_video_converter() -> VideoConverter:
    """VideoConverter の DI 用依存関数."""
    if _video_converter is None:
        msg = "VideoConverter が初期化されていません"
        raise RuntimeError(msg)
    return _video_converter


def get_thumbnail_service() -> ThumbnailService:
    """ThumbnailService の DI 用依存関数."""
    if _thumbnail_service is None:
        msg = "ThumbnailService が初期化されていません"
        raise RuntimeError(msg)
    return _thumbnail_service


def get_indexer() -> Indexer:
    """Indexer の DI 用依存関数."""
    if _indexer is None:
        msg = "Indexer が初期化されていません"
        raise RuntimeError(msg)
    return _indexer


def _get_path_security() -> PathSecurity:
    """PathSecurity の DI 用依存関数."""
    if _node_registry is None:
        msg = "NodeRegistry が初期化されていません"
        raise RuntimeError(msg)
    return _node_registry.path_security


@asynccontextmanager
async def lifespan(_app: FastAPI) -> AsyncGenerator[None]:
    """アプリケーションの起動/終了処理.

    起動時: Settings → PathSecurity → NodeRegistry → ArchiveService を初期化。
    """
    global _node_registry, _archive_service, _temp_file_cache
    global _video_converter, _thumbnail_service, _indexer, _file_watcher

    # アプリケーションロガーを uvicorn のハンドラに接続
    # uvicorn はルートロガーを設定しないため、backend.* の出力先がない
    app_logger = logging.getLogger("backend")
    if not app_logger.handlers:
        app_logger.addHandler(logging.getLogger("uvicorn").handlers[0])
        app_logger.setLevel(logging.INFO)

    # スレッドプール上限を制御
    # anyio デフォルト 40 トークンは GIL 制約下で CPU スラッシングを招くため、
    # CPU コア数と GIL の実効並列度を考慮して 12 に制限する
    import anyio

    anyio.to_thread.current_default_thread_limiter().total_tokens = 12

    settings = init_settings()

    # マウントポイント設定の読み込み
    from backend.services.mount_config import MountConfigService

    base_dir = settings.mount_base_dir
    mount_service = MountConfigService(FilePath(settings.mount_config_path), base_dir)
    mount_config = mount_service.load()

    if not mount_config.mounts:
        logger.warning(
            "マウントポイントが未登録です。"
            "manage_mounts.sh でマウントポイントを追加してください"
        )

    # マウントポイントからルートディレクトリリストを構築
    # マウント未登録の場合は base_dir をフォールバックルートに
    if mount_config.mounts:
        root_dirs = [m.resolve_path(base_dir) for m in mount_config.mounts]
    else:
        root_dirs = [base_dir.resolve()]
    mount_names = {m.resolve_path(base_dir): m.name for m in mount_config.mounts}
    path_security = PathSecurity(
        root_dirs, is_allow_symlinks=settings.is_allow_symlinks
    )
    _node_registry = NodeRegistry(
        path_security,
        archive_registry_max_entries=settings.archive_registry_max_entries,
        mount_names=mount_names,
    )
    # mount_id → root_dir マッピング (search 用)
    _node_registry.set_mount_id_map(
        {m.mount_id: m.resolve_path(base_dir) for m in mount_config.mounts}
    )

    # DirIndex 参照を NodeRegistry に設定 (child_count/preview 高速化用)
    # ※ _dir_index はこの後の初期化ブロックで作成される
    # → lifespan の最後で設定する

    # アーカイブサービス初期化
    from backend.services.archive_security import ArchiveEntryValidator

    validator = ArchiveEntryValidator(settings)
    _archive_service = ArchiveService(
        validator=validator,
        cache_max_bytes=settings.archive_cache_mb * 1024 * 1024,
    )

    # ディスクキャッシュ初期化 (動画等の大きなエントリ用)
    _temp_file_cache = TempFileCache(
        max_size_bytes=settings.archive_disk_cache_mb * 1024 * 1024,
    )

    # 動画変換サービス初期化
    _video_converter = VideoConverter(
        temp_cache=_temp_file_cache,
        timeout=settings.video_remux_timeout,
    )
    if _video_converter.is_available:
        logger.info("Video remux: FFmpeg available")
    else:
        logger.warning("Video remux: FFmpeg not found, MKV files will not be remuxed")

    # サムネイルサービス初期化
    _thumbnail_service = ThumbnailService(temp_cache=_temp_file_cache)

    # 起動時診断: 各アーカイブ形式の利用可否
    diag = _archive_service.get_diagnostics()
    for fmt, available in diag.items():
        level = logging.INFO if available else logging.WARNING
        status = "available" if available else "NOT available"
        logger.log(level, "Archive: %s support: %s", fmt, status)

    # Indexer + DirIndex 初期化 + バックグラウンドスキャン
    from backend.services.dir_index import DirIndex
    from backend.services.file_watcher import FileWatcher
    from backend.services.indexer import Indexer

    _indexer = Indexer(settings.index_db_path)
    _indexer.init_db()

    # DirIndex: ディレクトリリスティング専用インデックス (独立 DB)
    dir_index_path = settings.index_db_path.replace(".db", "-dir.db")
    _dir_index = DirIndex(dir_index_path)
    _dir_index.init_db()

    # NodeRegistry に DirIndex 参照を設定 (child_count/preview 高速化)
    _node_registry.set_dir_index(_dir_index)

    # DB に既存エントリがあれば incremental_scan、なければ full scan
    has_existing = _indexer.entry_count() > 0

    # Warm Start: マウント構成が一致すれば既存データで即座に検索を提供
    current_mount_ids = sorted(m.mount_id for m in mount_config.mounts)
    if has_existing and _indexer.check_mount_fingerprint(current_mount_ids):
        _indexer.mark_warm_start()
        _dir_index.mark_warm_start()
        logger.info("Warm Start: 既存インデックスで検索を有効化 (stale)")
    else:
        has_existing = False

    # マウント構成を DB に保存 (次回起動時の Warm Start 判定用)
    _indexer.save_mount_fingerprint(current_mount_ids)

    import threading as _threading

    # FileWatcher 準備 (スキャン完了後に起動)
    watcher_mounts = [
        (m.mount_id, m.resolve_path(base_dir)) for m in mount_config.mounts
    ]
    _file_watcher = FileWatcher(
        indexer=_indexer,
        path_security=path_security,
        mode=settings.watch_mode,
        poll_interval=settings.watch_poll_interval,
        mounts=watcher_mounts,
    )

    def _scan_then_watch(
        scan_fn: object,
        args: tuple[object, ...],
    ) -> None:
        """スキャン完了後に FileWatcher を起動する."""
        try:
            scan_fn(*args)  # type: ignore[operator]
        except Exception:
            logger.exception("スキャンスレッドで例外が発生しました")
            return
        try:
            if _file_watcher is not None:
                _file_watcher.start()
        except Exception:
            logger.exception("FileWatcher の起動に失敗しました")

    for mount in mount_config.mounts:
        root = mount.resolve_path(base_dir)
        if has_existing:
            scan_fn = _background_incremental_scan_sync
        else:
            scan_fn = _background_scan_sync
        scan_args = (_indexer, _dir_index, root, path_security, mount.mount_id)
        t = _threading.Thread(
            target=_scan_then_watch,
            args=(scan_fn, scan_args),
            daemon=False,
            name=f"index-scan-{mount.mount_id}",
        )
        t.start()

    # DI: routers のスタブを実インスタンスに差し替え
    _app.dependency_overrides[browse.get_node_registry] = get_node_registry
    _app.dependency_overrides[file.get_node_registry] = get_node_registry
    _app.dependency_overrides[browse.get_archive_service] = get_archive_service
    _app.dependency_overrides[file.get_archive_service] = get_archive_service
    _app.dependency_overrides[file.get_temp_file_cache] = get_temp_file_cache
    _app.dependency_overrides[file.get_video_converter] = get_video_converter
    _app.dependency_overrides[search.get_indexer] = get_indexer
    _app.dependency_overrides[search.get_node_registry] = get_node_registry
    _app.dependency_overrides[search.get_path_security] = _get_path_security
    _app.dependency_overrides[mounts.get_node_registry] = get_node_registry
    _app.dependency_overrides[thumbnail.get_node_registry] = get_node_registry
    _app.dependency_overrides[thumbnail.get_archive_service] = get_archive_service
    _app.dependency_overrides[thumbnail.get_thumbnail_service] = get_thumbnail_service

    # サムネイルプリウォーム初期化
    from backend.services.thumbnail_warmer import ThumbnailWarmer

    _thumbnail_warmer = ThumbnailWarmer(
        thumb_service=_thumbnail_service,
        archive_service=_archive_service,
        registry=_node_registry,
    )
    _app.dependency_overrides[browse.get_thumbnail_warmer] = lambda: _thumbnail_warmer

    # DirIndex の DI 登録
    _app.dependency_overrides[browse.get_dir_index] = lambda: _dir_index

    yield

    # シャットダウン
    if _file_watcher:
        _file_watcher.stop()
        _file_watcher = None
    _indexer = None
    _node_registry = None
    _archive_service = None
    _video_converter = None
    _thumbnail_service = None
    _temp_file_cache = None


def _log_task_exception(task: asyncio.Task[object]) -> None:
    """asyncio タスクの未処理例外をログに記録する."""
    if task.cancelled():
        logger.warning(
            "バックグラウンドタスクがキャンセルされました: %s", task.get_name()
        )
    elif task.exception():
        logger.error("バックグラウンドタスク例外: %s", task.exception())


def _background_scan_sync(
    indexer: Indexer,
    dir_index: object,
    root_dir: FilePath,
    path_security: PathSecurity,
    mount_id: str = "",
) -> None:
    """バックグラウンドスレッドで初回インデックススキャンを実行する."""
    from backend.services.dir_index import DirIndex

    di = dir_index if isinstance(dir_index, DirIndex) else None
    on_walk = di.ingest_walk_entry if di else None
    try:
        count = indexer.scan_directory(
            root_dir, path_security, mount_id, on_walk_entry=on_walk
        )
        if di:
            di.mark_full_scan_done()
            di.mark_ready()
        logger.info(
            "初回インデックス完了: %d エントリ (%s)",
            count,
            mount_id or "default",
        )
    except Exception:
        logger.exception("初回インデックススキャンに失敗しました")


def _background_incremental_scan_sync(
    indexer: Indexer,
    dir_index: object,
    root_dir: FilePath,
    path_security: PathSecurity,
    mount_id: str = "",
) -> None:
    """バックグラウンドスレッドで差分インデックススキャンを実行する."""
    from backend.services.dir_index import DirIndex

    di = dir_index if isinstance(dir_index, DirIndex) else None

    # DirIndex が空の場合は incremental scan の枝刈りで子エントリが渡されない
    # → Indexer は incremental、DirIndex は full scan で初期化
    di_needs_full = di is not None and not di.is_full_scan_done()
    on_walk = di.ingest_walk_entry if (di and not di_needs_full) else None
    try:
        added, updated, deleted = indexer.incremental_scan(
            root_dir, path_security, mount_id, on_walk_entry=on_walk
        )
        logger.info(
            "差分インデックス完了: +%d ~%d -%d (%s)",
            added,
            updated,
            deleted,
            mount_id or "default",
        )

        # DirIndex 初回構築: full scan で全ディレクトリの子エントリを取得
        if di_needs_full and di:
            logger.info("DirIndex 初回構築: full scan 開始 (%s)", mount_id)
            indexer.scan_directory(
                root_dir,
                path_security,
                mount_id,
                on_walk_entry=di.ingest_walk_entry,
            )
            di.mark_full_scan_done()
            logger.info("DirIndex 初回構築完了 (%s)", mount_id)

        if di:
            di.mark_ready()
    except Exception:
        logger.exception("差分インデックススキャンに失敗しました")


app = FastAPI(
    title="Local Content Viewer",
    version="0.1.0",
    lifespan=lifespan,
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:5173"],  # Vite dev server
    allow_methods=["GET"],
    allow_headers=["*"],
)

# バイナリレスポンス (画像/動画/PDF) の gzip 圧縮をスキップする ASGI ミドルウェア
# GZipMiddleware は Content-Encoding が設定済みのレスポンスをスキップする性質を利用し、
# バイナリ Content-Type に identity を設定して無駄な圧縮を防止する
_BINARY_CONTENT_PREFIXES = (
    "image/",
    "video/",
    "audio/",
    "application/pdf",
    "application/zip",
    "application/x-rar",
    "application/x-7z",
    "application/octet-stream",
)


class _SkipGzipForBinaryMiddleware:
    """バイナリレスポンスに identity を付与して gzip をバイパス."""

    def __init__(self, app: ASGIApp) -> None:
        self.app = app

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        if scope["type"] != "http":
            await self.app(scope, receive, send)
            return

        async def _send_with_identity(message: Message) -> None:
            if message["type"] == "http.response.start":
                headers = MutableHeaders(scope=message)
                content_type = headers.get("content-type", "")
                if any(content_type.startswith(p) for p in _BINARY_CONTENT_PREFIXES):
                    headers["content-encoding"] = "identity"
            await send(message)

        await self.app(scope, receive, _send_with_identity)


# 内側: バイナリレスポンスに identity を設定
app.add_middleware(_SkipGzipForBinaryMiddleware)
# 外側: JSON レスポンスを gzip 圧縮
# compresslevel=1: ローカル環境では帯域より CPU 優先
app.add_middleware(GZipMiddleware, minimum_size=500, compresslevel=1)

# 例外ハンドラ登録
app.add_exception_handler(PathSecurityError, path_security_error_handler)  # type: ignore[arg-type]
app.add_exception_handler(NodeNotFoundError, node_not_found_error_handler)  # type: ignore[arg-type]
app.add_exception_handler(ArchiveSecurityError, archive_security_error_handler)
app.add_exception_handler(ArchivePasswordError, archive_password_error_handler)

# ルーター登録
app.include_router(mounts.router)
app.include_router(browse.router)
app.include_router(file.router)
app.include_router(thumbnail.router)
app.include_router(search.router)


@app.get("/api/health")
async def health() -> dict[str, str]:
    """Health check endpoint."""
    return {"status": "ok"}


# --- 本番用: 静的ファイル配信 + SPA フォールバック ---
# Vite ビルド出力を配信。開発時は Vite dev server がプロキシで処理するため影響なし
_static_dir = FilePath(__file__).parent.parent / "static"

if _static_dir.exists():
    # ハッシュ付きアセット (JS/CSS) — immutable 長期キャッシュ
    # Vite はファイル名にコンテンツハッシュを含むため安全にキャッシュ可能
    app.mount(
        "/assets",
        StaticFiles(directory=_static_dir / "assets"),
        name="assets",
    )

    @app.middleware("http")
    async def assets_cache_control(request: Request, call_next):  # type: ignore[no-untyped-def]
        """Vite ハッシュ付きアセットに immutable キャッシュヘッダーを付与."""
        response = await call_next(request)
        if request.url.path.startswith("/assets/"):
            response.headers["Cache-Control"] = "public, max-age=31536000, immutable"
        return response

    # Service Worker ファイル — SPA フォールバックより先に登録
    # no-cache でブラウザが常に最新版をチェックする
    @app.get("/sw.js")
    async def serve_sw() -> FileResponse:
        """Service Worker スクリプトを配信する."""
        return FileResponse(
            _static_dir / "sw.js",
            media_type="application/javascript",
            headers={"Cache-Control": "no-cache"},
        )

    @app.get("/workbox-{rest:path}")
    async def serve_workbox(rest: str) -> FileResponse:
        """Workbox ランタイムスクリプトを配信する."""
        return FileResponse(
            _static_dir / f"workbox-{rest}",
            media_type="application/javascript",
            headers={"Cache-Control": "public, max-age=31536000, immutable"},
        )

    # SPA フォールバック — /api 以外の全パスで index.html を返す
    @app.get("/{full_path:path}")
    async def spa_fallback(full_path: str) -> FileResponse:
        """SPA のクライアントサイドルーティング対応."""
        return FileResponse(
            _static_dir / "index.html",
            headers={"Cache-Control": "no-cache"},
        )
