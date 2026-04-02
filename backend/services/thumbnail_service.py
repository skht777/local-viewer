"""サムネイル生成サービス.

Pillow で画像をリサイズし、TempFileCache にキャッシュする。
- 画像 → JPEG サムネイル (300px 以内、quality=80)
- RGBA/P/LA → RGB 変換 (JPEG 互換)
- CPU-bound のため run_in_threadpool で呼び出すこと
"""

import hashlib
import io
from pathlib import Path

from PIL import Image

from backend.services.temp_file_cache import TempFileCache

# サムネイルのデフォルト設定
DEFAULT_WIDTH = 300
JPEG_QUALITY = 80


class ThumbnailService:
    """Pillow ベースのサムネイル生成 + ディスクキャッシュ."""

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
            PIL.UnidentifiedImageError: 画像として認識できないデータ
        """
        img: Image.Image = Image.open(io.BytesIO(source_bytes))
        return self._resize_and_encode(img, width)

    def generate_thumbnail_from_path(
        self, path: Path, width: int = DEFAULT_WIDTH
    ) -> bytes:
        """ファイルパスからサムネイル JPEG を生成する.

        Image.open(path) で遅延読み込みを利用し、メモリ効率を向上させる。
        path_security 検証済みパスのみ渡すこと。

        Raises:
            PIL.UnidentifiedImageError: 画像として認識できないデータ
        """
        img: Image.Image = Image.open(path)
        return self._resize_and_encode(img, width)

    @staticmethod
    def _resize_and_encode(img: Image.Image, width: int) -> bytes:
        """画像をリサイズして JPEG エンコードする."""
        # JPEG 非互換モードを RGB に変換
        if img.mode in ("RGBA", "P", "LA"):
            img = img.convert("RGB")
        # reducing_gap=2.0: libjpeg 整数ダウンスケールを活用して高速化
        # BILINEAR: 300px では LANCZOS との差は視認不可能
        img.thumbnail((width, width), Image.Resampling.BILINEAR, reducing_gap=2.0)
        buf = io.BytesIO()
        img.save(
            buf, format="JPEG", quality=JPEG_QUALITY, optimize=True, progressive=True
        )
        return buf.getvalue()

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
