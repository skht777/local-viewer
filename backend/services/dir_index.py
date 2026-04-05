"""ディレクトリリスティング専用 SQLite インデックス (DirIndex).

全エントリ (画像含む) を parent_path ベースで格納し、
date/name ソート + ページネーション + child_count/preview を高速化する。
既存 Indexer (FTS5 検索) とは独立した DB。

- sort_key: natural_sort_key の事前計算結果 (数値 10 桁ゼロ埋め)
- parent_path インデックスで O(log n) のディレクトリ内クエリ
- Warm Start パターン対応 (is_ready / is_stale)
"""

from __future__ import annotations

import re
import sqlite3
import threading

_BATCH_SIZE = 1000
_SPLIT_RE = re.compile(r"(\d+)")


def encode_sort_key(name: str) -> str:
    """ファイル名を SQLite TEXT 比較で自然順になるソートキーに変換する.

    数値部分を 10 桁ゼロ埋め、テキスト部分は小文字化、
    要素間を NUL 文字で区切る。
    例: "file2.jpg" → "file\\x000000000002\\x00.jpg"
    """
    parts = _SPLIT_RE.split(name.lower())
    encoded: list[str] = []
    for part in parts:
        if part.isdigit():
            encoded.append(part.zfill(10))
        else:
            encoded.append(part)
    return "\x00".join(encoded)


