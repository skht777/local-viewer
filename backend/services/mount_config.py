"""マウントポイント設定の管理.

mounts.json の読み書きとマウントポイントの CRUD を提供する。
slug のバリデーションは PathSecurity.validate_slug() に委譲する。

スキーマ:
  v1: mount_id, name, path (コンテナ内絶対パス)
  v2: mount_id, name, slug (MOUNT_BASE_DIR からの相対名), host_path
"""

from __future__ import annotations

import json
import os
import uuid
from dataclasses import dataclass
from pathlib import Path

from backend.errors import PathSecurityError
from backend.services.path_security import PathSecurity


@dataclass
class MountPoint:
    """マウントポイント定義.

    - mount_id: UUID v4 hex 16文字の安定識別子
    - name: 表示名
    - slug: MOUNT_BASE_DIR からの相対ディレクトリ名
    - host_path: ホスト側パス (バックエンドでは不使用、manage_mounts.sh 用)
    """

    mount_id: str
    name: str
    slug: str
    host_path: str = ""

    def resolve_path(self, base_dir: Path) -> Path:
        """slug からコンテナ内の絶対パスを導出する.

        - "." は base_dir 自体を返す (ROOT_DIR マイグレーション互換)
        - それ以外は validate_slug() で安全性を検証した上で base_dir / slug を返す
        - resolve 後が base_dir 配下であることを防御的に確認
        """
        if self.slug == ".":
            return base_dir.resolve()
        PathSecurity.validate_slug(self.slug)
        resolved = (base_dir / self.slug).resolve()
        base_resolved = base_dir.resolve()
        base_str = str(base_resolved)
        resolved_str = str(resolved)
        if resolved_str != base_str and not resolved_str.startswith(base_str + os.sep):
            msg = "slug が MOUNT_BASE_DIR 外を参照しています"
            raise PathSecurityError(msg)
        return resolved


@dataclass
class MountConfig:
    """マウントポイント設定全体.

    - version: スキーマバージョン (1 or 2)
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

        v1 (path フィールド) と v2 (slug フィールド) の両方に対応。
        ファイルが存在しない場合は空の設定を返す。
        """
        if not self._config_path.exists():
            return MountConfig(version=2, mounts=[])

        try:
            raw = json.loads(self._config_path.read_text())
        except (json.JSONDecodeError, OSError) as exc:
            msg = f"設定ファイルの読み込みに失敗: {exc}"
            raise ValueError(msg) from exc

        mounts: list[MountPoint] = []
        for m in raw.get("mounts", []):
            slug = m.get("slug", "")
            if not slug and "path" in m:
                # v1 互換: path から slug を導出
                slug = self._derive_slug_from_path(m["path"])
            mounts.append(
                MountPoint(
                    mount_id=m["mount_id"],
                    name=m["name"],
                    slug=slug,
                    host_path=m.get("host_path", ""),
                )
            )
        return MountConfig(version=raw.get("version", 2), mounts=mounts)

    def save(self, config: MountConfig) -> None:
        """設定を v2 形式でファイルに書き込む."""
        self._config_path.parent.mkdir(parents=True, exist_ok=True)
        data = {
            "version": 2,
            "mounts": [
                {
                    "mount_id": m.mount_id,
                    "name": m.name,
                    "slug": m.slug,
                    "host_path": m.host_path,
                }
                for m in config.mounts
            ],
        }
        self._config_path.write_text(
            json.dumps(data, ensure_ascii=False, indent=2) + "\n"
        )

    def add_mount(self, name: str, slug: str, host_path: str = "") -> MountPoint:
        """マウントポイントを追加する.

        - slug の安全性を検証
        - 名前・slug の重複チェック
        - mounts.json に永続化
        """
        PathSecurity.validate_slug(slug)

        config = self.load()

        # 名前の重複チェック
        for m in config.mounts:
            if m.name == name:
                msg = f"同名のマウントポイントが既に存在します: {name}"
                raise ValueError(msg)

        # slug の重複チェック
        for m in config.mounts:
            if m.slug == slug:
                msg = f"同じ slug が既に登録されています: {slug}"
                raise ValueError(msg)

        mount = MountPoint(
            mount_id=uuid.uuid4().hex[:16],
            name=name,
            slug=slug,
            host_path=host_path,
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
        ROOT_DIR == MOUNT_BASE_DIR の場合は slug="." (ベースディレクトリ自体)。
        """
        resolved = root_dir.resolve()
        slug = self._derive_slug_from_path(str(resolved))
        mount = MountPoint(
            mount_id=uuid.uuid4().hex[:16],
            name=resolved.name,
            slug=slug,
        )
        config = MountConfig(version=2, mounts=[mount])
        self.save(config)
        return mount

    def _derive_slug_from_path(self, path_str: str) -> str:
        """v1 の path フィールドから slug を導出する.

        base_dir と一致する場合は "." を返す。
        base_dir 配下の場合は相対パスを返す。
        それ以外は basename を返す。
        """
        try:
            resolved = Path(path_str).resolve()
        except OSError, ValueError:
            return Path(path_str).name

        base_resolved = self._base_dir
        if resolved == base_resolved:
            return "."
        try:
            return str(resolved.relative_to(base_resolved))
        except ValueError:
            return resolved.name
