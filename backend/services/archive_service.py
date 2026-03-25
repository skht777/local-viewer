"""アーカイブ操作の統合サービス.

- 複数の ArchiveReader を保持し、拡張子に応じて適切なリーダーを選択
- メモリ LRU キャッシュでエントリデータをバイト上限管理
- list_entries はキャッシュしない (毎回最新を読む)
- extract_entry の結果をキャッシュ
"""

import threading
from collections import OrderedDict
from pathlib import Path

from backend.services.archive_reader import (
    ArchiveEntry,
    ArchiveReader,
    RarArchiveReader,
    SevenZipArchiveReader,
    ZipArchiveReader,
)
from backend.services.archive_security import ArchiveEntryValidator
from backend.services.extensions import VIDEO_EXTENSIONS


class ByteLRUCache:
    """バイト上限付き LRU キャッシュ.

    - threading.Lock() でスレッドセーフ
    - OrderedDict ベース、バイト上限で LRU 追い出し
    - キャッシュキーは呼び出し側が生成
    """

    def __init__(self, max_bytes: int) -> None:
        self._data: OrderedDict[str, bytes] = OrderedDict()
        self._current_bytes = 0
        self._max_bytes = max_bytes
        self._lock = threading.Lock()

    def get(self, key: str) -> bytes | None:
        with self._lock:
            if key in self._data:
                self._data.move_to_end(key)
                return self._data[key]
            return None

    def put(self, key: str, value: bytes) -> None:
        size = len(value)
        with self._lock:
            # 既存キーなら先に削除
            if key in self._data:
                self._current_bytes -= len(self._data[key])
                del self._data[key]

            # LRU 追い出し
            while self._current_bytes + size > self._max_bytes and self._data:
                _, evicted = self._data.popitem(last=False)
                self._current_bytes -= len(evicted)

            self._data[key] = value
            self._current_bytes += size

    @property
    def current_bytes(self) -> int:
        return self._current_bytes


class ArchiveService:
    """アーカイブ操作の統合サービス.

    - 複数の ArchiveReader を保持し、拡張子に応じて選択
    - extract_entry の結果を ByteLRUCache でキャッシュ
    """

    def __init__(
        self,
        validator: ArchiveEntryValidator,
        cache_max_bytes: int = 256 * 1024 * 1024,
    ) -> None:
        self._readers: list[ArchiveReader] = [
            ZipArchiveReader(validator),
            RarArchiveReader(validator),
            SevenZipArchiveReader(validator),
        ]
        self._cache = ByteLRUCache(max_bytes=cache_max_bytes)

    def get_reader(self, path: Path) -> ArchiveReader | None:
        """パスに対応するリーダーを返す."""
        for reader in self._readers:
            if reader.supports(path):
                return reader
        return None

    def list_entries(self, archive_path: Path) -> list[ArchiveEntry]:
        """アーカイブのエントリ一覧を返す."""
        reader = self.get_reader(archive_path)
        if reader is None:
            msg = f"サポートされていないアーカイブ形式です: {archive_path.suffix}"
            raise ValueError(msg)
        return reader.list_entries(archive_path)

    def extract_entry(self, archive_path: Path, entry_name: str) -> bytes:
        """エントリを抽出する (キャッシュ付き).

        キャッシュキー: "{archive_path}:{mtime_ns}:{entry_name}"
        動画エントリはメモリキャッシュをバイパスする (OOM 防止)。
        """
        # 動画エントリはメモリキャッシュをスキップ
        if self._is_video_entry(entry_name):
            return self._extract_raw(archive_path, entry_name)

        # キャッシュキー生成 (mtime_ns でアーカイブ更新時に自動無効化)
        stat = archive_path.stat()
        cache_key = f"{archive_path}:{stat.st_mtime_ns}:{entry_name}"

        # キャッシュヒット確認
        cached = self._cache.get(cache_key)
        if cached is not None:
            return cached

        # キャッシュミス: リーダーで抽出
        data = self._extract_raw(archive_path, entry_name)

        # キャッシュに格納
        self._cache.put(cache_key, data)
        return data

    def _extract_raw(self, archive_path: Path, entry_name: str) -> bytes:
        """リーダーで直接抽出する (キャッシュなし)."""
        reader = self.get_reader(archive_path)
        if reader is None:
            msg = f"サポートされていないアーカイブ形式です: {archive_path.suffix}"
            raise ValueError(msg)
        return reader.extract_entry(archive_path, entry_name)

    @staticmethod
    def _is_video_entry(entry_name: str) -> bool:
        """動画拡張子かどうかを判定する."""
        dot_idx = entry_name.rfind(".")
        if dot_idx <= 0:
            return False
        return entry_name[dot_idx:].lower() in VIDEO_EXTENSIONS

    def is_supported(self, path: Path) -> bool:
        """アーカイブとしてサポートされるか."""
        return self.get_reader(path) is not None

    def get_diagnostics(self) -> dict[str, bool]:
        """各形式の利用可否を返す (起動時診断用)."""
        diag: dict[str, bool] = {}
        for reader in self._readers:
            if isinstance(reader, ZipArchiveReader):
                diag["zip"] = True  # 標準ライブラリなので常に利用可能
            elif isinstance(reader, RarArchiveReader):
                diag["rar"] = reader.is_available
            elif isinstance(reader, SevenZipArchiveReader):
                # py7zr は Pure Python だが _zstd 問題がありうる
                try:
                    import py7zr  # noqa: F401

                    diag["7z"] = True
                except ImportError:
                    diag["7z"] = False
        return diag
