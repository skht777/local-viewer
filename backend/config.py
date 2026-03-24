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

    # アーカイブ安全性設定
    archive_max_total_size: int  # 展開後合計サイズ上限 (bytes)
    archive_max_entry_size: int  # 1エントリ展開後サイズ上限 (bytes)
    archive_max_ratio: float  # 圧縮率上限
    archive_max_video_entry_size: int  # 動画1エントリ展開後サイズ上限 (bytes)
    archive_cache_mb: int  # メモリキャッシュ容量 (MB)
    archive_disk_cache_mb: int  # ディスクキャッシュ容量 (MB)
    archive_registry_max_entries: int  # NodeRegistry アーカイブエントリ上限

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

        # アーカイブ設定
        self.archive_max_total_size = int(
            os.environ.get("ARCHIVE_MAX_TOTAL_SIZE", str(1024 * 1024 * 1024))
        )
        self.archive_max_entry_size = int(
            os.environ.get("ARCHIVE_MAX_ENTRY_SIZE", str(32 * 1024 * 1024))
        )
        self.archive_max_video_entry_size = int(
            os.environ.get("ARCHIVE_MAX_VIDEO_ENTRY_SIZE", str(500 * 1024 * 1024))
        )
        self.archive_max_ratio = float(os.environ.get("ARCHIVE_MAX_RATIO", "100.0"))
        self.archive_cache_mb = int(os.environ.get("ARCHIVE_CACHE_MB", "256"))
        self.archive_disk_cache_mb = int(
            os.environ.get("ARCHIVE_DISK_CACHE_MB", "1024")
        )
        self.archive_registry_max_entries = int(
            os.environ.get("ARCHIVE_REGISTRY_MAX_ENTRIES", "100000")
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
