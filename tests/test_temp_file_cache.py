"""TempFileCache のテスト."""

import threading
from pathlib import Path

from py_backend.services.temp_file_cache import TempFileCache


def test_キャッシュミスで新規ファイルが作成される(tmp_path: Path) -> None:
    cache = TempFileCache(cache_dir=tmp_path, max_size_bytes=10 * 1024 * 1024)
    key = "test-key"
    data = b"hello video data"
    path = cache.put(key, data, suffix=".mp4")

    assert path.exists()
    assert path.read_bytes() == data
    assert path.name == f"{key}.mp4"


def test_キャッシュヒットで既存ファイルのパスを返す(tmp_path: Path) -> None:
    cache = TempFileCache(cache_dir=tmp_path, max_size_bytes=10 * 1024 * 1024)
    key = "test-key"
    data = b"cached data"
    put_path = cache.put(key, data, suffix=".mp4")

    hit_path = cache.get(key)
    assert hit_path == put_path
    assert hit_path is not None
    assert hit_path.read_bytes() == data


def test_キャッシュミスでNoneを返す(tmp_path: Path) -> None:
    cache = TempFileCache(cache_dir=tmp_path, max_size_bytes=10 * 1024 * 1024)
    assert cache.get("nonexistent") is None


def test_キャッシュキーにmtimeが含まれる(tmp_path: Path) -> None:
    cache = TempFileCache(cache_dir=tmp_path)
    archive = tmp_path / "test.zip"
    key1 = cache.make_key(archive, mtime_ns=1000, entry_name="video.mp4")
    key2 = cache.make_key(archive, mtime_ns=2000, entry_name="video.mp4")
    # mtime が異なればキーも異なる (アーカイブ更新で自動無効化)
    assert key1 != key2


def test_サフィックスがファイル名に反映される(tmp_path: Path) -> None:
    cache = TempFileCache(cache_dir=tmp_path, max_size_bytes=10 * 1024 * 1024)
    path = cache.put("key1", b"data", suffix=".webm")
    assert path.suffix == ".webm"


def test_合計サイズ上限を超えると古いファイルが削除される(tmp_path: Path) -> None:
    # 上限 100 bytes
    cache = TempFileCache(cache_dir=tmp_path, max_size_bytes=100)
    # 60 bytes のファイルを2つ入れると、1つ目が追い出される
    path1 = cache.put("key1", b"x" * 60, suffix=".mp4")
    assert path1.exists()

    path2 = cache.put("key2", b"y" * 60, suffix=".mp4")
    assert path2.exists()
    assert not path1.exists()  # 追い出された
    assert cache.get("key1") is None


def test_put_with_writerでコールバック経由でファイルが作成される(
    tmp_path: Path,
) -> None:
    cache = TempFileCache(cache_dir=tmp_path, max_size_bytes=10 * 1024 * 1024)
    key = "writer-key"
    content = b"streamed video data"

    def writer(dest: Path) -> None:
        dest.write_bytes(content)

    path = cache.put_with_writer(key, writer, size_hint=len(content), suffix=".mp4")

    assert path.exists()
    assert path.read_bytes() == content
    assert path.name == f"{key}.mp4"
    # キャッシュヒットすること
    assert cache.get(key) == path


def test_put_with_writerでLRU追い出しが動作する(tmp_path: Path) -> None:
    # 上限 100 bytes
    cache = TempFileCache(cache_dir=tmp_path, max_size_bytes=100)

    def writer60(dest: Path) -> None:
        dest.write_bytes(b"x" * 60)

    path1 = cache.put_with_writer("key1", writer60, size_hint=60, suffix=".mp4")
    assert path1.exists()

    path2 = cache.put_with_writer("key2", writer60, size_hint=60, suffix=".mp4")
    assert path2.exists()
    assert not path1.exists()  # 追い出された
    assert cache.get("key1") is None


def test_put_with_writerでwriter例外時に一時ファイルが削除される(
    tmp_path: Path,
) -> None:
    cache = TempFileCache(cache_dir=tmp_path, max_size_bytes=10 * 1024 * 1024)

    def failing_writer(dest: Path) -> None:
        dest.write_bytes(b"partial")
        msg = "展開エラー"
        raise RuntimeError(msg)

    try:
        cache.put_with_writer("fail-key", failing_writer, size_hint=100, suffix=".mp4")
    except RuntimeError:
        pass

    # 一時ファイルが残っていないこと
    remaining = list(tmp_path.iterdir())
    assert len(remaining) == 0
    # キャッシュにも登録されていないこと
    assert cache.get("fail-key") is None


def test_並行書き込みでファイルが破損しない(tmp_path: Path) -> None:
    cache = TempFileCache(cache_dir=tmp_path, max_size_bytes=10 * 1024 * 1024)
    results: list[Path] = []
    errors: list[Exception] = []

    def write_entry(key: str, data: bytes) -> None:
        try:
            path = cache.put(key, data, suffix=".mp4")
            results.append(path)
        except Exception as e:
            errors.append(e)

    threads = [
        threading.Thread(target=write_entry, args=(f"key-{i}", f"data-{i}".encode()))
        for i in range(10)
    ]
    for t in threads:
        t.start()
    for t in threads:
        t.join()

    assert len(errors) == 0
    assert len(results) == 10
    for path in results:
        assert path.exists()
