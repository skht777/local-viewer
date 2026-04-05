"""アーカイブ展開ファイルのディスクキャッシュ.

- 大きなエントリ (動画等) をディスクにキャッシュし Range Request に対応する
- キャッシュキー: archive_path + mtime_ns + entry_name → MD5 ハッシュ
- 合計サイズ上限で最古ファイルを削除する簡易 LRU
- アトミック書き込み: tmpfile → os.rename (並行リクエストの競合防止)
- threading.Lock でスレッドセーフ
"""

import hashlib
import os
import tempfile
import threading
from collections import OrderedDict
from collections.abc import Callable
from pathlib import Path


class TempFileCache:
    """ディスクベースの LRU キャッシュ."""

    def __init__(
        self,
        cache_dir: Path | None = None,
        max_size_bytes: int = 1024 * 1024 * 1024,
    ) -> None:
        if cache_dir is None:
            cache_dir = Path(tempfile.gettempdir()) / "viewer-disk-cache"
        self._cache_dir = cache_dir
        self._cache_dir.mkdir(parents=True, exist_ok=True)
        self._max_size_bytes = max_size_bytes
        # key → (path, size) の LRU 順序マップ
        self._entries: OrderedDict[str, tuple[Path, int]] = OrderedDict()
        self._current_bytes = 0
        self._lock = threading.Lock()

    def make_key(self, archive_path: Path, mtime_ns: int, entry_name: str) -> str:
        """キャッシュキーを生成する (MD5 ハッシュ)."""
        raw = f"{archive_path}:{mtime_ns}:{entry_name}"
        return hashlib.md5(raw.encode()).hexdigest()  # noqa: S324

    def get(self, key: str) -> Path | None:
        """キャッシュヒットならファイルパスを返す。ミスなら None."""
        with self._lock:
            if key not in self._entries:
                return None
            path, _size = self._entries[key]
            if not path.exists():
                # ファイルが消えている場合はエントリを削除
                self._current_bytes -= _size
                del self._entries[key]
                return None
            self._entries.move_to_end(key)
            return path

    def put(self, key: str, data: bytes, suffix: str = "") -> Path:
        """データをディスクに書き込みキャッシュに登録する.

        アトミック書き込み: tmpfile → os.rename で競合を防止。
        """
        size = len(data)
        final_name = f"{key}{suffix}"
        final_path = self._cache_dir / final_name

        # 一時ファイルに書き込み → rename でアトミックに配置
        fd, tmp_path_str = tempfile.mkstemp(dir=self._cache_dir, prefix=f".tmp_{key}_")
        try:
            os.write(fd, data)
        finally:
            os.close(fd)
        try:
            os.rename(tmp_path_str, final_path)
        except BaseException:
            Path(tmp_path_str).unlink(missing_ok=True)
            raise

        with self._lock:
            # 既存エントリがあれば先に削除
            if key in self._entries:
                old_path, old_size = self._entries[key]
                self._current_bytes -= old_size
                del self._entries[key]
                if old_path != final_path:
                    old_path.unlink(missing_ok=True)

            # LRU 追い出し
            self._evict_if_needed(size)

            self._entries[key] = (final_path, size)
            self._current_bytes += size

        return final_path

    def put_with_writer(
        self,
        key: str,
        writer: Callable[[Path], None],
        size_hint: int,
        suffix: str = "",
    ) -> Path:
        """コールバックでファイルに書き込み、キャッシュに登録する.

        - size_hint で事前に LRU 追い出しを実行 (近似値で良い)
        - writer(tmp_path) を呼び、一時ファイルに書き込ませる
        - 書き込み完了後に stat で実サイズを取得し、LRU に登録
        - アトミック os.replace() → 最終パスに配置
        - writer 例外時は一時ファイルを確実に削除
        """
        final_name = f"{key}{suffix}"
        final_path = self._cache_dir / final_name

        # 一時ファイルを作成 (writer が Path を受け取って書き込む)
        fd, tmp_path_str = tempfile.mkstemp(dir=self._cache_dir, prefix=f".tmp_{key}_")
        os.close(fd)
        tmp_path = Path(tmp_path_str)

        try:
            writer(tmp_path)
            actual_size = tmp_path.stat().st_size
            os.replace(tmp_path_str, final_path)
        except BaseException:
            tmp_path.unlink(missing_ok=True)
            raise

        with self._lock:
            # 既存エントリがあれば先に削除
            if key in self._entries:
                old_path, old_size = self._entries[key]
                self._current_bytes -= old_size
                del self._entries[key]
                if old_path != final_path:
                    old_path.unlink(missing_ok=True)

            # LRU 追い出し
            self._evict_if_needed(actual_size)

            self._entries[key] = (final_path, actual_size)
            self._current_bytes += actual_size

        return final_path

    def _evict_if_needed(self, incoming_size: int) -> None:
        """合計サイズ上限を超える場合、最古のエントリを削除する.

        ロック保持中に呼び出されること。
        """
        while (
            self._current_bytes + incoming_size > self._max_size_bytes and self._entries
        ):
            _key, (path, size) = self._entries.popitem(last=False)
            self._current_bytes -= size
            path.unlink(missing_ok=True)
