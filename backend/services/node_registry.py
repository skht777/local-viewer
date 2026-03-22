"""node_id ↔ 実パス マッピング管理.

node_id は HMAC-SHA256(secret, relative_path) の先頭16文字 (hex)。
- 同じパスに対して常に同じ node_id を返す (冪等)
- secret により外部からの推測を防止
- クライアントに実パスを公開しない
"""

import hashlib
import hmac
import mimetypes
import os
from enum import StrEnum
from pathlib import Path

from pydantic import BaseModel

from backend.errors import NodeNotFoundError, PathSecurityError
from backend.services.path_security import PathSecurity

# 拡張子 → EntryKind のマッピング
IMAGE_EXTENSIONS = frozenset(
    {".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".avif"}
)
VIDEO_EXTENSIONS = frozenset({".mp4", ".webm", ".mkv", ".avi", ".mov"})
ARCHIVE_EXTENSIONS = frozenset({".zip", ".rar", ".7z", ".cbz", ".cbr"})
PDF_EXTENSIONS = frozenset({".pdf"})


class EntryKind(StrEnum):
    """エントリの種類."""

    DIRECTORY = "directory"
    IMAGE = "image"
    VIDEO = "video"
    PDF = "pdf"
    ARCHIVE = "archive"
    OTHER = "other"


class EntryMeta(BaseModel):
    """browse レスポンスの 1 エントリ.

    - node_id: 不透明 ID
    - name: ファイル/ディレクトリ名
    - kind: エントリの種類
    - size_bytes: ファイルサイズ (ディレクトリは None)
    - mime_type: MIME タイプ (ディレクトリは None)
    - child_count: ディレクトリの子エントリ数 (ファイルは None)
    """

    node_id: str
    name: str
    kind: EntryKind
    size_bytes: int | None = None
    mime_type: str | None = None
    child_count: int | None = None


class BrowseResponse(BaseModel):
    """browse API のレスポンス.

    - current_node_id: 現在のディレクトリの node_id (ルートは None)
    - current_name: 現在のディレクトリ名
    - parent_node_id: 親ディレクトリの node_id (ルートは None)
    - entries: 子エントリ一覧
    """

    current_node_id: str | None = None
    current_name: str
    parent_node_id: str | None = None
    entries: list[EntryMeta]


class NodeRegistry:
    """node_id ↔ 実パス のマッピングを管理する.

    - HMAC-SHA256 でパスから node_id を決定的に生成
    - 双方向マッピングをメモリに保持
    - path_security を経由して安全なパスのみ登録
    """

    def __init__(self, path_security: PathSecurity) -> None:
        self._path_security = path_security
        self._secret = os.environ.get(
            "NODE_SECRET", "local-viewer-default-secret"
        ).encode()
        self._id_to_path: dict[str, Path] = {}
        self._path_to_id: dict[Path, str] = {}

    @property
    def path_security(self) -> PathSecurity:
        """PathSecurity インスタンスを返す."""
        return self._path_security

    def _generate_id(self, path: Path) -> str:
        """パスから決定的な node_id を生成する.

        HMAC-SHA256(secret, relative_path) の先頭16文字 (hex)。
        """
        relative = path.relative_to(self._path_security.root_dir)
        digest = hmac.new(
            self._secret,
            str(relative).encode(),
            hashlib.sha256,
        ).hexdigest()
        return digest[:16]

    def register(self, path: Path) -> str:
        """パスを登録し、node_id を返す.

        既に登録済みならキャッシュから返す。
        """
        resolved = path.resolve()
        if resolved in self._path_to_id:
            return self._path_to_id[resolved]

        node_id = self._generate_id(resolved)
        self._id_to_path[node_id] = resolved
        self._path_to_id[resolved] = node_id
        return node_id

    def resolve(self, node_id: str) -> Path:
        """node_id から実パスを返す.

        Raises:
            NodeNotFoundError: 未登録の node_id
        """
        path = self._id_to_path.get(node_id)
        if path is None:
            raise NodeNotFoundError(node_id)
        return path

    def list_directory(self, directory: Path) -> list[EntryMeta]:
        """ディレクトリの内容を一覧し、各エントリを登録して返す.

        - path_security でディレクトリの安全性を検証
        - 各エントリを登録して node_id を付与
        - ソート: ディレクトリ優先、名前の自然順
        """
        validated = self._path_security.validate_existing(directory)
        entries: list[EntryMeta] = []

        children = list(validated.iterdir())
        children.sort(key=lambda p: (not p.is_dir(), p.name.lower()))

        for child in children:
            # 不正なエントリはスキップ
            try:
                self._path_security.validate(child)
            except (PathSecurityError, OSError):
                continue

            node_id = self.register(child)
            kind = self._classify(child)

            if child.is_dir():
                child_count = self._count_children(child)
                entries.append(
                    EntryMeta(
                        node_id=node_id,
                        name=child.name,
                        kind=kind,
                        child_count=child_count,
                    )
                )
            else:
                st = child.stat()
                mime = mimetypes.guess_type(child.name)[0]
                entries.append(
                    EntryMeta(
                        node_id=node_id,
                        name=child.name,
                        kind=kind,
                        size_bytes=st.st_size,
                        mime_type=mime,
                    )
                )

        return entries

    def get_parent_node_id(self, path: Path) -> str | None:
        """パスの親ディレクトリの node_id を返す.

        ROOT_DIR または ROOT_DIR 直下のディレクトリの場合は None。
        """
        resolved = path.resolve()
        if resolved == self._path_security.root_dir:
            return None
        parent = resolved.parent
        if parent == self._path_security.root_dir:
            return None
        try:
            self._path_security.validate(parent)
        except (PathSecurityError, OSError):
            return None
        return self.register(parent)

    @staticmethod
    def _classify(path: Path) -> EntryKind:
        """ファイルの種類を拡張子から判定する."""
        if path.is_dir():
            return EntryKind.DIRECTORY
        suffix = path.suffix.lower()
        if suffix in IMAGE_EXTENSIONS:
            return EntryKind.IMAGE
        if suffix in VIDEO_EXTENSIONS:
            return EntryKind.VIDEO
        if suffix in PDF_EXTENSIONS:
            return EntryKind.PDF
        if suffix in ARCHIVE_EXTENSIONS:
            return EntryKind.ARCHIVE
        return EntryKind.OTHER

    @staticmethod
    def _count_children(directory: Path) -> int:
        """ディレクトリの直接の子エントリ数を返す."""
        try:
            return sum(1 for _ in directory.iterdir())
        except PermissionError:
            return 0
