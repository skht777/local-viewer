"""Local Content Viewer -- FastAPI entry point."""

from __future__ import annotations

import asyncio
import logging
from collections.abc import AsyncGenerator
from contextlib import asynccontextmanager
from pathlib import Path as FilePath
from typing import TYPE_CHECKING

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.middleware.gzip import GZipMiddleware
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles
from starlette.concurrency import run_in_threadpool

from backend.config import init_settings
from backend.errors import (
    NodeNotFoundError,
    PathSecurityError,
    archive_password_error_handler,
    archive_security_error_handler,
    node_not_found_error_handler,
    path_security_error_handler,
)
from backend.routers import browse, file, mounts, search
from backend.services.archive_security import (
    ArchivePasswordError,
    ArchiveSecurityError,
)
from backend.services.archive_service import ArchiveService
from backend.services.node_registry import NodeRegistry
from backend.services.path_security import PathSecurity
from backend.services.temp_file_cache import TempFileCache
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
    global _video_converter, _indexer, _file_watcher

    # アプリケーションロガーを uvicorn のハンドラに接続
    # uvicorn はルートロガーを設定しないため、backend.* の出力先がない
    app_logger = logging.getLogger("backend")
    if not app_logger.handlers:
        app_logger.addHandler(logging.getLogger("uvicorn").handlers[0])
        app_logger.setLevel(logging.INFO)

    settings = init_settings()

    # マウントポイント設定の読み込み
    from backend.services.mount_config import MountConfigService

    base_dir = settings.mount_base_dir or settings.root_dir
    mount_service = MountConfigService(FilePath(settings.mount_config_path), base_dir)
    mount_config = mount_service.load()

    # mounts.json が空なら ROOT_DIR から自動マイグレーション
    if not mount_config.mounts:
        logger.info("mounts.json 未設定: ROOT_DIR からマイグレーション")
        mount_service.migrate_from_root_dir(settings.root_dir)
        mount_config = mount_service.load()

    # マウントポイントからルートディレクトリリストを構築
    root_dirs = [FilePath(m.path).resolve() for m in mount_config.mounts]
    mount_names = {FilePath(m.path).resolve(): m.name for m in mount_config.mounts}
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
        {m.mount_id: FilePath(m.path).resolve() for m in mount_config.mounts}
    )

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

    # 起動時診断: 各アーカイブ形式の利用可否
    diag = _archive_service.get_diagnostics()
    for fmt, available in diag.items():
        level = logging.INFO if available else logging.WARNING
        status = "available" if available else "NOT available"
        logger.log(level, "Archive: %s support: %s", fmt, status)

    # Indexer 初期化 + バックグラウンドスキャン
    from backend.services.file_watcher import FileWatcher
    from backend.services.indexer import Indexer

    _indexer = Indexer(settings.index_db_path)
    _indexer.init_db()

    # DB に既存エントリがあれば incremental_scan、なければ full scan
    has_existing = _indexer.entry_count() > 0

    for mount in mount_config.mounts:
        root = FilePath(mount.path).resolve()
        if has_existing:
            scan_task = asyncio.create_task(
                _background_incremental_scan(
                    _indexer, root, path_security, mount.mount_id
                )
            )
        else:
            scan_task = asyncio.create_task(
                _background_scan(_indexer, root, path_security, mount.mount_id)
            )
        scan_task.add_done_callback(lambda _: None)

    # FileWatcher 開始 (全マウントを一括監視)
    # PollingObserver.start() は初回スナップショットで同期的にディレクトリ全体を走査する
    # WSL2 (9p) 等の遅いファイルシステムで lifespan をブロックしないよう非同期で起動
    watcher_mounts = [
        (m.mount_id, FilePath(m.path).resolve()) for m in mount_config.mounts
    ]
    _file_watcher = FileWatcher(
        indexer=_indexer,
        path_security=path_security,
        mode=settings.watch_mode,
        poll_interval=settings.watch_poll_interval,
        mounts=watcher_mounts,
    )
    watcher_task = asyncio.create_task(run_in_threadpool(_file_watcher.start))
    watcher_task.add_done_callback(lambda _: None)  # GC 防止

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

    yield

    # シャットダウン
    if _file_watcher:
        _file_watcher.stop()
        _file_watcher = None
    _indexer = None
    _node_registry = None
    _archive_service = None
    _video_converter = None
    _temp_file_cache = None


async def _background_scan(
    indexer: Indexer,
    root_dir: FilePath,
    path_security: PathSecurity,
    mount_id: str = "",
) -> None:
    """バックグラウンドで初回インデックススキャンを実行する."""
    try:
        from starlette.concurrency import run_in_threadpool

        count = await run_in_threadpool(
            indexer.scan_directory, root_dir, path_security, mount_id
        )
        logger.info(
            "初回インデックス完了: %d エントリ (%s)",
            count,
            mount_id or "default",
        )
    except Exception:
        logger.exception("初回インデックススキャンに失敗しました")


async def _background_incremental_scan(
    indexer: Indexer,
    root_dir: FilePath,
    path_security: PathSecurity,
    mount_id: str = "",
) -> None:
    """バックグラウンドで差分インデックススキャンを実行する."""
    try:
        from starlette.concurrency import run_in_threadpool

        added, updated, deleted = await run_in_threadpool(
            indexer.incremental_scan, root_dir, path_security, mount_id
        )
        logger.info(
            "差分インデックス完了: +%d ~%d -%d (%s)",
            added,
            updated,
            deleted,
            mount_id or "default",
        )
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

# JSON レスポンス圧縮 (browse API 等)
# minimum_size=500: 500B 以上で gzip 適用
# compresslevel=5: 圧縮率と速度のバランス
app.add_middleware(GZipMiddleware, minimum_size=500, compresslevel=5)

# 例外ハンドラ登録
app.add_exception_handler(PathSecurityError, path_security_error_handler)  # type: ignore[arg-type]
app.add_exception_handler(NodeNotFoundError, node_not_found_error_handler)  # type: ignore[arg-type]
app.add_exception_handler(ArchiveSecurityError, archive_security_error_handler)
app.add_exception_handler(ArchivePasswordError, archive_password_error_handler)

# ルーター登録
app.include_router(mounts.router)
app.include_router(browse.router)
app.include_router(file.router)
app.include_router(search.router)


@app.get("/api/health")
async def health() -> dict[str, str]:
    """Health check endpoint."""
    return {"status": "ok"}


# --- 本番用: 静的ファイル配信 + SPA フォールバック ---
# Vite ビルド出力を配信。開発時は Vite dev server がプロキシで処理するため影響なし
_static_dir = FilePath(__file__).parent.parent / "static"

if _static_dir.exists():
    # ハッシュ付きアセット (JS/CSS) — 長期キャッシュ可能
    app.mount(
        "/assets",
        StaticFiles(directory=_static_dir / "assets"),
        name="assets",
    )

    # SPA フォールバック — /api 以外の全パスで index.html を返す
    @app.get("/{full_path:path}")
    async def spa_fallback(full_path: str) -> FileResponse:
        """SPA のクライアントサイドルーティング対応."""
        return FileResponse(_static_dir / "index.html")
