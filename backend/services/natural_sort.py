"""Windows Explorer 互換の自然順ソートキー."""

import re

_SPLIT_RE = re.compile(r"(\d+)")


def natural_sort_key(name: str) -> tuple[int | str, ...]:
    """ファイル名を自然順ソート用のキーに変換する.

    文字列を「テキスト部分」と「数値部分」に分割し、
    数値部分を int に変換してタプル比較することで
    file1, file2, file10 の順にソートする。
    """
    parts = _SPLIT_RE.split(name.lower())
    return tuple(int(p) if p.isdigit() else p for p in parts)
