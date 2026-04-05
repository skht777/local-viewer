"""VideoConverter のユニットテスト."""

import subprocess
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from backend.services.temp_file_cache import TempFileCache
from backend.services.video_converter import VideoConverter


@pytest.fixture
def temp_cache(tmp_path: Path) -> TempFileCache:
    """テスト用 TempFileCache."""
    return TempFileCache(cache_dir=tmp_path / "cache", max_size_bytes=100 * 1024 * 1024)


class TestIsAvailable:
    """FFmpeg 可用性チェック."""

    def test_FFmpegが見つかればis_availableがTrueを返す(
        self, temp_cache: TempFileCache
    ) -> None:
        with patch("shutil.which", return_value="/usr/bin/ffmpeg"):
            converter = VideoConverter(temp_cache=temp_cache, timeout=30)
        assert converter.is_available is True

    def test_FFmpegが見つからなければis_availableがFalseを返す(
        self, temp_cache: TempFileCache
    ) -> None:
        with patch("shutil.which", return_value=None):
            converter = VideoConverter(temp_cache=temp_cache, timeout=30)
        assert converter.is_available is False


class TestNeedsRemux:
    """remux 対象判定."""

    @pytest.mark.parametrize("ext", [".mkv", ".MKV", ".Mkv"])
    def test_needs_remuxがmkvでTrueを返す(self, ext: str) -> None:
        assert VideoConverter.needs_remux(ext) is True

    @pytest.mark.parametrize("ext", [".mp4", ".webm", ".avi", ".mov", ".pdf"])
    def test_needs_remuxがmp4等でFalseを返す(self, ext: str) -> None:
        assert VideoConverter.needs_remux(ext) is False


class TestGetRemuxed:
    """remux 実行とキャッシュ."""

    def test_remux成功でキャッシュファイルパスを返す(
        self, temp_cache: TempFileCache, tmp_path: Path
    ) -> None:
        source = tmp_path / "test.mkv"
        source.write_bytes(b"fake mkv data")

        with patch("shutil.which", return_value="/usr/bin/ffmpeg"):
            converter = VideoConverter(temp_cache=temp_cache, timeout=30)

        # subprocess.run をモック — dest にダミーデータを書き込む
        def mock_run(cmd: list[str], **kwargs: object) -> MagicMock:
            dest_path = Path(cmd[-1])
            dest_path.write_bytes(b"fake mp4 data")
            return MagicMock(returncode=0)

        with patch("subprocess.run", side_effect=mock_run):
            result = converter.get_remuxed(source, source.stat().st_mtime_ns)

        assert result is not None
        assert result.exists()
        assert result.read_bytes() == b"fake mp4 data"

    def test_remux失敗でNoneを返す(
        self, temp_cache: TempFileCache, tmp_path: Path
    ) -> None:
        source = tmp_path / "test.mkv"
        source.write_bytes(b"fake mkv data")

        with patch("shutil.which", return_value="/usr/bin/ffmpeg"):
            converter = VideoConverter(temp_cache=temp_cache, timeout=30)

        with patch(
            "subprocess.run",
            side_effect=subprocess.CalledProcessError(1, "ffmpeg", stderr=b"error"),
        ):
            result = converter.get_remuxed(source, source.stat().st_mtime_ns)

        assert result is None

    def test_タイムアウトでNoneを返す(
        self, temp_cache: TempFileCache, tmp_path: Path
    ) -> None:
        source = tmp_path / "test.mkv"
        source.write_bytes(b"fake mkv data")

        with patch("shutil.which", return_value="/usr/bin/ffmpeg"):
            converter = VideoConverter(temp_cache=temp_cache, timeout=30)

        with patch(
            "subprocess.run",
            side_effect=subprocess.TimeoutExpired("ffmpeg", 30),
        ):
            result = converter.get_remuxed(source, source.stat().st_mtime_ns)

        assert result is None

    def test_キャッシュヒットでFFmpegを呼ばない(
        self, temp_cache: TempFileCache, tmp_path: Path
    ) -> None:
        source = tmp_path / "test.mkv"
        source.write_bytes(b"fake mkv data")
        mtime_ns = source.stat().st_mtime_ns

        with patch("shutil.which", return_value="/usr/bin/ffmpeg"):
            converter = VideoConverter(temp_cache=temp_cache, timeout=30)

        # 1回目: remux 実行
        def mock_run(cmd: list[str], **kwargs: object) -> MagicMock:
            dest_path = Path(cmd[-1])
            dest_path.write_bytes(b"fake mp4 data")
            return MagicMock(returncode=0)

        with patch("subprocess.run", side_effect=mock_run) as mock_subprocess:
            converter.get_remuxed(source, mtime_ns)
            assert mock_subprocess.call_count == 1

        # 2回目: キャッシュヒット
        with patch("subprocess.run") as mock_subprocess:
            result = converter.get_remuxed(source, mtime_ns)
            mock_subprocess.assert_not_called()

        assert result is not None
        assert result.exists()

    def test_is_availableがFalseの場合Noneを返す(
        self, temp_cache: TempFileCache, tmp_path: Path
    ) -> None:
        source = tmp_path / "test.mkv"
        source.write_bytes(b"fake mkv data")

        with patch("shutil.which", return_value=None):
            converter = VideoConverter(temp_cache=temp_cache, timeout=30)

        result = converter.get_remuxed(source, source.stat().st_mtime_ns)
        assert result is None


