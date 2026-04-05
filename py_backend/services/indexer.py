"""SQLite FTS5 trigram によるファイルインデックス.

- ファイル名と相対パスの部分一致検索を提供
- trigram トークナイザで日本語・英語混在の部分一致に対応
- WAL モードで読み取り/書き込みの並行アクセスをサポート
- 操作ごとに新しい connection を作成 (スレッドセーフ)
"""

from __future__ import annotations

import bisect
import logging
import re
import sqlite3
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from py_backend.services.extensions import (
    ARCHIVE_EXTENSIONS,
    IMAGE_EXTENSIONS,
    PDF_EXTENSIONS,
    VIDEO_EXTENSIONS,
)
from py_backend.services.parallel_walk import parallel_walk

if TYPE_CHECKING:
    from py_backend.services.path_security import PathSecurity

logger = logging.getLogger(__name__)

# FTS5 の特殊文字パターン (エスケープ対象)
_FTS5_SPECIAL = re.compile(r'["\*]')

# バッチ INSERT のサイズ
_BATCH_SIZE = 1000

# FTS5 trigram の最小トークン長
_TRIGRAM_MIN_CHARS = 3

# インデックス対象の kind (検索可能なエントリ)
# image は個別ファイル数が膨大 (TB 規模で数百万件) のため除外
INDEXABLE_KINDS = frozenset({"directory", "video", "archive", "pdf"})

# スキーマバージョン (DB 永続化時のマイグレーション用)
# バージョン不一致なら全テーブル DROP → 再作成
_SCHEMA_VERSION = 2

_SCHEMA_SQL = """
PRAGMA journal_mode=WAL;
PRAGMA busy_timeout=5000;

CREATE TABLE IF NOT EXISTS schema_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    relative_path TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    size_bytes INTEGER,
    mtime_ns INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_entries_kind ON entries(kind);
CREATE INDEX IF NOT EXISTS idx_entries_relative_path ON entries(relative_path);

CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(
    name,
    relative_path,
    content=entries,
    content_rowid=id,
    tokenize='trigram'
);

CREATE TRIGGER IF NOT EXISTS entries_ai AFTER INSERT ON entries BEGIN
    INSERT INTO entries_fts(rowid, name, relative_path)
        VALUES (new.id, new.name, new.relative_path);
END;

CREATE TRIGGER IF NOT EXISTS entries_ad AFTER DELETE ON entries BEGIN
    INSERT INTO entries_fts(entries_fts, rowid, name, relative_path)
        VALUES('delete', old.id, old.name, old.relative_path);
END;

CREATE TRIGGER IF NOT EXISTS entries_au AFTER UPDATE ON entries BEGIN
    INSERT INTO entries_fts(entries_fts, rowid, name, relative_path)
        VALUES('delete', old.id, old.name, old.relative_path);
    INSERT INTO entries_fts(rowid, name, relative_path)
        VALUES (new.id, new.name, new.relative_path);
END;
"""


@dataclass
class IndexEntry:
    """インデックスの 1 エントリ."""

    relative_path: str
    name: str
    kind: str
    size_bytes: int | None
    mtime_ns: int


@dataclass
class SearchHit:
    """検索結果の 1 件."""

    relative_path: str
    name: str
    kind: str
    size_bytes: int | None


def _classify_by_extension(name: str) -> str:
    """拡張子から EntryKind 相当の文字列を返す."""
    dot_idx = name.rfind(".")
    if dot_idx <= 0:
        return "other"
    ext = name[dot_idx:].lower()
    if ext in IMAGE_EXTENSIONS:
        return "image"
    if ext in VIDEO_EXTENSIONS:
        return "video"
    if ext in PDF_EXTENSIONS:
        return "pdf"
    if ext in ARCHIVE_EXTENSIONS:
        return "archive"
    return "other"


def _mark_descendants_seen(
    sorted_keys: list[str], dir_rel: str, seen: set[str]
) -> None:
    """ソート済みキーからプレフィックス一致するエントリを seen に追加する.

    bisect で開始位置を特定し、プレフィックスが外れた時点で打ち切る。
    dir_rel="dir" の場合、"dir/" で始まるエントリのみ対象 ("dir2/" は除外)。
    """
    prefix = dir_rel + "/"
    start = bisect.bisect_left(sorted_keys, prefix)
    for i in range(start, len(sorted_keys)):
        if sorted_keys[i].startswith(prefix):
            seen.add(sorted_keys[i])
        else:
            break


