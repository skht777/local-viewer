#!/usr/bin/env bash
# 初回セットアップ
#
# 1. .env が存在しなければ .env.example からコピー
# 2. manage_mounts.sh でマウントポイントを設定
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# .env がなければ .env.example からコピー
if [[ ! -f "${SCRIPT_DIR}/.env" ]]; then
    cp "${SCRIPT_DIR}/.env.example" "${SCRIPT_DIR}/.env"
    echo ".env を作成しました。必要に応じて編集してください。"
fi

# マウントポイント設定
"${SCRIPT_DIR}/manage_mounts.sh"
