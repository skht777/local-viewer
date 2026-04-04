"""browse API のカーソルベースページネーション.

- カーソル: base64 エンコード JSON + HMAC-SHA256 署名
- ソート順: name-asc, name-desc, date-asc, date-desc
- 位置復元: node_id + ソートキー (名前/日付/サイズ) で一意に特定
- 改ざん耐性: NODE_SECRET を使った HMAC 署名で検証
"""

from __future__ import annotations

import base64
import hashlib
import hmac
import json
import os
from enum import StrEnum
from typing import TYPE_CHECKING

from backend.services.natural_sort import natural_sort_key

if TYPE_CHECKING:
    from backend.services.node_registry import EntryMeta


class SortOrder(StrEnum):
    """ソート順序."""

    NAME_ASC = "name-asc"
    NAME_DESC = "name-desc"
    DATE_ASC = "date-asc"
    DATE_DESC = "date-desc"


# ページネーションのデフォルト値
DEFAULT_LIMIT = 100
MAX_LIMIT = 500


def _get_secret() -> bytes:
    return os.environ.get("NODE_SECRET", "local-viewer-default-secret").encode()


def encode_cursor(
    sort: SortOrder,
    last_entry: EntryMeta,
    etag: str,
) -> str:
    """カーソルを base64 + HMAC 署名でエンコードする."""
    payload = {
        "s": sort.value,
        "id": last_entry.node_id,
        "n": last_entry.name,
        "d": last_entry.kind == "directory",
        "m": last_entry.modified_at,
        "sz": last_entry.size_bytes,
        "et": etag,
    }
    payload_json = json.dumps(payload, separators=(",", ":"), sort_keys=True)
    sig = hmac.new(_get_secret(), payload_json.encode(), hashlib.sha256).hexdigest()[
        :16
    ]
    payload["sig"] = sig
    signed_json = json.dumps(payload, separators=(",", ":"), sort_keys=True)
    return base64.urlsafe_b64encode(signed_json.encode()).decode()


def decode_cursor(cursor_str: str, expected_sort: SortOrder) -> dict[str, object]:
    """カーソルをデコードし、署名と整合性を検証する.

    Raises:
        ValueError: 署名不一致、ソート順不一致、不正なフォーマット
    """
    try:
        raw = base64.urlsafe_b64decode(cursor_str).decode()
        data = json.loads(raw)
    except Exception as exc:
        msg = "不正なカーソルフォーマットです"
        raise ValueError(msg) from exc

    # 署名検証
    sig = data.pop("sig", None)
    if sig is None:
        msg = "カーソル署名がありません"
        raise ValueError(msg)

    payload_json = json.dumps(data, separators=(",", ":"), sort_keys=True)
    expected_sig = hmac.new(
        _get_secret(), payload_json.encode(), hashlib.sha256
    ).hexdigest()[:16]
    if not hmac.compare_digest(sig, expected_sig):
        msg = "カーソル署名が不正です"
        raise ValueError(msg)

    # ソート順の一致確認
    if data.get("s") != expected_sort.value:
        msg = "カーソルのソート順がリクエストと一致しません"
        raise ValueError(msg)

    result: dict[str, object] = data
    return result


def sort_entries(entries: list[EntryMeta], sort: SortOrder) -> list[EntryMeta]:
    """エントリをソート順に並び替える."""
    if sort == SortOrder.NAME_ASC:
        return sorted(
            entries,
            key=lambda e: (e.kind != "directory", natural_sort_key(e.name)),
        )
    if sort == SortOrder.NAME_DESC:
        return sorted(
            entries,
            key=lambda e: (e.kind != "directory", natural_sort_key(e.name)),
            reverse=True,
        )
    if sort == SortOrder.DATE_DESC:
        return sorted(
            entries,
            key=lambda e: (e.modified_at is None, -(e.modified_at or 0)),
        )
    # DATE_ASC
    return sorted(
        entries,
        key=lambda e: (e.modified_at is None, e.modified_at or 0),
    )


def apply_cursor(
    entries: list[EntryMeta],
    cursor_data: dict[str, object],
) -> list[EntryMeta]:
    """カーソル位置以降のエントリを返す.

    cursor_data の node_id で一致するエントリを見つけ、その次から返す。
    """
    cursor_id = cursor_data.get("id")
    for i, entry in enumerate(entries):
        if entry.node_id == cursor_id:
            return entries[i + 1 :]
    # カーソルのエントリが見つからない場合は先頭から (フォールバック)
    return entries


def paginate(
    entries: list[EntryMeta],
    sort: SortOrder,
    limit: int | None = None,
    cursor: str | None = None,
    etag: str = "",
) -> tuple[list[EntryMeta], str | None, int]:
    """ソート・ページネーションを適用する.

    Returns:
        (page_entries, next_cursor, total_count)
    """
    total_count = len(entries)

    # ソート
    sorted_entries = sort_entries(entries, sort)

    # カーソル適用
    if cursor:
        cursor_data = decode_cursor(cursor, sort)
        sorted_entries = apply_cursor(sorted_entries, cursor_data)

    # limit 省略時は全件返却 (後方互換)
    if limit is None:
        return sorted_entries, None, total_count

    page = sorted_entries[:limit]

    # next_cursor 生成
    next_cursor = None
    if len(sorted_entries) > limit:
        next_cursor = encode_cursor(sort, page[-1], etag)

    return page, next_cursor, total_count