class Indexer:
    """SQLite FTS5 trigram によるファイルインデックス.

    スレッドセーフティ:
    - 操作ごとに新しい sqlite3.Connection を作成
    - WAL モードで読み取りは並行、書き込みは SQLite 内部で直列化
    - busy_timeout=5000 で書き込み競合時にリトライ
    """

    def __init__(self, db_path: str) -> None:
        self._db_path = db_path
        self._is_ready = False
        self._is_stale = False
        self._is_rebuilding = False

    def _connect(self) -> sqlite3.Connection:
        """新しい接続を作成する (スレッドセーフ)."""
        conn = sqlite3.connect(self._db_path)
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("PRAGMA busy_timeout=5000")
        # インデックス DB はファイルシステムから復元可能なキャッシュのため、
        # fsync を緩和して書き込み性能を優先する
        conn.execute("PRAGMA synchronous=NORMAL")
        # FTS5 検索のページキャッシュを 8MB に拡大 (デフォルト 2MB)
        conn.execute("PRAGMA cache_size=-8192")
        # FTS5 の一時テーブルをメモリに配置して I/O を削減
        conn.execute("PRAGMA temp_store=MEMORY")
        return conn

    def init_db(self) -> None:
        """DB スキーマを作成する.

        永続化 DB のバージョンが不一致なら全テーブルを DROP して再作成する。
        インデックス DB はファイルシステムから復元可能なキャッシュなので安全。
        """
        conn = self._connect()
        try:
            # 既存バージョンを確認 (schema_meta が存在しない場合は None)
            existing_version = self._get_schema_version(conn)
            if existing_version is not None and existing_version != str(
                _SCHEMA_VERSION
            ):
                logger.info(
                    "スキーマバージョン不一致 (%s → %s): DB を再作成",
                    existing_version,
                    _SCHEMA_VERSION,
                )
                self._drop_all_tables(conn)

            conn.executescript(_SCHEMA_SQL)

            # バージョンを記録
            conn.execute(
                "INSERT OR REPLACE INTO schema_meta (key, value) VALUES (?, ?)",
                ("schema_version", str(_SCHEMA_VERSION)),
            )
            conn.commit()
        finally:
            conn.close()

    @staticmethod
    def _get_schema_version(conn: sqlite3.Connection) -> str | None:
        """現在のスキーマバージョンを返す (テーブル未存在なら None)."""
        try:
            row = conn.execute(
                "SELECT value FROM schema_meta WHERE key = 'schema_version'"
            ).fetchone()
            return row[0] if row else None
        except sqlite3.OperationalError:
            return None

    @staticmethod
    def _drop_all_tables(conn: sqlite3.Connection) -> None:
        """全テーブルを DROP する (スキーマ再作成用)."""
        conn.execute("DROP TABLE IF EXISTS entries_fts")
        conn.execute("DROP TABLE IF EXISTS entries")
        conn.execute("DROP TABLE IF EXISTS schema_meta")
        conn.commit()

    # --- 検索 ---

    def search(
        self,
        query: str,
        kind: str | None = None,
        limit: int = 50,
        offset: int = 0,
    ) -> tuple[list[SearchHit], bool]:
        """FTS5 trigram 検索を実行する.

        - 3 文字以上: FTS5 MATCH (trigram インデックス活用)
        - 2 文字: entries テーブルの LIKE フォールバック
        - 戻り値: (結果リスト, has_more)
        """
        query = query.strip()
        if not query:
            return [], False

        conn = self._connect()
        try:
            # limit+1 で取得し、超過分で has_more を判定
            fetch_limit = limit + 1

            tokens = query.split()
            min_len = min(len(t) for t in tokens) if tokens else 0

            if min_len >= _TRIGRAM_MIN_CHARS:
                rows = self._search_fts(conn, tokens, kind, fetch_limit, offset)
            else:
                rows = self._search_like(conn, query, kind, fetch_limit, offset)

            has_more = len(rows) > limit
            results = [
                SearchHit(
                    relative_path=r[0],
                    name=r[1],
                    kind=r[2],
                    size_bytes=r[3],
                )
                for r in rows[:limit]
            ]
            return results, has_more
        finally:
            conn.close()

    def _search_fts(
        self,
        conn: sqlite3.Connection,
        tokens: list[str],
        kind: str | None,
        limit: int,
        offset: int,
    ) -> list[tuple[str, str, str, int | None]]:
        """FTS5 trigram MATCH で検索する (3 文字以上)."""
        fts_query = self._build_fts_query(tokens)

        if kind:
            rows = conn.execute(
                """
                SELECT e.relative_path, e.name, e.kind, e.size_bytes
                FROM entries_fts f
                JOIN entries e ON f.rowid = e.id
                WHERE entries_fts MATCH ?
                AND e.kind = ?
                ORDER BY e.name
                LIMIT ? OFFSET ?
                """,
                (fts_query, kind, limit, offset),
            ).fetchall()
        else:
            rows = conn.execute(
                """
                SELECT e.relative_path, e.name, e.kind, e.size_bytes
                FROM entries_fts f
                JOIN entries e ON f.rowid = e.id
                WHERE entries_fts MATCH ?
                ORDER BY e.name
                LIMIT ? OFFSET ?
                """,
                (fts_query, limit, offset),
            ).fetchall()
        return rows

    def _search_like(
        self,
        conn: sqlite3.Connection,
        query: str,
        kind: str | None,
        limit: int,
        offset: int,
    ) -> list[tuple[str, str, str, int | None]]:
        """LIKE フォールバックで検索する (2 文字クエリ等)."""
        # LIKE 用パターン (%keyword%)
        like_pattern = f"%{query}%"

        if kind:
            rows = conn.execute(
                """
                SELECT relative_path, name, kind, size_bytes
                FROM entries
                WHERE (name LIKE ? OR relative_path LIKE ?)
                AND kind = ?
                ORDER BY name
                LIMIT ? OFFSET ?
                """,
                (like_pattern, like_pattern, kind, limit, offset),
            ).fetchall()
        else:
            rows = conn.execute(
                """
                SELECT relative_path, name, kind, size_bytes
                FROM entries
                WHERE name LIKE ? OR relative_path LIKE ?
                ORDER BY name
                LIMIT ? OFFSET ?
                """,
                (like_pattern, like_pattern, limit, offset),
            ).fetchall()
        return rows

    @staticmethod
    def _build_fts_query(tokens: list[str]) -> str:
        """検索トークンを FTS5 trigram 用クエリに変換する.

        各トークンをダブルクォートでエスケープし、暗黙 AND で結合。
        """
        escaped = []
        for token in tokens:
            # ダブルクォート内のダブルクォートは "" でエスケープ
            safe = token.replace('"', '""')
            escaped.append(f'"{safe}"')
        return " ".join(escaped)

    # --- CRUD ---

    def add_entry(self, entry: IndexEntry) -> None:
        """1 エントリを追加/更新する (UPSERT)."""
        conn = self._connect()
        try:
            conn.execute(
                """
                INSERT OR REPLACE INTO entries
                    (relative_path, name, kind, size_bytes, mtime_ns)
                VALUES (?, ?, ?, ?, ?)
                """,
                (
                    entry.relative_path,
                    entry.name,
                    entry.kind,
                    entry.size_bytes,
                    entry.mtime_ns,
                ),
            )
            conn.commit()
        finally:
            conn.close()

    def remove_entry(self, relative_path: str) -> None:
        """1 エントリを削除する."""
        conn = self._connect()
        try:
            conn.execute(
                "DELETE FROM entries WHERE relative_path = ?",
                (relative_path,),
            )
            conn.commit()
        finally:
            conn.close()

    # --- スキャン ---

    def scan_directory(
        self,
        root_dir: Path,
        path_security: PathSecurity,
        mount_id: str = "",
        workers: int = 8,
        on_walk_entry: Callable[..., None] | None = None,
    ) -> int:
        """ルートディレクトリ以下を再帰走査してインデックスに追加する.

        - parallel_walk で stat() を並列実行 (WSL2 drvfs 高速化)
        - PathSecurity チェックを通過したエントリのみ登録
        - 1000 エントリごとにバッチ INSERT
        - on_walk_entry: 各 WalkEntry を受け取るコールバック (DirIndex 連携用)
        - スキャン完了後 _is_ready = True
        - 戻り値: 登録エントリ数
        """
        conn = self._connect()
        count = 0
        batch: list[tuple[str, str, str, int | None, int]] = []
        prefix = f"{mount_id}/" if mount_id else ""

        # PathSecurity.validate() は例外を投げるため、bool ラッパーで包む
        def _safe_validate(p: Path) -> bool:
            try:
                path_security.validate(p)
                return True
            except Exception:
                return False

        try:
            for walk_entry in parallel_walk(
                root_dir,
                workers=workers,
                skip_hidden=True,
                path_validator=_safe_validate,
            ):
                dp = walk_entry.path

                # DirIndex コールバック: WalkEntry を丸ごと配る
                if on_walk_entry is not None:
                    on_walk_entry(
                        str(dp),
                        str(root_dir),
                        mount_id,
                        walk_entry.mtime_ns,
                        walk_entry.subdirs,
                        walk_entry.files,
                    )

                # ディレクトリ自体を登録 (ルートは除く)
                # mtime_ns は parallel_walk が取得済み
                if dp != root_dir:
                    rel = prefix + str(dp.relative_to(root_dir))
                    batch.append((rel, dp.name, "directory", None, walk_entry.mtime_ns))
                    count += 1

                # 拡張子チェックを先に実行
                # (drvfs 上では resolve() が 0.43ms/call のため、
                #  indexable でない 96% のファイルの validate() をスキップ)
                # parallel_walk が stat() を済ませているため個別 stat() 不要
                for fname, size_bytes, mtime_ns in walk_entry.files:
                    kind = _classify_by_extension(fname)
                    if kind not in INDEXABLE_KINDS:
                        continue
                    rel = prefix + str(dp.relative_to(root_dir) / fname)
                    batch.append((rel, fname, kind, size_bytes, mtime_ns))
                    count += 1

                    if len(batch) >= _BATCH_SIZE:
                        self._batch_insert(conn, batch)
                        batch.clear()

            if batch:
                self._batch_insert(conn, batch)

            self._is_stale = False
            self._is_ready = True
            return count
        finally:
            conn.close()

    def incremental_scan(
        self,
        root_dir: Path,
        path_security: PathSecurity,
        mount_id: str = "",
        workers: int = 8,
        on_walk_entry: Callable[..., None] | None = None,
    ) -> tuple[int, int, int]:
        """差分スキャン (追加, 更新, 削除) の件数を返す.

        - parallel_walk で stat() を並列実行 (WSL2 drvfs 高速化)
        - mtime_ns で変更を検出
        - dir_filter でディレクトリ mtime 未変更なら配下を枝刈り (mlocate 方式)
        - 既存パスが見つからなければ削除
        """
        conn = self._connect()
        added = 0
        updated = 0
        deleted = 0
        prefix = f"{mount_id}/" if mount_id else ""

        try:
            # 既存エントリのパスと mtime を取得 (マウント単位でフィルタ)
            if prefix:
                existing = dict(
                    conn.execute(
                        "SELECT relative_path, mtime_ns FROM entries"
                        " WHERE relative_path LIKE ?",
                        (prefix + "%",),
                    ).fetchall()
                )
            else:
                existing = dict(
                    conn.execute(
                        "SELECT relative_path, mtime_ns FROM entries"
                    ).fetchall()
                )
            seen: set[str] = set()

            # 枝刈り用: ソート済みキーで子孫エントリを高速に seen マーク
            sorted_existing_keys = sorted(existing.keys())

            # PathSecurity.validate() の bool ラッパー
            def _safe_validate(p: Path) -> bool:
                try:
                    path_security.validate(p)
                    return True
                except Exception:
                    return False

            # mtime 枝刈り: ディレクトリの mtime が未変更なら子孫を走査しない
            def _dir_mtime_filter(subdir_path: Path, mtime_ns: int) -> bool:
                rel = prefix + str(subdir_path.relative_to(root_dir))
                seen.add(rel)
                if rel in existing and existing[rel] == mtime_ns:
                    # mtime 未変更: 子孫を seen に追加して枝刈り
                    _mark_descendants_seen(sorted_existing_keys, rel, seen)
                    return False
                # 新規 or 変更あり → ディレクトリを登録して子孫を走査
                if rel not in existing:
                    self.add_entry(
                        IndexEntry(rel, subdir_path.name, "directory", None, mtime_ns)
                    )
                    return True
                # mtime 変更
                self.add_entry(
                    IndexEntry(rel, subdir_path.name, "directory", None, mtime_ns)
                )
                return True

            for walk_entry in parallel_walk(
                root_dir,
                workers=workers,
                skip_hidden=True,
                path_validator=_safe_validate,
                dir_filter=_dir_mtime_filter,
            ):
                dp = walk_entry.path

                # DirIndex コールバック
                if on_walk_entry is not None:
                    on_walk_entry(
                        str(dp),
                        str(root_dir),
                        mount_id,
                        walk_entry.mtime_ns,
                        walk_entry.subdirs,
                        walk_entry.files,
                    )

                # ルートは dir_filter を通らないためここで処理
                if dp != root_dir:
                    rel = prefix + str(dp.relative_to(root_dir))
                    # dir_filter で既に登録済みだが、seen には追加済み
                    # updated/added カウントは dir_filter 内で add_entry 済み
                    # → ここではカウントのみ
                    if rel not in existing:
                        added += 1
                    elif existing[rel] != walk_entry.mtime_ns:
                        updated += 1

                # ファイル処理 (parallel_walk が stat() 済み)
                for fname, size_bytes, mtime_ns in walk_entry.files:
                    kind = _classify_by_extension(fname)
                    if kind not in INDEXABLE_KINDS:
                        continue
                    rel = prefix + str(dp.relative_to(root_dir) / fname)
                    seen.add(rel)
                    if rel not in existing:
                        self.add_entry(
                            IndexEntry(rel, fname, kind, size_bytes, mtime_ns)
                        )
                        added += 1
                    elif existing[rel] != mtime_ns:
                        self.add_entry(
                            IndexEntry(rel, fname, kind, size_bytes, mtime_ns)
                        )
                        updated += 1

            # 削除検出
            for rel_path in existing:
                if rel_path not in seen:
                    self.remove_entry(rel_path)
                    deleted += 1

            self._is_stale = False
            self._is_ready = True
            return added, updated, deleted
        finally:
            conn.close()

    def rebuild(
        self,
        root_dir: Path,
        path_security: PathSecurity,
        mount_id: str = "",
    ) -> int:
        """全エントリを削除して再スキャンする."""
        self._is_rebuilding = True
        try:
            conn = self._connect()
            try:
                conn.execute("DELETE FROM entries")
                conn.commit()
            finally:
                conn.close()
            return self.scan_directory(root_dir, path_security, mount_id)
        finally:
            self._is_rebuilding = False

    def entry_count(self) -> int:
        """登録済みエントリ数を返す."""
        conn = self._connect()
        try:
            row = conn.execute("SELECT COUNT(*) FROM entries").fetchone()
            return row[0] if row else 0
        finally:
            conn.close()

    @property
    def is_ready(self) -> bool:
        """インデックスが利用可能か."""
        return self._is_ready

    @property
    def is_stale(self) -> bool:
        """インデックスが古い可能性があるか (バックグラウンドスキャン中)."""
        return self._is_stale

    @property
    def is_rebuilding(self) -> bool:
        """再構築中か."""
        return self._is_rebuilding

    def mark_warm_start(self) -> None:
        """既存 DB データで即座に検索を提供する (stale-while-revalidate)."""
        self._is_ready = True
        self._is_stale = True

    def check_mount_fingerprint(self, mount_ids: list[str]) -> bool:
        """現在のマウント構成が DB 保存時と一致するか確認する."""
        conn = self._connect()
        try:
            row = conn.execute(
                "SELECT value FROM schema_meta WHERE key = 'mount_fingerprint'"
            ).fetchone()
            if row is None:
                return False
            return bool(row[0] == ",".join(mount_ids))
        finally:
            conn.close()

    def save_mount_fingerprint(self, mount_ids: list[str]) -> None:
        """スキャン完了時にマウント構成を保存する."""
        conn = self._connect()
        try:
            conn.execute(
                "INSERT OR REPLACE INTO schema_meta (key, value) VALUES (?, ?)",
                ("mount_fingerprint", ",".join(mount_ids)),
            )
            conn.commit()
        finally:
            conn.close()

    @staticmethod
    def _batch_insert(
        conn: sqlite3.Connection,
        batch: list[tuple[str, str, str, int | None, int]],
    ) -> None:
        """バッチ INSERT (UPSERT)."""
        conn.executemany(
            """
            INSERT OR REPLACE INTO entries
                (relative_path, name, kind, size_bytes, mtime_ns)
            VALUES (?, ?, ?, ?, ?)
            """,
            batch,
        )
        conn.commit()
