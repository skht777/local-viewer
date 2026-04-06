"""ArchiveEntryValidator のテスト."""

import pytest

from backend.services.archive_security import (
    ArchiveEntryValidator,
    ArchiveSecurityError,
)


@pytest.fixture
def validator(test_settings) -> ArchiveEntryValidator:
    return ArchiveEntryValidator(test_settings)


# --- エントリ名検証 ---


def test_正常なエントリ名を許可する(validator: ArchiveEntryValidator) -> None:
    # 例外なく通過すること
    validator.validate_entry_name("image01.jpg")
    validator.validate_entry_name("subdir/image02.png")
    validator.validate_entry_name("a/b/c/deep.webp")


def test_ドットドットを含むエントリ名を拒否する(
    validator: ArchiveEntryValidator,
) -> None:
    with pytest.raises(ArchiveSecurityError):
        validator.validate_entry_name("../etc/passwd")

    with pytest.raises(ArchiveSecurityError):
        validator.validate_entry_name("images/../../secret.txt")


def test_絶対パスを含むエントリ名を拒否する(validator: ArchiveEntryValidator) -> None:
    with pytest.raises(ArchiveSecurityError):
        validator.validate_entry_name("/etc/passwd")

    # Windows 形式の絶対パス(バックスラッシュ正規化後に C:/... = 相対パス扱い)
    # ただし先頭が / の場合は絶対パスとして拒否
    with pytest.raises(ArchiveSecurityError):
        validator.validate_entry_name("\\absolute\\path")


def test_NULバイトを含むエントリ名を拒否する(validator: ArchiveEntryValidator) -> None:
    with pytest.raises(ArchiveSecurityError):
        validator.validate_entry_name("image\x00.jpg")


def test_バックスラッシュを正規化してから検証する(
    validator: ArchiveEntryValidator,
) -> None:
    # バックスラッシュは / に正規化される(Windows 生成アーカイブ互換)
    # 正常なパスなら通過する
    validator.validate_entry_name("subdir\\image.jpg")

    # 正規化後に traversal になる場合は拒否
    with pytest.raises(ArchiveSecurityError):
        validator.validate_entry_name("..\\etc\\passwd")


# --- 拡張子ホワイトリスト ---


def test_許可拡張子のファイルを通す(validator: ArchiveEntryValidator) -> None:
    assert validator.is_allowed_extension("photo.jpg") is True
    assert validator.is_allowed_extension("photo.JPEG") is True
    assert validator.is_allowed_extension("image.png") is True
    assert validator.is_allowed_extension("image.gif") is True
    assert validator.is_allowed_extension("image.webp") is True
    assert validator.is_allowed_extension("image.bmp") is True
    assert validator.is_allowed_extension("image.avif") is True


def test_許可外拡張子のファイルを拒否する(validator: ArchiveEntryValidator) -> None:
    assert validator.is_allowed_extension("readme.txt") is False
    assert validator.is_allowed_extension("script.py") is False
    assert validator.is_allowed_extension("noextension") is False


def test_動画拡張子のファイルを許可する(validator: ArchiveEntryValidator) -> None:
    assert validator.is_allowed_extension("video.mp4") is True
    assert validator.is_allowed_extension("clip.webm") is True
    assert validator.is_allowed_extension("movie.MKV") is True
    assert validator.is_allowed_extension("file.avi") is True
    assert validator.is_allowed_extension("rec.mov") is True


def test_PDF拡張子のファイルを許可する(validator: ArchiveEntryValidator) -> None:
    assert validator.is_allowed_extension("document.pdf") is True
    assert validator.is_allowed_extension("report.PDF") is True


def test_PDFエントリのサイズ上限が動画と同じ(validator: ArchiveEntryValidator) -> None:
    """PDF は画像上限ではなく動画/PDF 用の大容量上限が適用される."""
    size = 100 * 1024 * 1024  # 100MB: 画像上限 (32MB) 超、動画上限 (500MB) 以内
    # PDF 名なら許可される
    validator.validate_entry_size(compressed=size, uncompressed=size, name="big.pdf")
    # 画像名なら拒否される (画像の上限を超えているため)
    with pytest.raises(ArchiveSecurityError):
        validator.validate_entry_size(
            compressed=size, uncompressed=size, name="big.jpg"
        )


# --- サイズ・圧縮率検証 ---


def test_圧縮率が上限を超えると拒否する(validator: ArchiveEntryValidator) -> None:
    # 圧縮率 = uncompressed / compressed = 1000 (> 100.0)
    with pytest.raises(ArchiveSecurityError, match="圧縮率"):
        validator.validate_entry_size(compressed=1, uncompressed=1_000)


def test_展開後合計サイズが上限を超えると拒否する(
    validator: ArchiveEntryValidator,
) -> None:
    # デフォルト上限は 1GB
    with pytest.raises(ArchiveSecurityError, match="合計サイズ"):
        validator.validate_total_size(2 * 1024 * 1024 * 1024)


def test_1エントリの展開後サイズが上限を超えると拒否する(
    validator: ArchiveEntryValidator,
) -> None:
    # デフォルト画像上限は 32MB
    large_size = 64 * 1024 * 1024
    with pytest.raises(ArchiveSecurityError, match="エントリサイズ"):
        validator.validate_entry_size(compressed=large_size, uncompressed=large_size)


def test_動画エントリのサイズ上限が画像より大きい(
    validator: ArchiveEntryValidator,
) -> None:
    # 100MB は画像上限 (32MB) を超えるが、動画上限 (500MB) 以内
    size = 100 * 1024 * 1024
    # 画像名なら拒否
    with pytest.raises(ArchiveSecurityError):
        validator.validate_entry_size(
            compressed=size, uncompressed=size, name="big.jpg"
        )
    # 動画名なら許可
    validator.validate_entry_size(compressed=size, uncompressed=size, name="big.mp4")


def test_画像エントリのサイズ上限は従来通り(
    validator: ArchiveEntryValidator,
) -> None:
    # 32MB ちょうどは OK
    ok_size = 32 * 1024 * 1024
    validator.validate_entry_size(
        compressed=ok_size, uncompressed=ok_size, name="ok.jpg"
    )
    # 32MB + 1 は NG
    over_size = 32 * 1024 * 1024 + 1
    with pytest.raises(ArchiveSecurityError):
        validator.validate_entry_size(
            compressed=over_size, uncompressed=over_size, name="over.png"
        )
