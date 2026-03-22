"""パスセキュリティのテスト."""

import os
from collections.abc import Generator
from pathlib import Path

import pytest

from backend.config import Settings
from backend.errors import PathSecurityError
from backend.services.path_security import PathSecurity


@pytest.fixture
def root_dir(tmp_path: Path) -> Path:
    """テスト用ルートディレクトリ."""
    (tmp_path / "file.txt").write_text("hello")
    (tmp_path / "subdir").mkdir()
    (tmp_path / "subdir" / "nested.txt").write_text("nested")
    return tmp_path


@pytest.fixture
def settings(root_dir: Path) -> Generator[Settings]:
    """テスト用 Settings."""
    os.environ["ROOT_DIR"] = str(root_dir)
    os.environ.pop("ALLOW_SYMLINKS", None)
    s = Settings()
    yield s
    os.environ.pop("ROOT_DIR", None)


@pytest.fixture
def security(settings: Settings) -> PathSecurity:
    """テスト用 PathSecurity."""
    return PathSecurity(settings)


def test_ROOT_DIR直下のファイルを許可する(
    security: PathSecurity, root_dir: Path
) -> None:
    result = security.validate(root_dir / "file.txt")
    assert result == (root_dir / "file.txt").resolve()


def test_ROOT_DIR直下のサブディレクトリを許可する(
    security: PathSecurity, root_dir: Path
) -> None:
    result = security.validate(root_dir / "subdir" / "nested.txt")
    assert result == (root_dir / "subdir" / "nested.txt").resolve()


def test_ROOT_DIR自体を許可する(security: PathSecurity, root_dir: Path) -> None:
    result = security.validate(root_dir)
    assert result == root_dir.resolve()


def test_ドットドットによるtraversalを拒否する(
    security: PathSecurity, root_dir: Path
) -> None:
    with pytest.raises(PathSecurityError):
        security.validate(root_dir / ".." / ".." / "etc" / "passwd")


def test_resolve後にROOT_DIR外になるパスを拒否する(
    security: PathSecurity, root_dir: Path
) -> None:
    # subdir/../../ で root_dir の親に脱出
    with pytest.raises(PathSecurityError):
        security.validate(root_dir / "subdir" / ".." / ".." / "etc")


def test_絶対パスのsafe_joinを拒否する(security: PathSecurity) -> None:
    with pytest.raises(PathSecurityError):
        security.safe_join("/etc/passwd")


def test_NULバイトを含むパスを拒否する(security: PathSecurity) -> None:
    with pytest.raises(PathSecurityError):
        security.safe_join("file\x00.txt")


def test_symlinkがデフォルトで拒否される(
    security: PathSecurity, root_dir: Path
) -> None:
    # symlink 作成: root_dir/link -> root_dir/subdir
    link = root_dir / "link"
    link.symlink_to(root_dir / "subdir")
    with pytest.raises(PathSecurityError):
        security.validate(link / "nested.txt")


def test_ALLOW_SYMLINKS有効時にsymlinkを許可する(
    root_dir: Path,
) -> None:
    os.environ["ROOT_DIR"] = str(root_dir)
    os.environ["ALLOW_SYMLINKS"] = "true"
    try:
        s = Settings()
        sec = PathSecurity(s)
        link = root_dir / "link"
        if not link.exists():
            link.symlink_to(root_dir / "subdir")
        result = sec.validate(link / "nested.txt")
        assert result == (root_dir / "subdir" / "nested.txt").resolve()
    finally:
        os.environ.pop("ROOT_DIR", None)
        os.environ.pop("ALLOW_SYMLINKS", None)


def test_validate_existingで存在しないパスがFileNotFoundError(
    security: PathSecurity, root_dir: Path
) -> None:
    with pytest.raises(FileNotFoundError):
        security.validate_existing(root_dir / "nonexistent.txt")


def test_safe_joinで正常なパスを結合する(
    security: PathSecurity, root_dir: Path
) -> None:
    result = security.safe_join("subdir", "nested.txt")
    assert result == (root_dir / "subdir" / "nested.txt").resolve()