class TestExtractFrame:
    """フレーム抽出."""

    def test_extract_frameがJPEGバイト列を返す(
        self, temp_cache: TempFileCache, tmp_path: Path
    ) -> None:
        source = tmp_path / "test.mp4"
        source.write_bytes(b"fake mp4 data")

        with patch("shutil.which", return_value="/usr/bin/ffmpeg"):
            converter = VideoConverter(temp_cache=temp_cache, timeout=30)

        fake_jpeg = b"\xff\xd8\xff\xe0" + b"\x00" * 100
        mock_result = MagicMock(stdout=fake_jpeg)

        with patch("subprocess.run", return_value=mock_result):
            result = converter.extract_frame(source, seek_seconds=1, timeout=10)

        assert result is not None
        assert result == fake_jpeg

    def test_extract_frameが失敗時にNoneを返す(
        self, temp_cache: TempFileCache, tmp_path: Path
    ) -> None:
        source = tmp_path / "test.mp4"
        source.write_bytes(b"fake mp4 data")

        with patch("shutil.which", return_value="/usr/bin/ffmpeg"):
            converter = VideoConverter(temp_cache=temp_cache, timeout=30)

        with patch(
            "subprocess.run",
            side_effect=subprocess.CalledProcessError(1, "ffmpeg", stderr=b"error"),
        ):
            result = converter.extract_frame(source)

        assert result is None

    def test_extract_frameがタイムアウト時にNoneを返す(
        self, temp_cache: TempFileCache, tmp_path: Path
    ) -> None:
        source = tmp_path / "test.mp4"
        source.write_bytes(b"fake mp4 data")

        with patch("shutil.which", return_value="/usr/bin/ffmpeg"):
            converter = VideoConverter(temp_cache=temp_cache, timeout=30)

        with patch(
            "subprocess.run",
            side_effect=subprocess.TimeoutExpired("ffmpeg", 10),
        ):
            result = converter.extract_frame(source, timeout=10)

        assert result is None

    def test_FFmpeg未インストール時にextract_frameがNoneを返す(
        self, temp_cache: TempFileCache, tmp_path: Path
    ) -> None:
        source = tmp_path / "test.mp4"
        source.write_bytes(b"fake mp4 data")

        with patch("shutil.which", return_value=None):
            converter = VideoConverter(temp_cache=temp_cache, timeout=30)

        result = converter.extract_frame(source)
        assert result is None
