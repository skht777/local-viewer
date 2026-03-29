"""node_id ↔ 実パス マッピング管理.

node_id は HMAC-SHA256(secret, relative_path) の先頭16文字 (hex)。
- 同じパスに対して常に同じ node_id を返す (冪等)
- secret により外部からの推測を防止
- クライアントに実パスを公開しない
"""

from __future__ import annotations

import hashlib
import hmac
import mimetypes
import os
from collections import OrderedDict
from pathlib import Path
from typing import TYPE_CHECKING

from pydantic import BaseModel

from backend.errors import NodeNotFoundError, PathSecurityError
from backend.services.extensions import (
    ARCHIVE_EXTENSIONS,
    IMAGE_EXTENSIONS,
    MIME_MAP,
    PDF_EXTENSIONS,
    VIDEO_EXTENSIONS,
    EntryKind,
)
from backend.services.path_security import PathSecurity

if TYPE_CHECKING:
    from backend.services.archive_reader import ArchiveEntry

# 再エクスポート (既存の外部 import を壊さないため)
__all__ = [
    "ARCHIVE_EXTENSIONS",
    "IMAGE_EXTENSIONS",
    "MIME_MAP",
    "PDF_EXTENSIONS",
    "VIDEO_EXTENSIONS",
    "EntryKind",
]


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

    def __init__(
        self,
        path_security: PathSecurity,
        archive_registry_max_entries: int = 100_000,
        mount_names: dict[Path, str] | None = None,
    ) -> None:
        self._path_security = path_security
        self._secret = os.environ.get(
            "NODE_SECRET", "local-viewer-default-secret"
        ).encode()
        self._id_to_path: dict[str, Path] = {}
        self._path_to_id: dict[str, str] = {}
        # 複数ルート対応: 全ルートのキャッシュ
        self._root_entries: list[tuple[str, str, Path]] = [
            (str(r), str(r) + os.sep, r) for r in path_security.root_dirs
        ]
        # マウントポイント名マッピング
        self._mount_names: dict[Path, str] = mount_names or {}
        # mount_id → root_dir マッピング (search 用)
        self._mount_id_map: dict[str, Path] = {}
        # アーカイブエントリ用マッピング (LRU, 上限管理)
        self._id_to_archive_entry: OrderedDict[str, tuple[Path, str]] = OrderedDict()
        self._archive_entry_to_id: dict[str, str] = {}
        self._archive_registry_max = archive_registry_max_entries

    @property
    def path_security(self) -> PathSecurity:
        """PathSecurity インスタンスを返す."""
        return self._path_security

    @property
    def mount_id_map(self) -> dict[str, Path]:
        """mount_id → root_dir のマッピング."""
        return dict(self._mount_id_map)

    def set_mount_id_map(self, mapping: dict[str, Path]) -> None:
        """mount_id → root_dir マッピングを設定する."""
        self._mount_id_map = dict(mapping)

    def _generate_id(self, path: Path) -> str:
        """パスから決定的な node_id を生成する.

        HMAC-SHA256(secret, "{root}::{relative_path}") の先頭16文字。
        ルートパスを入力に含め、異なるマウントの同名ファイルの衝突を回避。
        """
        root = self._path_security.find_root_for(path)
        if root is None:
            msg = f"パスがどのルートにも属しません: {path}"
            raise ValueError(msg)
        relative = path.relative_to(root)
        hmac_input = f"{root}::{relative}"
        digest = hmac.new(
            self._secret,
            hmac_input.encode(),
            hashlib.sha256,
        ).hexdigest()
        return digest[:16]

    def register(self, path: Path) -> str:
        """パスを登録し、node_id を返す.

        既に登録済みならキャッシュから返す。
        外部からの呼び出し用。resolve() で正規化する (fail-closed)。
        """
        resolved = path.resolve()
        key = str(resolved)
        if key in self._path_to_id:
            return self._path_to_id[key]

        node_id = self._generate_id(resolved)
        self._id_to_path[node_id] = resolved
        self._path_to_id[key] = node_id
        return node_id

    def register_resolved(self, resolved: Path) -> str:
        """検証済み・正規化済みパスを登録する (内部用 fast-path).

        validate / validate_child 済みのパスのみ渡すこと。
        resolve() と relative_to() をスキップして高速化。
        """
        key = str(resolved)
        if key in self._path_to_id:
            return self._path_to_id[key]

        # 文字列スライスで相対パス取得 (root 配下が保証済み)
        # 複数ルートを順に試行
        root_str = ""
        rel = ""
        for rs, rp, _ in self._root_entries:
            if key == rs:
                root_str = rs
                rel = ""
                break
            if key.startswith(rp):
                root_str = rs
                rel = key[len(rp) :]
                break

        hmac_input = f"{root_str}::{rel}"
        digest = hmac.new(
            self._secret,
            hmac_input.encode(),
            hashlib.sha256,
        ).hexdigest()
        node_id = digest[:16]
        self._id_to_path[node_id] = resolved
        self._path_to_id[key] = node_id
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

        os.scandir() で DirEntry を取得し、キャッシュ済みの
        is_dir/is_symlink/stat を活用して I/O を最小化する。
        """
        validated = self._path_security.validate_existing(directory)
        entries: list[EntryMeta] = []

        # os.scandir() — DirEntry の is_dir/is_symlink は追加 I/O なし
        with os.scandir(validated) as scanner:
            dir_entries = sorted(
                scanner,
                key=lambda e: (not e.is_dir(follow_symlinks=False), e.name.lower()),
            )

        for de in dir_entries:
            child = Path(de.path)
            is_dir = de.is_dir(follow_symlinks=False)
            is_link = de.is_symlink()

            # 親は validate_existing 済み → 子は軽量チェックのみ
            try:
                resolved = self._path_security.validate_child(child, is_symlink=is_link)
            except PathSecurityError, OSError:
                continue

            node_id = self.register_resolved(resolved)
            kind = self._classify_entry(de)

            if is_dir:
                child_count = self._count_children_scandir(de.path)
                entries.append(
                    EntryMeta(
                        node_id=node_id,
                        name=de.name,
                        kind=kind,
                        child_count=child_count,
                    )
                )
            else:
                st = de.stat()
                name = de.name
                dot_idx = name.rfind(".")
                ext = name[dot_idx:].lower() if dot_idx > 0 else ""
                mime = MIME_MAP.get(ext) or mimetypes.guess_type(name)[0]
                entries.append(
                    EntryMeta(
                        node_id=node_id,
                        name=de.name,
                        kind=kind,
                        size_bytes=st.st_size,
                        mime_type=mime,
                    )
                )

        return entries

    def list_mount_roots(
        self, mount_names: dict[Path, str] | None = None
    ) -> list[EntryMeta]:
        """マウントポイントのルートディレクトリ一覧を返す.

        mount_names が指定されている場合はそれを使用、
        なければインスタンスの _mount_names、
        さらになければディレクトリ名をフォールバック。
        """
        names = mount_names or self._mount_names
        entries: list[EntryMeta] = []
        for _, _, root in self._root_entries:
            node_id = self.register(root)
            name = names.get(root, root.name)
            child_count = self._count_children_scandir(str(root))
            entries.append(
                EntryMeta(
                    node_id=node_id,
                    name=name,
                    kind=EntryKind.DIRECTORY,
                    child_count=child_count,
                )
            )
        return entries

    def get_parent_node_id(self, path: Path) -> str | None:
        """パスの親ディレクトリの node_id を返す.

        ルートディレクトリまたはルート直下のディレクトリの場合は None。
        """
        resolved = path.resolve()
        roots = self._path_security.root_dirs
        # ルートディレクトリ自体なら None
        if resolved in roots:
            return None
        parent = resolved.parent
        # ルート直下なら None
        if parent in roots:
            return None
        try:
            self._path_security.validate(parent)
        except PathSecurityError, OSError:
            return None
        return self.register(parent)

    # --- アーカイブエントリ対応 ---

    def register_archive_entry(self, archive_path: Path, entry_name: str) -> str:
        """アーカイブエントリを登録し node_id を返す.

        HMAC 入力: "arc::{archive_relative_path}::{entry_name}"
        LRU 方式で上限超過時は最も古い登録を削除。
        """
        composite_key = f"arc::{archive_path}::{entry_name}"
        if composite_key in self._archive_entry_to_id:
            node_id = self._archive_entry_to_id[composite_key]
            self._id_to_archive_entry.move_to_end(node_id)
            return node_id

        # HMAC でアーカイブ相対パスとエントリ名から node_id を生成
        resolved = archive_path.resolve()
        root = self._path_security.find_root_for(resolved)
        if root is None:
            msg = f"アーカイブがどのルートにも属しません: {resolved}"
            raise ValueError(msg)
        rel = str(resolved.relative_to(root))
        hmac_input = f"arc::{root}::{rel}::{entry_name}"
        digest = hmac.new(
            self._secret,
            hmac_input.encode(),
            hashlib.sha256,
        ).hexdigest()
        node_id = digest[:16]

        # LRU 上限管理
        while len(self._id_to_archive_entry) >= self._archive_registry_max:
            evicted_id, _ = self._id_to_archive_entry.popitem(last=False)
            # 逆引きからも削除 (値からキー検索は重いので skip)
            for k, v in list(self._archive_entry_to_id.items()):
                if v == evicted_id:
                    del self._archive_entry_to_id[k]
                    break

        self._id_to_archive_entry[node_id] = (resolved, entry_name)
        self._archive_entry_to_id[composite_key] = node_id
        return node_id

    def resolve_archive_entry(self, node_id: str) -> tuple[Path, str] | None:
        """node_id がアーカイブエントリなら (archive_path, entry_name) を返す.

        通常のファイル/ディレクトリの場合は None。
        """
        result = self._id_to_archive_entry.get(node_id)
        if result is not None:
            self._id_to_archive_entry.move_to_end(node_id)
        return result

    def is_archive_entry(self, node_id: str) -> bool:
        """node_id がアーカイブエントリかどうか."""
        return node_id in self._id_to_archive_entry

    def list_archive_entries(
        self,
        archive_path: Path,
        archive_entries: list[ArchiveEntry],
    ) -> list[EntryMeta]:
        """ArchiveEntry リストを EntryMeta リストに変換する.

        各エントリに node_id を付与し、kind を判定する。
        ソートはアーカイブリーダー側で済んでいることを前提とする。
        """
        result: list[EntryMeta] = []
        for entry in archive_entries:
            node_id = self.register_archive_entry(archive_path, entry.name)

            # 拡張子から kind 判定
            name = entry.name
            dot_idx = name.rfind(".")
            ext = name[dot_idx:].lower() if dot_idx > 0 else ""
            if ext in IMAGE_EXTENSIONS:
                kind = EntryKind.IMAGE
            elif ext in VIDEO_EXTENSIONS:
                kind = EntryKind.VIDEO
            elif ext in PDF_EXTENSIONS:
                kind = EntryKind.PDF
            else:
                kind = EntryKind.OTHER

            # 表示名 (パスの最後の要素)
            display_name = name.rsplit("/", 1)[-1] if "/" in name else name

            mime = MIME_MAP.get(ext) or mimetypes.guess_type(name)[0]

            result.append(
                EntryMeta(
                    node_id=node_id,
                    name=display_name,
                    kind=kind,
                    size_bytes=entry.size_uncompressed,
                    mime_type=mime,
                )
            )
        return result

    @staticmethod
    def _classify_entry(entry: os.DirEntry[str]) -> EntryKind:
        """DirEntry から種類を判定する (追加 I/O なし).

        Path 生成を避け、文字列操作のみで拡張子を取得する。
        dot_idx > 0 で隠しファイル (.bashrc 等) の誤認を防止。
        """
        if entry.is_dir(follow_symlinks=False):
            return EntryKind.DIRECTORY
        name = entry.name
        dot_idx = name.rfind(".")
        suffix = name[dot_idx:].lower() if dot_idx > 0 else ""
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
    def _count_children_scandir(path: str) -> int:
        """os.scandir() でディレクトリの子エントリ数を返す."""
        try:
            with os.scandir(path) as it:
                return sum(1 for _ in it)
        except PermissionError:
            return 0
