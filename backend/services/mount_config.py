"""マウントポイント設定の管理.

mounts.json の読み書きとマウントポイントの CRUD を提供する。
パス検証は PathSecurity.validate_mount_path() に委譲する。
"""

from __future__ import annotations

import json
import uuid
from dataclasses import asdict, dataclass
from pathlib import Path

from backend.services.path_security import PathSecurity


@dataclass
class MountPoint:
    """マウントポイント定義.

    - mount_id: UUID v4 hex 16文字の安定識別子
    - name: 表示名
    - path: コンテナ内の絶対パス (resolve 済み)
    """

    mount_id: str
    name: str
    path: str


@dataclass
class MountConfig:
    """マウントポイント設定全体.

    - version: スキーマバージョン
    - mounts: マウントポイントのリスト
    """

    version: int
    mounts: list[MountPoint]


class MountConfigService:
    """マウントポイント設定の CRUD + JSON 永続化.

    - config_path: mounts.json の保存先
    - base_dir: マウント許可親ディレクトリ (MOUNT_BASE_DIR)
    """

    def __init__(self, config_path: Path, base_dir: Path) -> None:
        self._config_path = config_path
        self._base_dir = base_dir.resolve()

    def load(self) -> MountConfig:
        """設定ファイルを読み込む.

        ファイルが存在しない場合は空の設定を返す。
        """
        if not self._config_path.exists():
            return MountConfig(version=1, mounts=[])

        try:
            raw = json.loads(self._config_path.read_text())
        except (json.JSONDecodeError, OSError) as exc:
            msg = f"設定ファイルの読み込みに失敗: {exc}"
            raise ValueError(msg) from exc

        mounts = [
            MountPoint(
                mount_id=m["mount_id"],
                name=m["name"],
                path=m["path"],
            )
            for m in raw.get("mounts", [])
        ]
        return MountConfig(version=raw.get("version", 1), mounts=mounts)

    def save(self, config: MountConfig) -> None:
        """設定をファイルに書き込む."""
        self._config_path.parent.mkdir(parents=True, exist_ok=True)
        data = {
            "version": config.version,
            "mounts": [asdict(m) for m in config.mounts],
        }
        self._config_path.write_text(
            json.dumps(data, ensure_ascii=False, indent=2) + "\n"
        )

    def add_mount(self, name: str, path: str) -> MountPoint:
        """マウントポイントを追加する.

        - PathSecurity でパス検証
        - 名前・パスの重複チェック
        - 親子関係チェック
        - mounts.json に永続化
        """
        resolved = PathSecurity.validate_mount_path(Path(path), self._base_dir)
        resolved_str = str(resolved)

        config = self.load()

        # 名前の重複チェック
        for m in config.mounts:
            if m.name == name:
                msg = f"同名のマウントポイントが既に存在します: {name}"
                raise ValueError(msg)

        # パスの重複チェック
        for m in config.mounts:
            if m.path == resolved_str:
                msg = f"同じパスが既に登録されています: {resolved_str}"
                raise ValueError(msg)

        # 親子関係チェック
        self._validate_no_overlap(resolved, config.mounts)

        mount = MountPoint(
            mount_id=uuid.uuid4().hex[:16],
            name=name,
            path=resolved_str,
        )
        config.mounts.append(mount)
        self.save(config)
        return mount

    def remove_mount(self, mount_id: str) -> None:
        """マウントポイントを削除する."""
        config = self.load()
        original_len = len(config.mounts)
        config.mounts = [m for m in config.mounts if m.mount_id != mount_id]
        if len(config.mounts) == original_len:
            msg = f"マウントポイントが見つかりません: {mount_id}"
            raise ValueError(msg)
        self.save(config)

    def update_mount(self, mount_id: str, *, name: str | None = None) -> MountPoint:
        """マウントポイントを更新する (名前のみ変更可能)."""
        config = self.load()
        for m in config.mounts:
            if m.mount_id == mount_id:
                if name is not None:
                    m.name = name
                self.save(config)
                return m
        msg = f"マウントポイントが見つかりません: {mount_id}"
        raise ValueError(msg)

    def migrate_from_root_dir(self, root_dir: Path) -> MountPoint:
        """ROOT_DIR からの自動マイグレーション.

        ROOT_DIR をそのまま1つのマウントポイントとして登録する。
        """
        resolved = root_dir.resolve()
        mount = MountPoint(
            mount_id=uuid.uuid4().hex[:16],
            name=resolved.name,
            path=str(resolved),
        )
        config = MountConfig(version=1, mounts=[mount])
        self.save(config)
        return mount

    def _validate_no_overlap(
        self,
        path: Path,
        existing: list[MountPoint],
        exclude_id: str | None = None,
    ) -> None:
        """既存マウントとの親子関係を検証する.

        path が既存マウントの親または子になっていないことを確認。
        """
        path_str = str(path)
        path_prefix = path_str + "/"
        for m in existing:
            if exclude_id and m.mount_id == exclude_id:
                continue
            m_str = m.path
            m_prefix = m_str + "/"
            # path が既存の子 or 親
            if path_str.startswith(m_prefix) or m_str.startswith(path_prefix):
                msg = f"親子関係のマウントポイントは登録できません: {path} と {m.path}"
                raise ValueError(msg)
            # 完全一致 (重複パスチェックで弾かれるが念のため)
            if path_str == m_str:
                msg = f"同じパスが既に登録されています: {path_str}"
                raise ValueError(msg)
