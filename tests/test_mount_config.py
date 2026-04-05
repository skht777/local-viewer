"""マウントポイント設定サービスのテスト."""

import json
from pathlib import Path

import pytest

from py_backend.errors import PathSecurityError
from py_backend.services.mount_config import MountConfigService, MountPoint


@pytest.fixture
def base_dir(tmp_path: Path) -> Path:
    """テスト用ベースディレクトリ."""
    base = tmp_path / "mnt-host"
    base.mkdir()
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
        assert config.version == 2

    def test_v2スキーマの設定ファイルを読み込める(
        self, service: MountConfigService, config_path: Path
    ) -> None:
        config_path.parent.mkdir(parents=True, exist_ok=True)
        config_path.write_text(
            json.dumps(
                {
                    "version": 2,
                    "mounts": [
                        {
                            "mount_id": "abc12345",
                            "name": "Photos",
                            "slug": "photos",
                            "host_path": "/mnt/d/photos",
                        }
                    ],
                }
            )
        )
        config = service.load()
        assert len(config.mounts) == 1
        assert config.mounts[0].name == "Photos"
        assert config.mounts[0].slug == "photos"
        assert config.mounts[0].host_path == "/mnt/d/photos"

    def test_v1スキーマをslugに変換して読み込める(
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
        assert config.mounts[0].slug == "photos"
        assert config.mounts[0].host_path == ""

    def test_壊れたJSONでエラーを返す(
        self, service: MountConfigService, config_path: Path
    ) -> None:
        config_path.parent.mkdir(parents=True, exist_ok=True)
        config_path.write_text("{invalid json")
        with pytest.raises(ValueError, match="設定ファイルの読み込みに失敗"):
            service.load()


class TestResolvePath:
    def test_通常のslugからパスを導出する(self, base_dir: Path) -> None:
        mount = MountPoint(mount_id="abc", name="Photos", slug="photos")
        result = mount.resolve_path(base_dir)
        assert result == (base_dir / "photos").resolve()

    def test_ドットslugはbase_dir自体を返す(self, base_dir: Path) -> None:
        mount = MountPoint(mount_id="abc", name="Root", slug=".")
        result = mount.resolve_path(base_dir)
        assert result == base_dir.resolve()

    def test_不正なslugでPathSecurityErrorを送出する(self, base_dir: Path) -> None:
        mount = MountPoint(mount_id="abc", name="Bad", slug="..")
        with pytest.raises(PathSecurityError):
            mount.resolve_path(base_dir)

    def test_スラッシュ含むslugでPathSecurityErrorを送出する(
        self, base_dir: Path
    ) -> None:
        mount = MountPoint(mount_id="abc", name="Bad", slug="a/b")
        with pytest.raises(PathSecurityError):
            mount.resolve_path(base_dir)


class TestAddMount:
    def test_マウントポイントを追加して永続化できる(
        self, service: MountConfigService
    ) -> None:
        mount = service.add_mount("Photos", "photos", "/mnt/d/photos")
        assert mount.name == "Photos"
        assert mount.slug == "photos"
        assert mount.host_path == "/mnt/d/photos"
        assert len(mount.mount_id) == 16

        # 永続化確認
        config = service.load()
        assert len(config.mounts) == 1
        assert config.mounts[0].mount_id == mount.mount_id

    def test_名前の重複を拒否する(self, service: MountConfigService) -> None:
        service.add_mount("Photos", "photos")
        with pytest.raises(ValueError, match="同名のマウントポイント"):
            service.add_mount("Photos", "videos")

    def test_重複slugのマウントを拒否する(self, service: MountConfigService) -> None:
        service.add_mount("Photos", "photos")
        with pytest.raises(ValueError, match="同じ slug"):
            service.add_mount("Photos2", "photos")

    def test_不正なslugを拒否する(self, service: MountConfigService) -> None:
        with pytest.raises(PathSecurityError):
            service.add_mount("Bad", "../escape")

    def test_空のslugを拒否する(self, service: MountConfigService) -> None:
        with pytest.raises(PathSecurityError):
            service.add_mount("Bad", "")


class TestRemoveMount:
    def test_マウントポイントを削除できる(
        self, service: MountConfigService
    ) -> None:
        mount = service.add_mount("Photos", "photos")
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
        self, service: MountConfigService
    ) -> None:
        mount = service.add_mount("Photos", "photos")
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
        assert mount.slug == "."
        assert mount.name == base_dir.name
        config = service.load()
        assert len(config.mounts) == 1
        assert config.version == 2

    def test_ROOT_DIRがbase_dirの子ディレクトリの場合(
        self, service: MountConfigService, base_dir: Path
    ) -> None:
        subdir = base_dir / "photos"
        subdir.mkdir()
        mount = service.migrate_from_root_dir(subdir)
        assert mount.slug == "photos"


class TestSave:
    def test_v2形式で保存される(
        self, service: MountConfigService, config_path: Path
    ) -> None:
        service.add_mount("Photos", "photos", "/mnt/d/photos")
        raw = json.loads(config_path.read_text())
        assert raw["version"] == 2
        assert "slug" in raw["mounts"][0]
        assert "host_path" in raw["mounts"][0]
        assert "path" not in raw["mounts"][0]
