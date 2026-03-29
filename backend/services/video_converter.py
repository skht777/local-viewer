"""MKV→MP4 remux サービス.

- FFmpeg stream copy で MKV を MP4 コンテナに再パッケージ
- 変換結果を TempFileCache でディスクキャッシュ
- FFmpeg 未インストール時はグレースフルに無効化
"""

import hashlib
import logging
import shutil
import subprocess
from pathlib import Path

from backend.services.extensions import REMUX_EXTENSIONS
from backend.services.temp_file_cache import TempFileCache

logger = logging.getLogger(__name__)


class VideoConverter:
    """MKV→MP4 remux サービス."""

    def __init__(self, temp_cache: TempFileCache, timeout: int = 120) -> None:
        self._temp_cache = temp_cache
        self._timeout = timeout
        self._ffmpeg_path = shutil.which("ffmpeg")

    @property
    def is_available(self) -> bool:
        """FFmpeg バイナリが利用可能か."""
        return self._ffmpeg_path is not None

    @staticmethod
    def needs_remux(ext: str) -> bool:
        """拡張子が remux 対象か判定する."""
        return ext.lower() in REMUX_EXTENSIONS

    @staticmethod
    def _make_cache_key(source: Path, mtime_ns: int) -> str:
        """remux キャッシュキーを生成する."""
        raw = f"{source}:{mtime_ns}:remux"
        return hashlib.md5(raw.encode()).hexdigest()  # noqa: S324

    def get_remuxed(self, source: Path, mtime_ns: int) -> Path | None:
        """MKV を MP4 に remux してキャッシュパスを返す.

        - キャッシュヒット時は即座にパスを返す
        - remux 失敗・FFmpeg 未インストール時は None を返す
        - run_in_threadpool で呼ぶ想定 (ブロッキング I/O)
        """
        if not self.is_available:
            return None

        key = self._make_cache_key(source, mtime_ns)

        # キャッシュヒット
        cached = self._temp_cache.get(key)
        if cached is not None:
            return cached

        # FFmpeg stream copy で remux
        def writer(dest: Path) -> None:
            self._run_ffmpeg(source, dest)

        try:
            return self._temp_cache.put_with_writer(key, writer, 0, ".mp4")
        except subprocess.CalledProcessError, subprocess.TimeoutExpired:
            logger.warning("MKV remux 失敗: %s", source, exc_info=True)
            return None

    def _run_ffmpeg(self, source: Path, dest: Path) -> None:
        """FFmpeg を実行して MKV→MP4 remux する."""
        subprocess.run(  # noqa: S603
            [
                self._ffmpeg_path or "ffmpeg",
                "-y",
                "-i",
                str(source),
                "-c",
                "copy",
                "-movflags",
                "+faststart",
                str(dest),
            ],
            timeout=self._timeout,
            capture_output=True,
            check=True,
        )
