#!/usr/bin/env bash
# codex-review の指定された一時ファイル・ディレクトリを削除する
# 使用例: .claude/scripts/codex-review-cleanup.sh /tmp/codex-review-XXXXXX /tmp/codex-review-output-XXXXXX.md
set -euo pipefail

for path in "$@"; do
  # /tmp/codex-review- プレフィックスのパスのみ許可（安全弁）
  case "$path" in
    /tmp/codex-review-*) rm -rf "$path" ;;
    *) echo "Error: 許可されていないパス: $path" >&2; exit 1 ;;
  esac
done
