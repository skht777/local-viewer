"""パストラバーサル防止モジュール.

全ファイルアクセスはこのモジュールを経由する。
- resolve() 後に許可ルートディレクトリ配下であることを検証
- symlink はデフォルトで追跡しない
- 不正アクセスは PathSecurityError を送出
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import TYPE_CHECKING

from py_backend.errors import PathSecurityError

if TYPE_CHECKING:
    from py_backend.config import Settings


class PathSecurity:
    """パスの安全性を検証するサービス.

    - root_dirs: 許可されたルートディレクトリのリスト
    - is_allow_symlinks: symlink 追跡の許可フラグ
    """

    def __init__(
        self,
        settings_or_dirs: Settings | list[Path],
        *,
        is_allow_symlinks: bool = False,
    ) -> None:
        if isinstance(settings_or_dirs, list):
            # 複数ルート対応: list[Path] を直接受け取る
            if not settings_or_dirs:
                msg = "root_dirs は少なくとも1つ必要です"
                raise ValueError(msg)
            self._roots = [r.resolve() for r in settings_or_dirs]
            self.is_allow_symlinks = is_allow_symlinks
        else:
            # Settings オブジェクトから初期化
            self._roots = [settings_or_dirs.mount_base_dir.resolve()]
            self.is_allow_symlinks = settings_or_dirs.is_allow_symlinks

        # 文字列比較用にキャッシュ (全ルート)
        self._root_entries: list[tuple[str, str, Path]] = [
            (str(r), str(r) + os.sep, r) for r in self._roots
        ]
        # 後方互換: 単一ルートの高速パス用キャッシュ
        self._root_str = str(self._roots[0])
        self._root_prefix = self._root_str + os.sep

    @property
    def root_dirs(self) -> list[Path]:
        """全許可ルートディレクトリを返す."""
        return list(self._roots)

    def find_root_for(self, resolved: Path) -> Path | None:
        """パスが属するルートディレクトリを返す.

        どのルートにも属さなければ None。
        resolved は resolve() 済みであること。
        """
        s = str(resolved)
        for root_str, root_prefix, root in self._root_entries:
            if s == root_str or s.startswith(root_prefix):
                return root
        return None

    def validate(self, path: Path) -> Path:
        """パスを検証し、解決済みの安全なパスを返す.

        検証手順:
        1. resolve() で正規化
        2. 許可ルートディレクトリのいずれか配下であることを確認
        3. symlink チェック (許可されていない場合)
        """
        resolved = path.resolve()

        if not self._is_under_root(resolved):
            msg = "許可ルートディレクトリの外へのアクセスは禁止されています"
            raise PathSecurityError(msg)

        if not self.is_allow_symlinks and self._has_symlink(path):
            raise PathSecurityError("symlink の追跡は許可されていません")

        return resolved

    def validate_existing(self, path: Path) -> Path:
        """パスを検証し、存在することも確認する.

        validate() + 存在確認。
        """
        resolved = self.validate(path)
        if not resolved.exists():
            msg = f"パスが存在しません: {resolved}"
            raise FileNotFoundError(msg)
        return resolved

    def safe_join(self, *parts: str) -> Path:
        """先頭ルートと部分パスを安全に結合する.

        各部分パスに不正な要素がないか検証してから結合する。
        """
        for part in parts:
            if "\x00" in part:
                raise PathSecurityError("パスに NUL バイトが含まれています")
            if Path(part).is_absolute():
                raise PathSecurityError("絶対パスは指定できません")

        joined = self._roots[0].joinpath(*parts)
        return self.validate(joined)

    def validate_child(self, child: Path, *, is_symlink: bool) -> Path:
        """validate 済みディレクトリの直接の子を検証する (軽量版).

        親が検証済みなので、子自身の symlink チェックのみ行う。
        resolve() も symlink でない場合はスキップ。
        """
        if not self.is_allow_symlinks and is_symlink:
            raise PathSecurityError("symlink の追跡は許可されていません")

        resolved = child.resolve() if is_symlink else child
        if not self._is_under_root(resolved):
            msg = "許可ルートディレクトリの外へのアクセスは禁止されています"
            raise PathSecurityError(msg)
        return resolved

    @staticmethod
    def validate_slug(slug: str) -> None:
        """マウントポイントの slug が安全であることを検証する.

        slug は MOUNT_BASE_DIR 直下のディレクトリ名。
        パストラバーサルに利用できる文字列を拒否する。
        """
        if not slug or slug == ".":
            msg = "slug が空または '.' です"
            raise PathSecurityError(msg)
        if "\x00" in slug:
            raise PathSecurityError("slug に NUL バイトが含まれています")
        if "/" in slug or "\\" in slug:
            raise PathSecurityError("slug にパス区切り文字が含まれています")
        if slug == ".." or slug.startswith(".."):
            raise PathSecurityError("slug に不正なパス要素が含まれています")

    def _is_under_root(self, resolved: Path) -> bool:
        """resolved パスが許可ルートのいずれか配下にあるか判定する.

        文字列比較で O(N) 判定 (N = ルート数)。
        root_prefix に os.sep を含めて /data と /data2 の誤判定を防止。
        """
        s = str(resolved)
        for root_str, root_prefix, _ in self._root_entries:
            if s == root_str or s.startswith(root_prefix):
                return True
        return False

    def _has_symlink(self, path: Path) -> bool:
        """パスのいずれかの要素が symlink かどうかを検出する.

        元のパス (resolve 前) の各要素を該当ルートから順に確認する。
        """
        abs_path = path if path.is_absolute() else Path.cwd() / path
        # パスが属するルートを特定
        resolved = abs_path.resolve()
        root = self.find_root_for(resolved)
        if root is None:
            return True

        try:
            rel = abs_path.relative_to(root)
        except ValueError:
            return True

        current = root
        for part in rel.parts:
            current = current / part
            if current.is_symlink():
                return True
        return False
