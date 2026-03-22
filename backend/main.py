"""Local Content Viewer -- FastAPI entry point."""

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
    node_not_found_error_handler,
    path_security_error_handler,
)
from backend.routers import browse, file
from backend.services.node_registry import NodeRegistry
from backend.services.path_security import PathSecurity

# サービスインスタンス (lifespan で初期化)
_node_registry: NodeRegistry | None = None


def get_node_registry() -> NodeRegistry:
    """NodeRegistry の DI 用依存関数."""
    if _node_registry is None:
        msg = "NodeRegistry が初期化されていません"
        raise RuntimeError(msg)
    return _node_registry


@asynccontextmanager
async def lifespan(_app: FastAPI) -> AsyncGenerator[None]:
    """アプリケーションの起動/終了処理.

    起動時: Settings → PathSecurity → NodeRegistry を初期化。
    """
    global _node_registry
    settings = init_settings()
    path_security = PathSecurity(settings)
    _node_registry = NodeRegistry(path_security)

    # DI: routers のスタブを実インスタンスに差し替え
    _app.dependency_overrides[browse.get_node_registry] = get_node_registry
    _app.dependency_overrides[file.get_node_registry] = get_node_registry

    yield

    _node_registry = None


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

# ルーター登録
app.include_router(browse.router)
app.include_router(file.router)


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
