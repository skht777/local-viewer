"""アプリケーション設定.

環境変数からの設定読み込みを一元管理する。
"""

import os
from pathlib import Path


class Settings:
    """アプリケーション設定.

    - ROOT_DIR: コンテンツ配信のルートディレクトリ
    - is_allow_symlinks: symlink 追跡の許可フラグ
    """

    root_dir: Path
    is_allow_symlinks: bool

    def __init__(self) -> None:
        raw = os.environ.get("ROOT_DIR", "")
        if not raw:
            msg = "ROOT_DIR 環境変数が設定されていません"
            raise ValueError(msg)
        self.root_dir = Path(raw).resolve()
        if not self.root_dir.is_dir():
            msg = (
                f"ROOT_DIR が存在しないか、ディレクトリではありません: {self.root_dir}"
            )
            raise ValueError(msg)
        self.is_allow_symlinks = os.environ.get("ALLOW_SYMLINKS", "false").lower() in (
            "true",
            "1",
            "yes",
        )


_settings: Settings | None = None


def get_settings() -> Settings:
    """現在の設定を返す。未初期化なら RuntimeError."""
    if _settings is None:
        msg = "Settings が初期化されていません"
        raise RuntimeError(msg)
    return _settings


def init_settings() -> Settings:
    """設定を初期化して返す."""
    global _settings
    _settings = Settings()
    return _settings
