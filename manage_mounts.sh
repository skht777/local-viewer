#!/usr/bin/env bash
# マウントポイント管理 TUI
#
# ホスト側で実行し、config/mounts.json と docker-compose.override.yml を更新する。
# 依存: jq, uuidgen
#
# 使い方:
#   ./manage_mounts.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OVERRIDE_FILE="${SCRIPT_DIR}/docker-compose.override.yml"
CONFIG_DIR="${SCRIPT_DIR}/config"
MOUNTS_JSON="${CONFIG_DIR}/mounts.json"
ENV_FILE="${SCRIPT_DIR}/.env"

# shellcheck source=scripts/convert-mount-path.sh
source "${SCRIPT_DIR}/scripts/convert-mount-path.sh"

# jq, uuidgen の存在を確認する
check_dependencies() {
    local missing=()
    command -v jq >/dev/null 2>&1 || missing+=("jq")
    command -v uuidgen >/dev/null 2>&1 || missing+=("uuidgen")
    if [[ ${#missing[@]} -gt 0 ]]; then
        echo "エラー: 以下のコマンドが必要です: ${missing[*]}"
        echo "インストール例:"
        echo "  Ubuntu/Debian: sudo apt install ${missing[*]}"
        echo "  macOS:         brew install ${missing[*]}"
        exit 1
    fi
}

# .env から MOUNT_BASE_DIR を読み込む (デフォルト: /mnt-host)
load_env() {
    MOUNT_BASE_DIR="/mnt-host"
    if [[ -f "$ENV_FILE" ]]; then
        local val
        val=$(grep -E '^MOUNT_BASE_DIR=' "$ENV_FILE" 2>/dev/null | head -1 | cut -d= -f2-)
        if [[ -n "$val" ]]; then
            MOUNT_BASE_DIR="$val"
        fi
    fi
}

# config ディレクトリと空の mounts.json を初期化する
ensure_config() {
    mkdir -p "$CONFIG_DIR"
    if [[ ! -f "$MOUNTS_JSON" ]]; then
        echo '{"version": 2, "mounts": []}' | jq . > "$MOUNTS_JSON"
    fi
}

# mounts.json が v1 なら v2 に変換する
migrate_v1_to_v2() {
    local version
    version=$(jq -r '.version // 1' "$MOUNTS_JSON")
    if [[ "$version" -ge 2 ]]; then
        return 0
    fi

    echo "mounts.json を v1 から v2 にマイグレーションします..."

    # host_path を空文字列で追加、path → slug に変換、version を 2 に更新
    local tmp
    tmp=$(jq --arg base "$MOUNT_BASE_DIR" '
        .version = 2 |
        .mounts = [.mounts[] |
            {
                mount_id: .mount_id,
                name: .name,
                slug: (
                    if .path == $base then "."
                    elif (.path | startswith($base + "/")) then (.path | ltrimstr($base + "/"))
                    else (.path | split("/") | last)
                    end
                ),
                host_path: ""
            }
        ]
    ' "$MOUNTS_JSON")
    echo "$tmp" > "$MOUNTS_JSON"

    echo "警告: マイグレーション完了。既存マウントの host_path は未設定です。"
    echo "  [e] 編集で各マウントのホストパスを設定してください。"
}

# マウントポイント一覧を表示する
show_mounts() {
    echo ""
    echo "MOUNT_BASE_DIR: ${MOUNT_BASE_DIR}"
    echo ""

    local count
    count=$(jq '.mounts | length' "$MOUNTS_JSON")
    if [[ "$count" -eq 0 ]]; then
        echo "  マウントポイントが登録されていません"
        return
    fi

    local i=0
    while [[ $i -lt $count ]]; do
        local name slug host_path mount_id
        mount_id=$(jq -r ".mounts[$i].mount_id" "$MOUNTS_JSON")
        name=$(jq -r ".mounts[$i].name" "$MOUNTS_JSON")
        slug=$(jq -r ".mounts[$i].slug" "$MOUNTS_JSON")
        host_path=$(jq -r ".mounts[$i].host_path" "$MOUNTS_JSON")
        printf "  %d. [%s] %s\n" "$((i + 1))" "$mount_id" "$name"
        if [[ -n "$host_path" ]]; then
            printf "     %s → %s/%s\n" "$host_path" "$MOUNT_BASE_DIR" "$slug"
        else
            printf "     → %s/%s [host_path 未設定]\n" "$MOUNT_BASE_DIR" "$slug"
        fi
        i=$((i + 1))
    done
}

# ホストパスの存在を確認する
validate_host_path() {
    local path="$1"
    if [[ ! -d "$path" ]]; then
        echo "エラー: ディレクトリが存在しません: $path"
        return 1
    fi
    return 0
}

# basename から slug を生成する (小文字化、スペース→ハイフン、特殊文字除去)
generate_slug() {
    local name="$1"
    echo "$name" | tr '[:upper:]' '[:lower:]' | tr ' ' '-' | sed 's/[^a-z0-9._-]//g'
}

# 既存 slug との衝突を検出する
check_slug_conflict() {
    local slug="$1"
    local existing
    existing=$(jq -r ".mounts[].slug" "$MOUNTS_JSON")
    echo "$existing" | grep -qx "$slug" && return 1
    return 0
}

# mount_id を生成する (16文字 hex)
generate_mount_id() {
    uuidgen | tr -d '-' | head -c 16
}

# マウントポイントを追加する
add_mount() {
    echo ""
    read -r -p "ホスト側パス: " host_path
    if [[ -z "$host_path" ]]; then
        echo "キャンセルしました"
        return
    fi

    if ! validate_host_path "$host_path"; then
        return
    fi

    # slug の生成: ディレクトリ名から導出
    local dirname
    dirname=$(basename "$host_path")
    local default_slug
    default_slug=$(generate_slug "$dirname")

    read -r -p "slug [$default_slug]: " slug
    slug="${slug:-$default_slug}"

    # slug バリデーション
    if [[ -z "$slug" || "$slug" == "." || "$slug" == ".." ]]; then
        echo "エラー: 無効な slug です"
        return
    fi
    if [[ "$slug" == */* || "$slug" == *\\* ]]; then
        echo "エラー: slug にパス区切り文字は使えません"
        return
    fi

    # 衝突チェック
    if ! check_slug_conflict "$slug"; then
        echo "エラー: slug '$slug' は既に登録されています"
        return
    fi

    # 表示名
    local default_name="$dirname"
    read -r -p "表示名 [$default_name]: " name
    name="${name:-$default_name}"

    # mounts.json に追加
    local mount_id
    mount_id=$(generate_mount_id)
    local tmp
    tmp=$(jq --arg id "$mount_id" --arg name "$name" --arg slug "$slug" --arg hp "$host_path" \
        '.mounts += [{"mount_id": $id, "name": $name, "slug": $slug, "host_path": $hp}]' \
        "$MOUNTS_JSON")
    echo "$tmp" > "$MOUNTS_JSON"

    echo ""
    echo "追加しました: [$mount_id] $name"

    # docker-compose.yml を同期
    sync_compose "$MOUNTS_JSON" "$OVERRIDE_FILE" "$MOUNT_BASE_DIR" "manage_mounts.sh"
    echo "docker compose up -d で反映してください"
}

# マウントポイント名を編集する
edit_mount() {
    local count
    count=$(jq '.mounts | length' "$MOUNTS_JSON")
    if [[ "$count" -eq 0 ]]; then
        echo "マウントポイントが登録されていません"
        return
    fi

    echo ""
    read -r -p "編集する番号: " idx_str
    local idx=$((idx_str - 1))
    if [[ $idx -lt 0 || $idx -ge $count ]]; then
        echo "無効な番号です"
        return
    fi

    local current_name
    current_name=$(jq -r ".mounts[$idx].name" "$MOUNTS_JSON")
    read -r -p "新しい表示名 [$current_name]: " new_name
    if [[ -z "$new_name" ]]; then
        echo "キャンセルしました"
        return
    fi

    local tmp
    tmp=$(jq --arg name "$new_name" --argjson idx "$idx" \
        '.mounts[$idx].name = $name' "$MOUNTS_JSON")
    echo "$tmp" > "$MOUNTS_JSON"

    echo "更新しました: $new_name"
}

# マウントポイントを削除する
delete_mount() {
    local count
    count=$(jq '.mounts | length' "$MOUNTS_JSON")
    if [[ "$count" -eq 0 ]]; then
        echo "マウントポイントが登録されていません"
        return
    fi

    echo ""
    read -r -p "削除する番号: " idx_str
    local idx=$((idx_str - 1))
    if [[ $idx -lt 0 || $idx -ge $count ]]; then
        echo "無効な番号です"
        return
    fi

    local name
    name=$(jq -r ".mounts[$idx].name" "$MOUNTS_JSON")
    read -r -p "'$name' を削除しますか? [y/N]: " confirm
    if [[ "$confirm" != "y" ]]; then
        echo "キャンセルしました"
        return
    fi

    local tmp
    tmp=$(jq --argjson idx "$idx" 'del(.mounts[$idx])' "$MOUNTS_JSON")
    echo "$tmp" > "$MOUNTS_JSON"

    echo "削除しました: $name"

    # docker-compose.yml を同期
    sync_compose "$MOUNTS_JSON" "$OVERRIDE_FILE" "$MOUNT_BASE_DIR" "manage_mounts.sh"
    echo "docker compose up -d で反映してください"
}

# TUI メインループ
main() {
    echo "Local Content Viewer - マウントポイント管理"

    check_dependencies
    load_env
    ensure_config
    migrate_v1_to_v2

    while true; do
        show_mounts
        echo ""
        echo "操作を選択: [a] 追加  [e] 編集  [d] 削除  [q] 終了"
        read -r -p "> " choice

        case "$choice" in
            a) add_mount ;;
            e) edit_mount ;;
            d) delete_mount ;;
            q) break ;;
            *) echo "無効な選択です" ;;
        esac
    done
}

# テストから source 可能にするため、直接実行時のみ main を呼ぶ
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
