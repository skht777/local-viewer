"""Local Content Viewer -- FastAPI entry point."""

import logging
from collections.abc import AsyncGenerator
from contextlib import asynccontextmanager
from pathlib import Path as FilePath

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.middleware.gzip import GZipMiddleware
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

from backend.config import init_settings
from backend.errors import (
    NodeNotFoundError,
    PathSecurityError,
    archive_password_error_handler,
    archive_security_error_handler,
    node_not_found_error_handler,
    path_security_error_handler,
)
from backend.routers import browse, file, search
from backend.services.archive_security import (
    ArchivePasswordError,
    ArchiveSecurityError,
)
from backend.services.archive_service import ArchiveService
from backend.services.node_registry import NodeRegistry
from backend.services.path_security import PathSecurity
from backend.services.temp_file_cache import TempFileCache

logger = logging.getLogger(__name__)

# サービスインスタンス (lifespan で初期化)
_node_registry: NodeRegistry | None = None
_archive_service: ArchiveService | None = None
_temp_file_cache: TempFileCache | None = None


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


@asynccontextmanager
async def lifespan(_app: FastAPI) -> AsyncGenerator[None]:
    """アプリケーションの起動/終了処理.

    起動時: Settings → PathSecurity → NodeRegistry → ArchiveService を初期化。
    """
    global _node_registry, _archive_service, _temp_file_cache

    # アプリケーションロガーを uvicorn のハンドラに接続
    # uvicorn はルートロガーを設定しないため、backend.* の出力先がない
    app_logger = logging.getLogger("backend")
    if not app_logger.handlers:
        app_logger.addHandler(logging.getLogger("uvicorn").handlers[0])
        app_logger.setLevel(logging.INFO)

    settings = init_settings()
    path_security = PathSecurity(settings)
    _node_registry = NodeRegistry(
        path_security,
        archive_registry_max_entries=settings.archive_registry_max_entries,
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

    # 起動時診断: 各アーカイブ形式の利用可否
    diag = _archive_service.get_diagnostics()
    for fmt, available in diag.items():
        level = logging.INFO if available else logging.WARNING
        status = "available" if available else "NOT available"
        logger.log(level, "Archive: %s support: %s", fmt, status)

    # DI: routers のスタブを実インスタンスに差し替え
    _app.dependency_overrides[browse.get_node_registry] = get_node_registry
    _app.dependency_overrides[file.get_node_registry] = get_node_registry
    _app.dependency_overrides[browse.get_archive_service] = get_archive_service
    _app.dependency_overrides[file.get_archive_service] = get_archive_service
    _app.dependency_overrides[file.get_temp_file_cache] = get_temp_file_cache

    yield

    _node_registry = None
    _archive_service = None
    _temp_file_cache = None


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
