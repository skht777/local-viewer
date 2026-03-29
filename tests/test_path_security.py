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


# --- 複数ルート対応テスト ---


@pytest.fixture
def multi_roots(tmp_path: Path) -> tuple[Path, Path]:
    """複数ルート用テストディレクトリ."""
    root_a = tmp_path / "root_a"
    root_b = tmp_path / "root_b"
    root_a.mkdir()
    root_b.mkdir()
    (root_a / "file_a.txt").write_text("a")
    (root_b / "file_b.txt").write_text("b")
    (root_a / "shared_name").mkdir()
    (root_b / "shared_name").mkdir()
    return root_a, root_b


@pytest.fixture
def multi_security(multi_roots: tuple[Path, Path]) -> PathSecurity:
    """複数ルートの PathSecurity."""
    return PathSecurity(list(multi_roots))


class TestRootDirsプロパティ:
    def test_単一ルートでroot_dirsが1要素のリストを返す(
        self, security: PathSecurity, root_dir: Path
    ) -> None:
        assert security.root_dirs == [root_dir.resolve()]

    def test_複数ルートでroot_dirsが全ルートを返す(
        self, multi_security: PathSecurity, multi_roots: tuple[Path, Path]
    ) -> None:
        root_a, root_b = multi_roots
        assert multi_security.root_dirs == [root_a.resolve(), root_b.resolve()]

    def test_単一ルートでroot_dirs先頭が取得できる(
        self, security: PathSecurity, root_dir: Path
    ) -> None:
        assert security.root_dirs[0] == root_dir.resolve()


class TestMultiRootValidate:
    def test_root_A配下のファイルを許可する(
        self, multi_security: PathSecurity, multi_roots: tuple[Path, Path]
    ) -> None:
        root_a, _ = multi_roots
        result = multi_security.validate(root_a / "file_a.txt")
        assert result == (root_a / "file_a.txt").resolve()

    def test_root_B配下のファイルを許可する(
        self, multi_security: PathSecurity, multi_roots: tuple[Path, Path]
    ) -> None:
        _, root_b = multi_roots
        result = multi_security.validate(root_b / "file_b.txt")
        assert result == (root_b / "file_b.txt").resolve()

    def test_どのルートにも属さないパスを拒否する(
        self, multi_security: PathSecurity, tmp_path: Path
    ) -> None:
        with pytest.raises(PathSecurityError):
            multi_security.validate(tmp_path / "outside.txt")

    def test_root_A配下からroot_Bへのトラバーサルを拒否する(
        self, multi_security: PathSecurity, multi_roots: tuple[Path, Path]
    ) -> None:
        root_a, _ = multi_roots
        with pytest.raises(PathSecurityError):
            multi_security.validate(root_a / ".." / ".." / "etc" / "passwd")


class TestFindRootFor:
    def test_root_A配下のパスに対してroot_Aを返す(
        self, multi_security: PathSecurity, multi_roots: tuple[Path, Path]
    ) -> None:
        root_a, _ = multi_roots
        result = multi_security.find_root_for((root_a / "file_a.txt").resolve())
        assert result == root_a.resolve()

    def test_root_B配下のパスに対してroot_Bを返す(
        self, multi_security: PathSecurity, multi_roots: tuple[Path, Path]
    ) -> None:
        _, root_b = multi_roots
        result = multi_security.find_root_for((root_b / "file_b.txt").resolve())
        assert result == root_b.resolve()

    def test_どのルートにも属さないパスにNoneを返す(
        self, multi_security: PathSecurity, tmp_path: Path
    ) -> None:
        result = multi_security.find_root_for(tmp_path / "outside.txt")
        assert result is None

    def test_ルート自体に対してルートを返す(
        self, multi_security: PathSecurity, multi_roots: tuple[Path, Path]
    ) -> None:
        root_a, _ = multi_roots
        result = multi_security.find_root_for(root_a.resolve())
        assert result == root_a.resolve()


class TestValidateSlug:
    def test_正常なスラッグを許可する(self) -> None:
        PathSecurity.validate_slug("photos")  # 例外なし

    def test_ハイフン付きスラッグを許可する(self) -> None:
        PathSecurity.validate_slug("my-photos")  # 例外なし

    def test_空のスラッグを拒否する(self) -> None:
        with pytest.raises(PathSecurityError):
            PathSecurity.validate_slug("")

    def test_ドットのみのスラッグを拒否する(self) -> None:
        with pytest.raises(PathSecurityError):
            PathSecurity.validate_slug(".")

    def test_ドットドットを拒否する(self) -> None:
        with pytest.raises(PathSecurityError):
            PathSecurity.validate_slug("..")

    def test_NULバイトを含むスラッグを拒否する(self) -> None:
        with pytest.raises(PathSecurityError):
            PathSecurity.validate_slug("test\x00slug")

    def test_スラッシュを含むスラッグを拒否する(self) -> None:
        with pytest.raises(PathSecurityError):
            PathSecurity.validate_slug("path/traversal")

    def test_バックスラッシュを含むスラッグを拒否する(self) -> None:
        with pytest.raises(PathSecurityError):
            PathSecurity.validate_slug("path\\traversal")
