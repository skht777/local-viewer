#!/usr/bin/env bash
# 初回セットアップ
#
# 1. .env が存在しなければ .env.example からコピー
# 2. ホスト CPU 数に基づいて CPUS / SCAN_WORKERS を最適化
# 3. NODE_SECRET が未設定なら 32 バイトのランダム hex を自動生成
# 4. manage_mounts.sh でマウントポイントを設定
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

# NODE_SECRET が空なら 32 バイトの hex をランダム生成して .env に書き戻す
# - 既に値が入っている場合は尊重し上書きしない
# - openssl が無ければ /dev/urandom にフォールバック
ensure_node_secret() {
    local env_file="${SCRIPT_DIR}/.env"
    local current
    current=$(grep -E "^NODE_SECRET=" "$env_file" | head -n1 | cut -d= -f2- || true)

    if [[ -n "${current}" ]]; then
        return
    fi

    local secret
    if command -v openssl >/dev/null 2>&1; then
        secret=$(openssl rand -hex 32)
    elif [[ -r /dev/urandom ]]; then
        secret=$(head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n')
    else
        echo "警告: NODE_SECRET を自動生成できませんでした (openssl も /dev/urandom も利用不可)。.env を手動編集してください。" >&2
        return
    fi

    if grep -q "^NODE_SECRET=" "$env_file"; then
        # hex のみ (0-9a-f) なので sed 区切り文字の衝突は起きない
        sed -i "s/^NODE_SECRET=.*/NODE_SECRET=${secret}/" "$env_file"
    else
        echo "NODE_SECRET=${secret}" >> "$env_file"
    fi

    echo "NODE_SECRET を自動生成しました。"
}

ensure_node_secret

# マウントポイント設定
"${SCRIPT_DIR}/manage_mounts.sh"
