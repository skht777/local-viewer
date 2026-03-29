#!/usr/bin/env bash
# 初回セットアップ
#
# 1. .env が存在しなければ .env.example からコピー
# 2. ホスト CPU 数に基づいて CPUS / SCAN_WORKERS を最適化
# 3. manage_mounts.sh でマウントポイントを設定
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# .env がなければ .env.example からコピー
if [[ ! -f "${SCRIPT_DIR}/.env" ]]; then
    cp "${SCRIPT_DIR}/.env.example" "${SCRIPT_DIR}/.env"
    echo ".env を作成しました。必要に応じて編集してください。"
fi

# CPU 数を自動検出して .env に反映
optimize_for_host() {
    local cpu_count
    cpu_count=$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 2)

    local env_file="${SCRIPT_DIR}/.env"

    # CPUS: Docker コンテナの CPU 制限
    if ! grep -q "^CPUS=" "$env_file"; then
        echo "CPUS=${cpu_count}.0" >> "$env_file"
    else
        sed -i "s/^CPUS=.*/CPUS=${cpu_count}.0/" "$env_file"
    fi

    # SCAN_WORKERS: インデックススキャンのスレッド数 (I/O バウンドなので CPU の 2 倍)
    local scan_workers=$((cpu_count * 2))
    if ! grep -q "^SCAN_WORKERS=" "$env_file"; then
        echo "SCAN_WORKERS=${scan_workers}" >> "$env_file"
    else
        sed -i "s/^SCAN_WORKERS=.*/SCAN_WORKERS=${scan_workers}/" "$env_file"
    fi

    echo "ホスト CPU: ${cpu_count} コア → CPUS=${cpu_count}.0, SCAN_WORKERS=${scan_workers}"
}

optimize_for_host

# マウントポイント設定
"${SCRIPT_DIR}/manage_mounts.sh"
