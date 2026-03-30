"""アプリケーション設定.

環境変数からの設定読み込みを一元管理する。
"""

import os
from pathlib import Path


class Settings:
    """アプリケーション設定.

    - mount_base_dir: マウント許可親ディレクトリ (MOUNT_BASE_DIR)
    - is_allow_symlinks: symlink 追跡の許可フラグ
    """

    is_allow_symlinks: bool

    # アーカイブ安全性設定
    archive_max_total_size: int  # 展開後合計サイズ上限 (bytes)
    archive_max_entry_size: int  # 1エントリ展開後サイズ上限 (bytes)
    archive_max_ratio: float  # 圧縮率上限
    archive_max_video_entry_size: int  # 動画1エントリ展開後サイズ上限 (bytes)
    archive_cache_mb: int  # メモリキャッシュ容量 (MB)
    archive_disk_cache_mb: int  # ディスクキャッシュ容量 (MB)
    archive_registry_max_entries: int  # NodeRegistry アーカイブエントリ上限

    # 動画変換設定
    video_remux_timeout: int  # FFmpeg remux タイムアウト (秒)

    # マウントポイント設定
    mount_base_dir: Path  # マウント許可親ディレクトリ (MOUNT_BASE_DIR)
    mount_config_path: str  # mounts.json パス

    # 検索/インデックス設定
    index_db_path: str  # SQLite FTS5 インデックス DB パス
    watch_mode: str  # ファイル監視モード (auto, native, polling)
    watch_poll_interval: int  # polling モードの間隔 (秒)
    rebuild_rate_limit_seconds: int  # rebuild API のレート制限 (秒)
    search_max_results: int  # 検索結果の最大件数
    search_query_timeout: int  # 検索クエリのタイムアウト (秒)

    def __init__(self) -> None:
        # マウントポイント設定 (MOUNT_BASE_DIR 必須)
        mount_base = os.environ.get("MOUNT_BASE_DIR", "")
        if not mount_base:
            msg = "MOUNT_BASE_DIR を設定してください"
            raise ValueError(msg)
        self.mount_base_dir: Path = Path(mount_base).resolve()
        self.mount_config_path = os.environ.get(
            "MOUNT_CONFIG_PATH", "config/mounts.json"
        )
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

        # 動画変換設定
        self.video_remux_timeout = int(os.environ.get("VIDEO_REMUX_TIMEOUT", "120"))

        # 検索/インデックス設定
        self.index_db_path = os.environ.get(
            "INDEX_DB_PATH",
            "/tmp/viewer-index.db",  # noqa: S108  # エフェメラル DB、Docker では tmpfs 上
        )
        self.watch_mode = os.environ.get("WATCH_MODE", "auto")
        self.watch_poll_interval = int(os.environ.get("WATCH_POLL_INTERVAL", "30"))
        self.rebuild_rate_limit_seconds = int(
            os.environ.get("REBUILD_RATE_LIMIT_SECONDS", "60")
        )
        self.search_max_results = int(os.environ.get("SEARCH_MAX_RESULTS", "200"))
        self.search_query_timeout = int(os.environ.get("SEARCH_QUERY_TIMEOUT", "5"))
        self.scan_workers = int(os.environ.get("SCAN_WORKERS", "8"))


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
