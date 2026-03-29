"""マウントポイント設定サービスのテスト."""

import json
from pathlib import Path

import pytest

from backend.errors import PathSecurityError
from backend.services.mount_config import MountConfigService, MountPoint


@pytest.fixture
def base_dir(tmp_path: Path) -> Path:
    """テスト用ベースディレクトリ."""
    base = tmp_path / "mnt-host"
    base.mkdir()
    (base / "photos").mkdir()
    (base / "videos").mkdir()
    (base / "music").mkdir()
    return base


@pytest.fixture
def config_path(tmp_path: Path) -> Path:
    """テスト用設定ファイルパス."""
    return tmp_path / "config" / "mounts.json"


@pytest.fixture
def service(config_path: Path, base_dir: Path) -> MountConfigService:
    """テスト用 MountConfigService."""
    return MountConfigService(config_path, base_dir)


class TestLoad:
    def test_設定ファイルが存在しない場合に空のリストを返す(
        self, service: MountConfigService
    ) -> None:
        config = service.load()
        assert config.mounts == []
        assert config.version == 1

    def test_有効な設定ファイルを読み込める(
        self, service: MountConfigService, config_path: Path, base_dir: Path
    ) -> None:
        config_path.parent.mkdir(parents=True, exist_ok=True)
        config_path.write_text(
            json.dumps(
                {
                    "version": 1,
                    "mounts": [
                        {
                            "mount_id": "abc12345",
                            "name": "Photos",
                            "path": str(base_dir / "photos"),
                        }
                    ],
                }
            )
        )
        config = service.load()
        assert len(config.mounts) == 1
        assert config.mounts[0].name == "Photos"
        assert config.mounts[0].mount_id == "abc12345"

    def test_壊れたJSONでエラーを返す(
        self, service: MountConfigService, config_path: Path
    ) -> None:
        config_path.parent.mkdir(parents=True, exist_ok=True)
        config_path.write_text("{invalid json")
        with pytest.raises(ValueError, match="設定ファイルの読み込みに失敗"):
            service.load()


class TestAddMount:
    def test_マウントポイントを追加して永続化できる(
        self, service: MountConfigService, base_dir: Path
    ) -> None:
        mount = service.add_mount("Photos", str(base_dir / "photos"))
        assert mount.name == "Photos"
        assert mount.path == str((base_dir / "photos").resolve())
        assert len(mount.mount_id) == 16

        # 永続化確認
        config = service.load()
        assert len(config.mounts) == 1
        assert config.mounts[0].mount_id == mount.mount_id

    def test_名前の重複を拒否する(
        self, service: MountConfigService, base_dir: Path
    ) -> None:
        service.add_mount("Photos", str(base_dir / "photos"))
        with pytest.raises(ValueError, match="同名のマウントポイント"):
            service.add_mount("Photos", str(base_dir / "videos"))

    def test_重複パスのマウントを拒否する(
        self, service: MountConfigService, base_dir: Path
    ) -> None:
        service.add_mount("Photos", str(base_dir / "photos"))
        with pytest.raises(ValueError, match="既に登録されています"):
            service.add_mount("Photos2", str(base_dir / "photos"))

    def test_MOUNT_BASE_DIR外のパスを拒否する(
        self, service: MountConfigService, tmp_path: Path
    ) -> None:
        outside = tmp_path / "outside"
        outside.mkdir()
        with pytest.raises(PathSecurityError, match="MOUNT_BASE_DIR"):
            service.add_mount("Outside", str(outside))

    def test_存在しないディレクトリを拒否する(
        self, service: MountConfigService, base_dir: Path
    ) -> None:
        with pytest.raises(PathSecurityError, match="ディレクトリが存在しません"):
            service.add_mount("Nonexistent", str(base_dir / "nonexistent"))

    def test_親子関係のマウントを拒否する(
        self, service: MountConfigService, base_dir: Path
    ) -> None:
        # 親を先に登録
        service.add_mount("Base", str(base_dir))
        # その子を追加しようとする
        with pytest.raises(ValueError, match="親子関係"):
            service.add_mount("Photos", str(base_dir / "photos"))

    def test_子を先に登録した後に親を追加しようとすると拒否する(
        self, service: MountConfigService, base_dir: Path
    ) -> None:
        service.add_mount("Photos", str(base_dir / "photos"))
        with pytest.raises(ValueError, match="親子関係"):
            service.add_mount("Base", str(base_dir))


class TestRemoveMount:
    def test_マウントポイントを削除できる(
        self, service: MountConfigService, base_dir: Path
    ) -> None:
        mount = service.add_mount("Photos", str(base_dir / "photos"))
        service.remove_mount(mount.mount_id)
        config = service.load()
        assert len(config.mounts) == 0

    def test_存在しないmount_idで削除するとエラー(
        self, service: MountConfigService
    ) -> None:
        with pytest.raises(ValueError, match="見つかりません"):
            service.remove_mount("nonexistent")


class TestUpdateMount:
    def test_マウントポイント名を更新できる(
        self, service: MountConfigService, base_dir: Path
    ) -> None:
        mount = service.add_mount("Photos", str(base_dir / "photos"))
        updated = service.update_mount(mount.mount_id, name="My Photos")
        assert updated.name == "My Photos"
        assert updated.mount_id == mount.mount_id

        config = service.load()
        assert config.mounts[0].name == "My Photos"

    def test_存在しないmount_idで更新するとエラー(
        self, service: MountConfigService
    ) -> None:
        with pytest.raises(ValueError, match="見つかりません"):
            service.update_mount("nonexistent", name="New Name")


class TestROOTDIRマイグレーション:
    def test_ROOT_DIRからの自動マイグレーション(
        self, service: MountConfigService, base_dir: Path
    ) -> None:
        mount = service.migrate_from_root_dir(base_dir)
        assert mount.name == base_dir.name
        assert mount.path == str(base_dir.resolve())
        config = service.load()
        assert len(config.mounts) == 1
