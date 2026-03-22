"""パストラバーサル防止モジュール.

全ファイルアクセスはこのモジュールを経由する。
- resolve() 後に ROOT_DIR 配下であることを検証
- symlink はデフォルトで追跡しない
- 不正アクセスは PathSecurityError を送出
"""

import os
from pathlib import Path

from backend.config import Settings
from backend.errors import PathSecurityError


class PathSecurity:
    """パスの安全性を検証するサービス.

    - root_dir: 許可されたルートディレクトリ
    - is_allow_symlinks: symlink 追跡の許可フラグ
    """

    def __init__(self, settings: Settings) -> None:
        self.root_dir = settings.root_dir
        self.is_allow_symlinks = settings.is_allow_symlinks
        # 文字列比較用にキャッシュ
        # root_prefix に os.sep を含めて /root vs /root2 を区別
        self._root_str = str(self.root_dir)
        self._root_prefix = self._root_str + os.sep

    def validate(self, path: Path) -> Path:
        """パスを検証し、解決済みの安全なパスを返す.

        検証手順:
        1. resolve() で正規化
        2. ROOT_DIR 配下であることを確認
        3. symlink チェック (許可されていない場合)
        """
        resolved = path.resolve()

        if not self._is_under_root(resolved):
            raise PathSecurityError("ROOT_DIR の外へのアクセスは禁止されています")

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
        """ROOT_DIR と部分パスを安全に結合する.

        各部分パスに不正な要素がないか検証してから結合する。
        """
        for part in parts:
            if "\x00" in part:
                raise PathSecurityError("パスに NUL バイトが含まれています")
            if Path(part).is_absolute():
                raise PathSecurityError("絶対パスは指定できません")

        joined = self.root_dir.joinpath(*parts)
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
            raise PathSecurityError("ROOT_DIR の外へのアクセスは禁止されています")
        return resolved

    def _is_under_root(self, resolved: Path) -> bool:
        """resolved パスが root_dir 配下にあるか判定する.

        文字列比較で O(1) 判定。root_prefix に os.sep を含めて
        /data と /data2 のような prefix 一致の誤判定を防止。
        """
        s = str(resolved)
        return s == self._root_str or s.startswith(self._root_prefix)

    def _has_symlink(self, path: Path) -> bool:
        """パスのいずれかの要素が symlink かどうかを検出する.

        元のパス (resolve 前) の各要素を root_dir から順に確認する。
        resolve 後のパスではなく、元パスを辿ることで symlink を検出する。
        """
        # 元パスを絶対パスにして root_dir からの相対パスを取得
        abs_path = path if path.is_absolute() else Path.cwd() / path
        try:
            rel = abs_path.relative_to(self.root_dir)
        except ValueError:
            return True

        current = self.root_dir
        for part in rel.parts:
            current = current / part
            if current.is_symlink():
                return True
        return False
