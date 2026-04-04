"""サムネイル生成サービス.

pyvips で画像をリサイズし、TempFileCache にキャッシュする。
- 画像 → JPEG サムネイル (300px 以内、quality=80)
- アルファチャネル → 白背景で合成 (JPEG 互換)
- CPU-bound のため run_in_threadpool で呼び出すこと
"""

import hashlib
from pathlib import Path

import pyvips

from backend.services.temp_file_cache import TempFileCache

# サムネイルのデフォルト設定
DEFAULT_WIDTH = 300
JPEG_QUALITY = 80


class ThumbnailService:
    """pyvips ベースのサムネイル生成 + ディスクキャッシュ."""

    def __init__(self, temp_cache: TempFileCache) -> None:
        self._cache = temp_cache

    @staticmethod
    def make_cache_key(node_id: str, mtime_ns: int, width: int = DEFAULT_WIDTH) -> str:
        """サムネイルのキャッシュキーを生成する."""
        raw = f"thumb:{mtime_ns}:{node_id}:{width}"
        return hashlib.md5(raw.encode()).hexdigest()  # noqa: S324

    def generate_thumbnail(
        self, source_bytes: bytes, width: int = DEFAULT_WIDTH
    ) -> bytes:
        """画像バイト列からサムネイル JPEG を生成する.

        Raises:
            pyvips.Error: 画像として認識できないデータ
        """
        img = pyvips.Image.new_from_buffer(source_bytes, "")
        return self._resize_and_encode(img, width)

    def generate_thumbnail_from_path(
        self, path: Path, width: int = DEFAULT_WIDTH
    ) -> bytes:
        """ファイルパスからサムネイル JPEG を生成する.

        path_security 検証済みパスのみ渡すこと。

        Raises:
            pyvips.Error: 画像として認識できないデータ
        """
        img = pyvips.Image.new_from_file(str(path))
        return self._resize_and_encode(img, width)

    @staticmethod
    def _resize_and_encode(img: pyvips.Image, width: int) -> bytes:
        """画像をリサイズして JPEG エンコードする."""
        # アルファチャネルがあれば白背景で合成 (JPEG 互換)
        if img.hasalpha():
            img = img.flatten(background=[255, 255, 255])
        # thumbnail_image: アスペクト比を保持してリサイズ
        img = img.thumbnail_image(width, height=width)
        # progressive JPEG (interlace=True) でクライアント側の段階的表示を実現
        result: bytes = img.write_to_buffer(".jpg", Q=JPEG_QUALITY, interlace=True)
        return result

    def get_or_generate(
        self,
        source_bytes: bytes,
        cache_key: str,
        width: int = DEFAULT_WIDTH,
    ) -> Path:
        """キャッシュから取得、なければバイト列から生成してキャッシュする."""
        cached = self._cache.get(cache_key)
        if cached is not None:
            return cached

        thumb_bytes = self.generate_thumbnail(source_bytes, width)
        return self._cache.put(cache_key, thumb_bytes, suffix=".jpg")

    def get_or_generate_bytes(
        self,
        source_bytes: bytes,
        cache_key: str,
        width: int = DEFAULT_WIDTH,
    ) -> bytes:
        """キャッシュから取得、なければバイト列から生成してキャッシュし bytes を返す."""
        cached = self._cache.get(cache_key)
        if cached is not None:
            return cached.read_bytes()

        thumb_bytes = self.generate_thumbnail(source_bytes, width)
        self._cache.put(cache_key, thumb_bytes, suffix=".jpg")
        return thumb_bytes

    def get_or_generate_from_path(
        self,
        source_path: Path,
        cache_key: str,
        width: int = DEFAULT_WIDTH,
    ) -> Path:
        """キャッシュから取得、なければファイルパスから生成してキャッシュする.

        path_security 検証済みパスのみ渡すこと。
        """
        cached = self._cache.get(cache_key)
        if cached is not None:
            return cached

        thumb_bytes = self.generate_thumbnail_from_path(source_path, width)
        return self._cache.put(cache_key, thumb_bytes, suffix=".jpg")

    def get_or_generate_bytes_from_path(
        self,
        source_path: Path,
        cache_key: str,
        width: int = DEFAULT_WIDTH,
    ) -> bytes:
        """キャッシュから取得、なければパスから生成してキャッシュし bytes を返す.

        path_security 検証済みパスのみ渡すこと。
        """
        cached = self._cache.get(cache_key)
        if cached is not None:
            return cached.read_bytes()

        thumb_bytes = self.generate_thumbnail_from_path(source_path, width)
        self._cache.put(cache_key, thumb_bytes, suffix=".jpg")
        return thumb_bytes
