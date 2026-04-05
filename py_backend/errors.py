"""共通エラーモデルと例外クラス.

全 API が統一的なエラーレスポンスを返すための基盤。
"""

from fastapi import Request
from fastapi.responses import JSONResponse
from pydantic import BaseModel


class ErrorResponse(BaseModel):
    """共通エラーレスポンスモデル.

    - error: 人間可読なエラーメッセージ
    - code: 機械可読なエラーコード
    - detail: 追加情報 (任意)
    """

    error: str
    code: str
    detail: str | None = None


class PathSecurityError(Exception):
    """パスセキュリティ違反 (traversal, symlink 等)."""

    def __init__(self, message: str = "アクセスが拒否されました") -> None:
        self.message = message
        super().__init__(message)


class NodeNotFoundError(Exception):
    """node_id に対応するパスが見つからない."""

    def __init__(self, node_id: str) -> None:
        self.node_id = node_id
        super().__init__(f"node_id が見つかりません: {node_id}")


async def path_security_error_handler(
    _request: Request, exc: PathSecurityError
) -> JSONResponse:
    """PathSecurityError → 403 レスポンス."""
    return JSONResponse(
        status_code=403,
        content=ErrorResponse(
            error=exc.message,
            code="FORBIDDEN_PATH",
        ).model_dump(),
    )


async def node_not_found_error_handler(
    _request: Request, exc: NodeNotFoundError
) -> JSONResponse:
    """NodeNotFoundError → 404 レスポンス."""
    return JSONResponse(
        status_code=404,
        content=ErrorResponse(
            error=f"node_id が見つかりません: {exc.node_id}",
            code="NOT_FOUND",
        ).model_dump(),
    )


class NotADirectoryApiError(Exception):
    """ディレクトリではないパスへの browse アクセス."""

    def __init__(self, message: str = "ディレクトリではありません") -> None:
        self.message = message
        super().__init__(message)


class InvalidArchiveError(Exception):
    """壊れた・読み取れないアーカイブ."""

    def __init__(self, message: str = "アーカイブを読み取れません") -> None:
        self.message = message
        super().__init__(message)


class InvalidCursorError(Exception):
    """不正なページネーションカーソル."""

    def __init__(self, message: str = "不正なカーソルです") -> None:
        self.message = message
        super().__init__(message)


async def not_a_directory_error_handler(
    _request: Request, exc: NotADirectoryApiError
) -> JSONResponse:
    """NotADirectoryApiError → 422 レスポンス."""
    return JSONResponse(
        status_code=422,
        content=ErrorResponse(
            error=exc.message,
            code="NOT_A_DIRECTORY",
        ).model_dump(),
    )


async def invalid_archive_error_handler(
    _request: Request, exc: InvalidArchiveError
) -> JSONResponse:
    """InvalidArchiveError → 422 レスポンス."""
    return JSONResponse(
        status_code=422,
        content=ErrorResponse(
            error=exc.message,
            code="INVALID_ARCHIVE",
        ).model_dump(),
    )


async def invalid_cursor_error_handler(
    _request: Request, exc: InvalidCursorError
) -> JSONResponse:
    """InvalidCursorError → 400 レスポンス."""
    return JSONResponse(
        status_code=400,
        content=ErrorResponse(
            error=exc.message,
            code="INVALID_CURSOR",
        ).model_dump(),
    )


async def archive_security_error_handler(
    _request: Request, exc: Exception
) -> JSONResponse:
    """ArchiveSecurityError → 422 レスポンス."""
    return JSONResponse(
        status_code=422,
        content=ErrorResponse(
            error=str(exc),
            code="ARCHIVE_SECURITY_ERROR",
        ).model_dump(),
    )


async def archive_password_error_handler(
    _request: Request, exc: Exception
) -> JSONResponse:
    """ArchivePasswordError → 422 レスポンス."""
    return JSONResponse(
        status_code=422,
        content=ErrorResponse(
            error=str(exc),
            code="ARCHIVE_PASSWORD_REQUIRED",
        ).model_dump(),
    )
