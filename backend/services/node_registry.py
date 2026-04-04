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
from concurrent.futures import ThreadPoolExecutor
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
from backend.services.natural_sort import natural_sort_key
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


class AncestorEntry(BaseModel):
    """パンくずリスト用の祖先エントリ."""

    node_id: str
    name: str


class EntryMeta(BaseModel):
    """browse レスポンスの 1 エントリ.

    - node_id: 不透明 ID
    - name: ファイル/ディレクトリ名
    - kind: エントリの種類
    - size_bytes: ファイルサイズ (ディレクトリは None)
    - mime_type: MIME タイプ (ディレクトリは None)
    - child_count: ディレクトリの子エントリ数 (ファイルは None)
    - modified_at: 更新日時 POSIX epoch 秒 (アーカイブエントリ・マウントルートは None)
    - preview_node_ids: ディレクトリ内の先頭画像 node_id (最大3件、kind=image のみ)
      画像がない場合は None。ファイル・アーカイブエントリ・マウントルートは常に None
    """

    node_id: str
    name: str
    kind: EntryKind
    size_bytes: int | None = None
    mime_type: str | None = None
    child_count: int | None = None
    modified_at: float | None = None
    preview_node_ids: list[str] | None = None


class BrowseResponse(BaseModel):
    """browse API のレスポンス.

    - current_node_id: 現在のディレクトリの node_id (ルートは None)
    - current_name: 現在のディレクトリ名
    - parent_node_id: 親ディレクトリの node_id (ルートは None)
    - ancestors: 祖先エントリ (マウントルートから親まで、パンくず用)
    - entries: 子エントリ一覧
    - next_cursor: 次ページカーソル (null = 最終ページ or ページネーション未使用)
    - total_count: 全エントリ数 (ページネーション使用時のみ)
    """

    current_node_id: str | None = None
    current_name: str
    parent_node_id: str | None = None
    ancestors: list[AncestorEntry] = []
    entries: list[EntryMeta]
    next_cursor: str | None = None
    total_count: int | None = None


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
        self._id_to_composite_key: dict[str, str] = {}  # eviction O(1) 用逆引き
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

    # stat 並列化の閾値 (WSL2 DrvFs 等で stat が遅い環境向け)
    _PARALLEL_STAT_THRESHOLD = 200
    _PARALLEL_STAT_WORKERS = 32
    # _scan_child_meta を実行するサブディレクトリ数の上限
    _CHILD_META_LIMIT = 100

    def list_directory(self, directory: Path) -> list[EntryMeta]:
        """ディレクトリの内容を一覧し、各エントリを登録して返す.

        os.scandir() で DirEntry を取得し、キャッシュ済みの
        is_dir/is_symlink を活用して I/O を最小化する。
        大量エントリ時は stat を並列化してスループットを向上する。
        """
        validated = self._path_security.validate_existing(directory)

        # os.scandir() — DirEntry の is_dir/is_symlink は追加 I/O なし
        with os.scandir(validated) as scanner:
            dir_entries = sorted(
                scanner,
                key=lambda e: (
                    not e.is_dir(follow_symlinks=False),
                    natural_sort_key(e.name),
                ),
            )

        # Phase 1: validate + classify (stat 不要、高速)
        pre_entries: list[tuple[os.DirEntry[str], Path, str, EntryKind, bool]] = []
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
            pre_entries.append((de, resolved, node_id, kind, is_dir))

        # Phase 2: stat (大量エントリ時は並列化で I/O スループット向上)
        if len(pre_entries) > self._PARALLEL_STAT_THRESHOLD:
            with ThreadPoolExecutor(max_workers=self._PARALLEL_STAT_WORKERS) as pool:
                stats = list(pool.map(lambda x: x[0].stat(), pre_entries))
        else:
            stats = [pe[0].stat() for pe in pre_entries]

        # Phase 3: EntryMeta 構築
        entries: list[EntryMeta] = []
        dir_count = 0
        for (de, resolved, node_id, kind, is_dir), st in zip(
            pre_entries, stats, strict=True
        ):
            if is_dir:
                dir_count += 1
                if dir_count <= self._CHILD_META_LIMIT:
                    child_count, preview_ids = self._scan_child_meta(resolved)
                else:
                    child_count, preview_ids = None, None
                entries.append(
                    EntryMeta(
                        node_id=node_id,
                        name=de.name,
                        kind=kind,
                        child_count=child_count,
                        modified_at=st.st_mtime,
                        preview_node_ids=preview_ids,
                    )
                )
            else:
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
                        modified_at=st.st_mtime,
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
            child_count, _ = self._scan_child_meta(root)
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

        ルートディレクトリ (マウントポイント) 自体の場合のみ None を返す。
        ルート直下のディレクトリの場合はルートの node_id を返す。
        """
        resolved = path.resolve()
        roots = self._path_security.root_dirs
        # ルートディレクトリ自体なら None (トップページへの遷移はフロントエンドで処理)
        if resolved in roots:
            return None
        parent = resolved.parent
        try:
            self._path_security.validate(parent)
        except PathSecurityError, OSError:
            return None
        return self.register(parent)

    def get_ancestors(self, path: Path) -> list[AncestorEntry]:
        """パスの祖先エントリを返す (マウントルートから親まで).

        前提: resolve(node_id) で取得した validate 済みパスを受け取る。
        パンくずリスト表示用。現在のディレクトリ自体は含まない。
        """
        resolved = path.resolve()
        root = self._path_security.find_root_for(resolved)
        if root is None:
            return []
        # ルート自体に ancestors はない
        if resolved == root:
            return []

        # resolved 済みパスの .parent も resolved なので register_resolved で高速化
        ancestors: list[AncestorEntry] = []
        current = resolved.parent
        while current != root:
            node_id = self.register_resolved(current)
            ancestors.append(AncestorEntry(node_id=node_id, name=current.name))
            current = current.parent

        # マウントルート自体を追加
        root_node_id = self.register_resolved(root)
        root_name = self._mount_names.get(root, root.name)
        ancestors.append(AncestorEntry(node_id=root_node_id, name=root_name))

        ancestors.reverse()
        return ancestors

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

        # LRU 上限管理 (O(1) eviction)
        while len(self._id_to_archive_entry) >= self._archive_registry_max:
            evicted_id, _ = self._id_to_archive_entry.popitem(last=False)
            evicted_key = self._id_to_composite_key.pop(evicted_id, None)
            if evicted_key is not None:
                self._archive_entry_to_id.pop(evicted_key, None)

        self._id_to_archive_entry[node_id] = (resolved, entry_name)
        self._archive_entry_to_id[composite_key] = node_id
        self._id_to_composite_key[node_id] = composite_key
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

    def _scan_child_meta(
        self, directory: Path, preview_limit: int = 3
    ) -> tuple[int, list[str] | None]:
        """子ディレクトリの child_count と preview_ids を1回の scandir で取得する.

        戻り値: (child_count, preview_ids)
        画像が見つからなければ preview_ids は None。
        """
        count = 0
        preview_ids: list[str] = []
        try:
            with os.scandir(directory) as scanner:
                for de in scanner:
                    count += 1
                    if len(preview_ids) >= preview_limit:
                        continue
                    if de.is_dir(follow_symlinks=False):
                        continue
                    name = de.name
                    dot_idx = name.rfind(".")
                    ext = name[dot_idx:].lower() if dot_idx > 0 else ""
                    if ext not in IMAGE_EXTENSIONS:
                        continue
                    child = Path(de.path)
                    try:
                        resolved = self._path_security.validate_child(
                            child, is_symlink=de.is_symlink()
                        )
                        preview_ids.append(self.register_resolved(resolved))
                    except PathSecurityError, OSError:
                        continue
        except PermissionError:
            pass
        return count, preview_ids if preview_ids else None
