"""マウントポイント管理 TUI.

プロジェクトルートから実行:
  source backend/.venv/bin/activate && python manage_mounts.py

Docker コンテナ内:
  docker compose exec viewer python manage_mounts.py
"""

# ruff: noqa: T201 — TUI スクリプトのため print は正当

from __future__ import annotations

import os
import sys
from pathlib import Path

from backend.services.mount_config import MountConfigService


def _get_service() -> MountConfigService:
    """環境変数から MountConfigService を構築する."""
    config_path = os.environ.get("MOUNT_CONFIG_PATH", "config/mounts.json")
    base_dir_str = os.environ.get("MOUNT_BASE_DIR", "")
    if not base_dir_str:
        print("エラー: MOUNT_BASE_DIR を設定してください")
        sys.exit(1)

    base_dir = Path(base_dir_str).resolve()
    if not base_dir.is_dir():
        print(f"エラー: MOUNT_BASE_DIR が存在しません: {base_dir}")
        sys.exit(1)

    return MountConfigService(Path(config_path), base_dir)


def _show_mounts(service: MountConfigService) -> None:
    """現在のマウントポイントを表示する."""
    config = service.load()
    base_dir = os.environ.get("MOUNT_BASE_DIR", "")
    print(f"\nMOUNT_BASE_DIR: {base_dir}")
    print()

    if not config.mounts:
        print("  マウントポイントが登録されていません")
        return

    for i, m in enumerate(config.mounts, 1):
        print(f"  {i}. [{m.mount_id}] {m.name}")
        print(f"     → {m.slug}")


def _add_mount(service: MountConfigService) -> None:
    """マウントポイントを追加する."""
    print()
    path = input("パス (コンテナ内の絶対パス): ").strip()
    if not path:
        print("キャンセルしました")
        return

    # パスから slug を導出 (ディレクトリ名)
    slug = Path(path).name
    default_name = slug
    name = input(f"表示名 [{default_name}]: ").strip() or default_name

    try:
        mount = service.add_mount(name, slug)
        print(f"\n追加しました: [{mount.mount_id}] {mount.name}")
        print("サーバー再起動が必要です")
    except Exception as exc:
        print(f"\nエラー: {exc}")


def _edit_mount(service: MountConfigService) -> None:
    """マウントポイント名を編集する."""
    config = service.load()
    if not config.mounts:
        print("マウントポイントが登録されていません")
        return

    print()
    idx_str = input("編集する番号: ").strip()
    try:
        idx = int(idx_str) - 1
        mount = config.mounts[idx]
    except ValueError, IndexError:
        print("無効な番号です")
        return

    new_name = input(f"新しい表示名 [{mount.name}]: ").strip()
    if not new_name:
        print("キャンセルしました")
        return

    try:
        updated = service.update_mount(mount.mount_id, name=new_name)
        print(f"\n更新しました: {updated.name}")
        print("サーバー再起動が必要です")
    except Exception as exc:
        print(f"\nエラー: {exc}")


def _delete_mount(service: MountConfigService) -> None:
    """マウントポイントを削除する."""
    config = service.load()
    if not config.mounts:
        print("マウントポイントが登録されていません")
        return

    print()
    idx_str = input("削除する番号: ").strip()
    try:
        idx = int(idx_str) - 1
        mount = config.mounts[idx]
    except ValueError, IndexError:
        print("無効な番号です")
        return

    confirm = input(f"'{mount.name}' を削除しますか? [y/N]: ").strip().lower()
    if confirm != "y":
        print("キャンセルしました")
        return

    try:
        service.remove_mount(mount.mount_id)
        print(f"\n削除しました: {mount.name}")
        print("サーバー再起動が必要です")
    except Exception as exc:
        print(f"\nエラー: {exc}")


def main() -> None:
    """TUI メインループ."""
    print("Local Content Viewer - マウントポイント管理")

    service = _get_service()

    while True:
        _show_mounts(service)
        print()
        print("操作を選択: [a] 追加  [e] 編集  [d] 削除  [q] 終了")
        choice = input("> ").strip().lower()

        if choice == "a":
            _add_mount(service)
        elif choice == "e":
            _edit_mount(service)
        elif choice == "d":
            _delete_mount(service)
        elif choice == "q":
            break
        else:
            print("無効な選択です")


if __name__ == "__main__":
    # PathSecurity の import で backend パッケージが解決可能であることを保証
    main()
