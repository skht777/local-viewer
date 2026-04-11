#!/usr/bin/env bash
# Docker コンテナ起動 (ビルド + 起動)
#
# 起動前に mounts.json から docker-compose.override.yml を再生成する。
# PowerShell 版 start.ps1 と対称で、WSL2 / Linux 環境向けにパスを
# /mnt/<drive>/... 形式に逆変換する。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OVERRIDE_FILE="${SCRIPT_DIR}/docker-compose.override.yml"
MOUNTS_JSON="${SCRIPT_DIR}/config/mounts.json"
ENV_FILE="${SCRIPT_DIR}/.env"

# shellcheck source=scripts/convert-mount-path.sh
source "${SCRIPT_DIR}/scripts/convert-mount-path.sh"

# 依存コマンドを確認する
check_dependencies() {
    local missing=()
    command -v jq >/dev/null 2>&1 || missing+=("jq")
    command -v docker >/dev/null 2>&1 || missing+=("docker")
    if [[ ${#missing[@]} -gt 0 ]]; then
        echo "エラー: 以下のコマンドが必要です: ${missing[*]}" >&2
        echo "インストール例:" >&2
        echo "  Ubuntu/Debian: sudo apt install ${missing[*]}" >&2
        echo "  macOS:         brew install ${missing[*]}" >&2
        exit 1
    fi
}

# .env から MOUNT_BASE_DIR を読み込む (デフォルト: /mnt-host)
load_mount_base_dir() {
    MOUNT_BASE_DIR="/mnt-host"
    if [[ -f "$ENV_FILE" ]]; then
        local val
        val=$(grep -E '^MOUNT_BASE_DIR=' "$ENV_FILE" 2>/dev/null | head -1 | cut -d= -f2-)
        if [[ -n "$val" ]]; then
            MOUNT_BASE_DIR="$val"
        fi
    fi
}

check_dependencies
load_mount_base_dir
sync_compose "$MOUNTS_JSON" "$OVERRIDE_FILE" "$MOUNT_BASE_DIR" "start.sh"

docker compose up --build