class DirIndex:
    """ディレクトリリスティング専用インデックス."""

    _SCHEMA_VERSION = "1"

    _BATCH_SIZE = 1000

    def __init__(self, db_path: str) -> None:
        self._db_path = db_path
        self._is_ready = False
        self._is_stale = True
        self._lock = threading.Lock()
        # バルクモード用 (begin_bulk/end_bulk で管理)
        self._bulk_conn: sqlite3.Connection | None = None
        self._bulk_entries: list[tuple[str, str, str, str, int | None, int]] = []
        self._bulk_meta: list[tuple[str, int]] = []

    @property
    def is_ready(self) -> bool:
        return self._is_ready

    @property
    def is_stale(self) -> bool:
        return self._is_stale

    def _connect(self) -> sqlite3.Connection:
        """新しい接続を作成する (スレッドセーフ)."""
        conn = sqlite3.connect(self._db_path)
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("PRAGMA busy_timeout=5000")
        conn.execute("PRAGMA synchronous=NORMAL")
        conn.execute("PRAGMA cache_size=-8192")
        conn.execute("PRAGMA temp_store=MEMORY")
        conn.row_factory = sqlite3.Row
        return conn

    def _connect_for_bulk(self) -> sqlite3.Connection:
        """バルク挿入用の接続 (synchronous=OFF で fsync を無効化)."""
        conn = sqlite3.connect(self._db_path)
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("PRAGMA synchronous=OFF")
        conn.execute("PRAGMA cache_size=-16384")
        conn.execute("PRAGMA temp_store=MEMORY")
        return conn

    def begin_bulk(self) -> None:
        """バルク挿入モードを開始する.

        単一接続 + synchronous=OFF + 1000 件バッチで INSERT。
        DirIndex はキャッシュ DB のため、中断時のデータ損失は許容。
        """
        self._bulk_conn = self._connect_for_bulk()
        self._bulk_entries = []
        self._bulk_meta = []

    def end_bulk(self) -> None:
        """バルク挿入モードを終了する (残りフラッシュ + 接続クローズ)."""
        if self._bulk_conn is None:
            return
        if self._bulk_entries or self._bulk_meta:
            self._flush_bulk()
        self._bulk_conn.close()
        self._bulk_conn = None

    def _flush_bulk(self) -> None:
        """バルクバッチを SQLite にフラッシュする."""
        conn = self._bulk_conn
        if conn is None:
            return
        if self._bulk_entries:
            conn.executemany(
                """
                INSERT OR REPLACE INTO dir_entries
                    (parent_path, name, kind, sort_key, size_bytes, mtime_ns)
                VALUES (?, ?, ?, ?, ?, ?)
                """,
                self._bulk_entries,
            )
        if self._bulk_meta:
            conn.executemany(
                "INSERT OR REPLACE INTO dir_meta (path, mtime_ns) VALUES (?, ?)",
                self._bulk_meta,
            )
        conn.commit()
        self._bulk_entries.clear()
        self._bulk_meta.clear()

    def init_db(self) -> None:
        """テーブルとインデックスを作成する."""
        conn = self._connect()
        try:
            conn.executescript("""
                CREATE TABLE IF NOT EXISTS schema_meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS dir_entries (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    parent_path TEXT NOT NULL,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    sort_key TEXT NOT NULL,
                    size_bytes INTEGER,
                    mtime_ns INTEGER NOT NULL,
                    UNIQUE(parent_path, name)
                );

                CREATE INDEX IF NOT EXISTS idx_dir_parent
                    ON dir_entries(parent_path);
                CREATE INDEX IF NOT EXISTS idx_dir_parent_sort
                    ON dir_entries(parent_path, sort_key);
                CREATE INDEX IF NOT EXISTS idx_dir_parent_mtime
                    ON dir_entries(parent_path, mtime_ns);

                CREATE TABLE IF NOT EXISTS dir_meta (
                    path TEXT PRIMARY KEY,
                    mtime_ns INTEGER NOT NULL
                );
            """)
            # スキーマバージョン保存
            conn.execute(
                "INSERT OR REPLACE INTO schema_meta (key, value) VALUES (?, ?)",
                ("schema_version", self._SCHEMA_VERSION),
            )
            conn.commit()
        finally:
            conn.close()

    def add_entries(
        self,
        parent_path: str,
        entries: list[tuple[str, str, int | None, int]],
    ) -> None:
        """ディレクトリのエントリを一括追加する.

        entries: [(name, kind, size_bytes, mtime_ns), ...]
        """
        conn = self._connect()
        try:
            rows = [
                (parent_path, name, kind, encode_sort_key(name), size_bytes, mtime_ns)
                for name, kind, size_bytes, mtime_ns in entries
            ]
            conn.executemany(
                """
                INSERT OR REPLACE INTO dir_entries
                    (parent_path, name, kind, sort_key, size_bytes, mtime_ns)
                VALUES (?, ?, ?, ?, ?, ?)
                """,
                rows,
            )
            conn.commit()
        finally:
            conn.close()

    def query_all(self, parent_path: str) -> list[dict[str, object]]:
        """指定ディレクトリの全エントリを返す."""
        conn = self._connect()
        try:
            rows = conn.execute(
                "SELECT * FROM dir_entries WHERE parent_path = ? ORDER BY sort_key",
                (parent_path,),
            ).fetchall()
            return [dict(r) for r in rows]
        finally:
            conn.close()

    def query_page(
        self,
        parent_path: str,
        sort: str = "name-asc",
        limit: int = 100,
        cursor_sort_key: str | None = None,
    ) -> list[dict[str, object]]:
        """ソート + ページネーション付きでエントリを返す.

        sort: name-asc, name-desc, date-asc, date-desc
        cursor_sort_key: 前ページ末尾のソートキー (seek cursor)
        """
        conn = self._connect()
        try:
            if sort == "name-asc":
                return self._query_name_asc(conn, parent_path, limit, cursor_sort_key)
            if sort == "name-desc":
                return self._query_name_desc(conn, parent_path, limit, cursor_sort_key)
            if sort == "date-desc":
                return self._query_date_desc(conn, parent_path, limit, cursor_sort_key)
            # date-asc
            return self._query_date_asc(conn, parent_path, limit, cursor_sort_key)
        finally:
            conn.close()

    def _query_name_asc(
        self,
        conn: sqlite3.Connection,
        parent_path: str,
        limit: int,
        cursor: str | None,
    ) -> list[dict[str, object]]:
        """ディレクトリ優先 + 名前昇順."""
        if cursor:
            rows = conn.execute(
                """
                SELECT * FROM dir_entries
                WHERE parent_path = ?
                  AND ((kind != 'directory'), sort_key) > (
                    (SELECT (kind != 'directory') FROM dir_entries
                     WHERE parent_path = ? AND sort_key = ?),
                    ?
                  )
                ORDER BY (kind != 'directory'), sort_key
                LIMIT ?
                """,
                (parent_path, parent_path, cursor, cursor, limit),
            ).fetchall()
        else:
            rows = conn.execute(
                """
                SELECT * FROM dir_entries
                WHERE parent_path = ?
                ORDER BY (kind != 'directory'), sort_key
                LIMIT ?
                """,
                (parent_path, limit),
            ).fetchall()
        return [dict(r) for r in rows]

    def _query_name_desc(
        self,
        conn: sqlite3.Connection,
        parent_path: str,
        limit: int,
        cursor: str | None,
    ) -> list[dict[str, object]]:
        """ディレクトリ優先 + 名前降順."""
        if cursor:
            rows = conn.execute(
                """
                SELECT * FROM dir_entries
                WHERE parent_path = ?
                  AND ((kind != 'directory'), sort_key) < (
                    (SELECT (kind != 'directory') FROM dir_entries
                     WHERE parent_path = ? AND sort_key = ?),
                    ?
                  )
                ORDER BY (kind != 'directory') ASC, sort_key DESC
                LIMIT ?
                """,
                (parent_path, parent_path, cursor, cursor, limit),
            ).fetchall()
        else:
            rows = conn.execute(
                """
                SELECT * FROM dir_entries
                WHERE parent_path = ?
                ORDER BY (kind != 'directory') ASC, sort_key DESC
                LIMIT ?
                """,
                (parent_path, limit),
            ).fetchall()
        return [dict(r) for r in rows]

    def _query_date_desc(
        self,
        conn: sqlite3.Connection,
        parent_path: str,
        limit: int,
        cursor: str | None,
    ) -> list[dict[str, object]]:
        """日付降順 (新しい順)."""
        if cursor:
            # cursor は mtime_ns の文字列表現
            rows = conn.execute(
                """
                SELECT * FROM dir_entries
                WHERE parent_path = ? AND mtime_ns < ?
                ORDER BY mtime_ns DESC
                LIMIT ?
                """,
                (parent_path, int(cursor), limit),
            ).fetchall()
        else:
            rows = conn.execute(
                """
                SELECT * FROM dir_entries
                WHERE parent_path = ?
                ORDER BY mtime_ns DESC
                LIMIT ?
                """,
                (parent_path, limit),
            ).fetchall()
        return [dict(r) for r in rows]

    def _query_date_asc(
        self,
        conn: sqlite3.Connection,
        parent_path: str,
        limit: int,
        cursor: str | None,
    ) -> list[dict[str, object]]:
        """日付昇順 (古い順)."""
        if cursor:
            rows = conn.execute(
                """
                SELECT * FROM dir_entries
                WHERE parent_path = ? AND mtime_ns > ?
                ORDER BY mtime_ns ASC
                LIMIT ?
                """,
                (parent_path, int(cursor), limit),
            ).fetchall()
        else:
            rows = conn.execute(
                """
                SELECT * FROM dir_entries
                WHERE parent_path = ?
                ORDER BY mtime_ns ASC
                LIMIT ?
                """,
                (parent_path, limit),
            ).fetchall()
        return [dict(r) for r in rows]

    def child_count(self, parent_path: str) -> int:
        """ディレクトリの子エントリ数を返す."""
        conn = self._connect()
        try:
            row = conn.execute(
                "SELECT COUNT(*) FROM dir_entries WHERE parent_path = ?",
                (parent_path,),
            ).fetchone()
            return row[0] if row else 0
        finally:
            conn.close()

    def preview_entries(
        self, parent_path: str, limit: int = 3
    ) -> list[dict[str, object]]:
        """ディレクトリ内のサムネイル対象エントリを返す."""
        conn = self._connect()
        try:
            rows = conn.execute(
                """
                SELECT * FROM dir_entries
                WHERE parent_path = ? AND kind IN ('image', 'archive', 'pdf')
                ORDER BY sort_key ASC
                LIMIT ?
                """,
                (parent_path, limit),
            ).fetchall()
            return [dict(r) for r in rows]
        finally:
            conn.close()

    def total_count(self, parent_path: str) -> int:
        """ディレクトリの全エントリ数を返す (child_count と同義)."""
        return self.child_count(parent_path)

    def set_dir_mtime(self, path: str, mtime_ns: int) -> None:
        """ディレクトリの mtime を記録する."""
        conn = self._connect()
        try:
            conn.execute(
                "INSERT OR REPLACE INTO dir_meta (path, mtime_ns) VALUES (?, ?)",
                (path, mtime_ns),
            )
            conn.commit()
        finally:
            conn.close()

    def get_dir_mtime(self, path: str) -> int | None:
        """ディレクトリの記録済み mtime を返す."""
        conn = self._connect()
        try:
            row = conn.execute(
                "SELECT mtime_ns FROM dir_meta WHERE path = ?",
                (path,),
            ).fetchone()
            return row[0] if row else None
        finally:
            conn.close()

    def entry_count(self) -> int:
        """DB 内の全エントリ数を返す."""
        conn = self._connect()
        try:
            row = conn.execute("SELECT COUNT(*) FROM dir_entries").fetchone()
            return row[0] if row else 0
        finally:
            conn.close()

    def ingest_walk_entry(
        self,
        walk_entry_path: str,
        root_dir: str,
        mount_id: str,
        dir_mtime_ns: int,
        subdirs: list[tuple[str, int]],
        files: list[tuple[str, int, int]],
    ) -> None:
        """parallel_walk の WalkEntry を受け取り DirIndex に格納する.

        - walk_entry_path: ディレクトリの絶対パス文字列
        - root_dir: マウントルートの絶対パス文字列
        - mount_id: マウント ID
        - dir_mtime_ns: ディレクトリ自体の mtime
        - subdirs: [(name, mtime_ns), ...]
        - files: [(name, size_bytes, mtime_ns), ...]
        """
        # parent_path: "{mount_id}/relative/path" (ルート自体は mount_id)
        from pathlib import Path

        from backend.services.indexer import _classify_by_extension

        rel = str(Path(walk_entry_path).relative_to(root_dir))
        parent_path = f"{mount_id}/{rel}" if rel != "." else mount_id

        # サブディレクトリをエントリとして追加
        entries: list[tuple[str, str, int | None, int]] = []
        for name, mtime_ns in subdirs:
            entries.append((name, "directory", None, mtime_ns))
        # ファイルをエントリとして追加
        for name, size_bytes, mtime_ns in files:
            kind = _classify_by_extension(name)
            entries.append((name, kind, size_bytes, mtime_ns))

        # バルクモード: バッチに蓄積して 1000 件ごとにフラッシュ
        # 通常モード: per-directory コミット (FileWatcher 用)
        if self._bulk_conn is not None:
            for e_name, e_kind, e_size, e_mtime in entries:
                sk = encode_sort_key(e_name)
                self._bulk_entries.append(
                    (parent_path, e_name, e_kind, sk, e_size, e_mtime)
                )
            self._bulk_meta.append((parent_path, dir_mtime_ns))
            if len(self._bulk_entries) >= self._BATCH_SIZE:
                self._flush_bulk()
        else:
            if entries:
                self.add_entries(parent_path, entries)
            self.set_dir_mtime(parent_path, dir_mtime_ns)

    def is_full_scan_done(self) -> bool:
        """フルスキャンが完了しているかを返す."""
        conn = self._connect()
        try:
            row = conn.execute(
                "SELECT value FROM schema_meta WHERE key = 'full_scan_done'"
            ).fetchone()
            return row is not None and row[0] == "1"
        finally:
            conn.close()

    def mark_full_scan_done(self) -> None:
        """フルスキャン完了フラグを設定する."""
        conn = self._connect()
        try:
            conn.execute(
                "INSERT OR REPLACE INTO schema_meta (key, value) VALUES (?, ?)",
                ("full_scan_done", "1"),
            )
            conn.commit()
        finally:
            conn.close()

    def mark_ready(self) -> None:
        """インデックスが使用可能な状態にする."""
        self._is_ready = True
        self._is_stale = False

    def mark_warm_start(self) -> None:
        """既存データで即座にクエリを提供する (stale)."""
        self._is_ready = True
        self._is_stale = True
