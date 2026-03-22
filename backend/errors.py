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
