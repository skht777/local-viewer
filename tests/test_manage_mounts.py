"""manage_mounts.py TUI のテスト."""

import os
from pathlib import Path
from unittest.mock import patch

import pytest

from manage_mounts import _get_service, main


@pytest.fixture
def base_dir(tmp_path: Path) -> Path:
    """テスト用ベースディレクトリ."""
    base = tmp_path / "mnt-host"
    base.mkdir()
    (base / "photos").mkdir()
    return base


@pytest.fixture
def config_path(tmp_path: Path) -> Path:
    """テスト用設定ファイルパス."""
    return tmp_path / "config" / "mounts.json"


@pytest.fixture
def _env(base_dir: Path, config_path: Path) -> None:
    """テスト用環境変数を設定."""
    with patch.dict(
        os.environ,
        {
            "MOUNT_BASE_DIR": str(base_dir),
            "MOUNT_CONFIG_PATH": str(config_path),
        },
    ):
        yield


@pytest.mark.usefixtures("_env")
def test_get_serviceでサービスを取得できる() -> None:
    service = _get_service()
    assert service is not None


@pytest.mark.usefixtures("_env")
def test_追加して終了する(base_dir: Path, config_path: Path) -> None:
    # "a" → パス入力 → 名前入力 → "q" で終了
    inputs = [
        "a",
        str(base_dir / "photos"),
        "My Photos",
        "q",
    ]
    with patch("builtins.input", side_effect=inputs):
        main()

    # 設定ファイルが作成されたことを確認
    service = _get_service()
    config = service.load()
    assert len(config.mounts) == 1
    assert config.mounts[0].name == "My Photos"


@pytest.mark.usefixtures("_env")
def test_即座に終了する() -> None:
    with patch("builtins.input", side_effect=["q"]):
        main()  # 例外なく終了


def test_MOUNT_BASE_DIRが未設定でエラー() -> None:
    with (
        patch.dict(os.environ, {}, clear=True),
        pytest.raises(SystemExit),
    ):
        _get_service()
