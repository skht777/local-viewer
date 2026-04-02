"""BFS レベル単位の並列ディレクトリ走査.

ThreadPoolExecutor で各ディレクトリの os.scandir + DirEntry.stat() を並列化し、
WSL2 drvfs 等の高レイテンシ FS でのスキャンを高速化する。
"""

from __future__ import annotations

import logging
import os
from collections.abc import Callable, Iterator
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass, field
from pathlib import Path

logger = logging.getLogger(__name__)


@dataclass(frozen=True)
class WalkEntry:
    """1 ディレクトリの走査結果."""

    path: Path
    mtime_ns: int = 0  # ディレクトリ自体の mtime (ナノ秒)
    subdirs: list[tuple[str, int]] = field(default_factory=list)  # (name, mtime_ns)
    files: list[tuple[str, int, int]] = field(
        default_factory=list
    )  # (name, size_bytes, mtime_ns)


def _scan_one(
    dir_path: Path,
    *,
    skip_hidden: bool,
    path_validator: Callable[[Path], bool] | None,
) -> WalkEntry:
    """1 ディレクトリを os.scandir で走査し、stat 付きの結果を返す.

    - path_validator が指定されている場合、stat() 前にパスを検証する
    - 隠しファイル/ディレクトリ (先頭 '.') はスキップ
    """
    subdirs: list[tuple[str, int]] = []
    files: list[tuple[str, int, int]] = []

    try:
        with os.scandir(dir_path) as entries:
            for entry in entries:
                if skip_hidden and entry.name.startswith("."):
                    continue

                child_path = dir_path / entry.name

                # セキュリティ検証 (stat 前)
                if path_validator and not path_validator(child_path):
                    continue

                try:
                    st = entry.stat(follow_symlinks=False)
                except OSError:
                    continue

                if entry.is_dir(follow_symlinks=False):
                    subdirs.append((entry.name, st.st_mtime_ns))
                elif entry.is_file(follow_symlinks=False):
                    files.append((entry.name, st.st_size, st.st_mtime_ns))
    except PermissionError, OSError:
        logger.debug("走査スキップ: %s", dir_path)

    # ディレクトリ自体の mtime を取得
    try:
        dir_mtime_ns = dir_path.stat().st_mtime_ns
    except OSError:
        dir_mtime_ns = 0

    return WalkEntry(path=dir_path, mtime_ns=dir_mtime_ns, subdirs=subdirs, files=files)


def parallel_walk(
    root: Path,
    *,
    workers: int = 8,
    skip_hidden: bool = True,
    path_validator: Callable[[Path], bool] | None = None,
) -> Iterator[WalkEntry]:
    """BFS レベル単位でディレクトリを並列走査する.

    - 各レベルのディレクトリを ThreadPoolExecutor で並列に os.scandir
    - stat() はワーカースレッド内で実行 (I/O レイテンシを並列化)
    - path_validator: stat() 前にパスを検証するコールバック (PathSecurity 連携用)
    """
    current_level = [root]

    with ThreadPoolExecutor(max_workers=workers) as pool:
        while current_level:
            # 現在のレベルのディレクトリを並列スキャン
            futures = {
                pool.submit(
                    _scan_one,
                    d,
                    skip_hidden=skip_hidden,
                    path_validator=path_validator,
                ): d
                for d in current_level
            }

            next_level: list[Path] = []

            for future in as_completed(futures):
                entry = future.result()
                yield entry

                # サブディレクトリを次のレベルに追加
                for name, _mtime_ns in entry.subdirs:
                    next_level.append(entry.path / name)

            current_level = next_level
