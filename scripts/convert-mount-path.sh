#!/usr/bin/env bash
# マウントパス変換と docker-compose.override.yml 生成
#
# このファイルは source 専用 (直接実行しない)。
# start.sh / manage_mounts.sh から source して sync_compose 関数を利用する。
#
# 使い方:
#   source "${SCRIPT_DIR}/scripts/convert-mount-path.sh"
#   sync_compose <mounts_json> <override_file> <mount_base_dir> <caller_name>
#
# 提供する関数:
#   convert_host_path_to_wsl_format <path>  - Windows パス → WSL 形式に逆変換
#   validate_host_path <path>                - YAML インジェクション検証 (0=OK, 1=NG)
#   validate_slug <slug>                     - slug 再検証 (0=OK, 1=NG)
#   sync_compose ...                         - override YAML 生成

# Windows 形式のパスを WSL (/mnt/<drive>/...) 形式に逆変換する。
# Linux パス・UNC パスは変換せずそのまま返す。
#
# 許容入力:
#   C:\Users\foo          → /mnt/c/Users/foo
#   C:/Users/foo          → /mnt/c/Users/foo
#   c:\data               → /mnt/c/data
#   /mnt/d/AI             → /mnt/d/AI (不変)
#   \\nas\share\pics      → \\nas\share\pics (UNC 不変)
#   /home/user/pics       → /home/user/pics (Linux 不変)
#   /a/foo                → /a/foo (Git Bash 非サポートのため Linux 扱い)
convert_host_path_to_wsl_format() {
    local path="$1"

    # UNC パス (\\server\share) はそのまま
    if [[ "$path" == \\\\* ]]; then
        printf '%s\n' "$path"
        return
    fi

    # 既に /mnt/<drive>/... 形式なら不変
    if [[ "$path" =~ ^/mnt/[a-zA-Z](/.*)?$ ]]; then
        printf '%s\n' "$path"
        return
    fi

    # Windows ドライブ (C:\ or C:/)
    if [[ "$path" =~ ^([a-zA-Z]):[\\/](.*)$ ]]; then
        local drive="${BASH_REMATCH[1],,}"
        local rest="${BASH_REMATCH[2]//\\//}"
        printf '/mnt/%s/%s\n' "$drive" "$rest"
        return
    fi

    # Linux 絶対パス (/home/... 等) はそのまま
    printf '%s\n' "$path"
}

# host_path の安全性を検証する (YAML インジェクション対策)。
# 許可文字のみで構成されていれば 0、それ以外は 1 を返す。
#
# 拒否条件:
#   - 改行 / タブ / NUL / 制御文字
#   - YAML 特殊な先頭文字 (- ? : ! & * | > % @ ` #)
#   - compose 変数展開 (${...})
#   - ドライブ区切り以外の余分なコロン
#   - 空白 / 空文字列
validate_host_path() {
    local path="$1"

    # 空文字列
    if [[ -z "$path" ]]; then
        return 1
    fi

    # 制御文字 (NUL, LF, CR, TAB など 0x00-0x1F, 0x7F)
    if [[ "$path" == *$'\n'* || "$path" == *$'\r'* || "$path" == *$'\t'* ]]; then
        return 1
    fi
    # 上記以外の制御文字も弾く
    if LC_ALL=C printf '%s' "$path" | LC_ALL=C grep -q '[[:cntrl:]]'; then
        return 1
    fi

    # 空白文字
    if [[ "$path" == *' '* ]]; then
        return 1
    fi

    # compose 変数展開 ${...}
    # shellcheck disable=SC2016
    if [[ "$path" == *'${'* ]]; then
        return 1
    fi

    # YAML コメント開始
    if [[ "$path" == *'#'* ]]; then
        return 1
    fi

    # 先頭文字が YAML 特殊な場合
    case "${path:0:1}" in
        -|\?|:|!|\&|\*|\||\>|%|@|\`)
            return 1
            ;;
    esac

    # ドライブ区切り以外の余分なコロン
    # 許容: "C:\..." "C:/..." のようにドライブ直後のみ
    local colon_count="${path//[^:]/}"
    colon_count="${#colon_count}"
    if (( colon_count > 1 )); then
        return 1
    fi
    if (( colon_count == 1 )); then
        # 1 つ含まれる場合は [A-Za-z]:[\\/] パターンであること
        if ! [[ "$path" =~ ^[A-Za-z]:[\\/] ]]; then
            return 1
        fi
    fi

    return 0
}

# slug の安全性を再検証する。
#
# 許可: 英数字 + . _ -
# 拒否: 空文字列, ".", "..", /, \, NUL, 制御文字, その他記号
validate_slug() {
    local slug="$1"

    if [[ -z "$slug" || "$slug" == "." || "$slug" == ".." ]]; then
        return 1
    fi

    # shellcheck disable=SC1003
    if [[ "$slug" == *'/'* || "$slug" == *'\'* ]]; then
        return 1
    fi

    if [[ "$slug" == *$'\n'* || "$slug" == *$'\r'* || "$slug" == *$'\t'* ]]; then
        return 1
    fi
    if LC_ALL=C printf '%s' "$slug" | LC_ALL=C grep -q '[[:cntrl:]]'; then
        return 1
    fi

    if ! [[ "$slug" =~ ^[A-Za-z0-9._-]+$ ]]; then
        return 1
    fi

    return 0
}


# mounts.json から docker-compose.override.yml を生成する。
#
# 各エントリに対して以下を順に実行:
#   1. host_path を convert_host_path_to_wsl_format で正規化
#   2. validate_host_path で YAML インジェクション検証 → NG なら警告してスキップ
#   3. validate_slug で slug 再検証 → NG なら警告してスキップ
#   4. "      - <host>:<base>/<slug>:ro" の形式で YAML に追記
#
# 引数:
#   $1 mounts_json     - mounts.json の絶対パス
#   $2 override_file   - 書き出す docker-compose.override.yml の絶対パス
#   $3 mount_base_dir  - コンテナ内のマウント親ディレクトリ (例: /mnt-host)
#   $4 caller_name     - ヘッダーコメントに記載する呼び出し元スクリプト名
sync_compose() {
    local mounts_json="$1"
    local override_file="$2"
    local mount_base_dir="$3"
    local caller_name="$4"

    if [[ ! -f "$mounts_json" ]]; then
        rm -f "$override_file"
        return
    fi

    local mount_lines=""
    local line host_path slug converted
    # jq から NUL 区切りで host_path と slug を交互に出力させる。
    # NUL は validate_host_path / validate_slug が拒否する文字なので
    # 区切り子として安全に使える。
    while IFS= read -r -d '' host_path && IFS= read -r -d '' slug; do
        if ! validate_slug "$slug"; then
            printf 'warning: slug を拒否しました: %q\n' "$slug" >&2
            continue
        fi

        converted=$(convert_host_path_to_wsl_format "$host_path")

        if ! validate_host_path "$converted"; then
            printf 'warning: host_path を拒否しました: %q\n' "$host_path" >&2
            continue
        fi

        # shellcheck disable=SC2016
        line=$(printf '      - %s:${MOUNT_BASE_DIR:-%s}/%s:ro' \
            "$converted" "$mount_base_dir" "$slug")
        if [[ -z "$mount_lines" ]]; then
            mount_lines="$line"
        else
            mount_lines="${mount_lines}"$'\n'"$line"
        fi
    done < <(jq -j '.mounts[] | .host_path + "\u0000" + .slug + "\u0000"' "$mounts_json" 2>/dev/null || true)

    # マウントが空なら override ファイルを削除
    if [[ -z "$mount_lines" ]]; then
        rm -f "$override_file"
        return
    fi

    local timestamp
    timestamp=$(date -u '+%Y-%m-%dT%H:%M:%SZ')

    # アトミック書き込み (tmp → mv)
    local tmp_file
    tmp_file=$(mktemp "${override_file}.XXXXXX")
    cat > "$tmp_file" <<EOF
# Auto-generated by ${caller_name} at ${timestamp} -- do not edit manually
services:
  viewer:
    volumes:
${mount_lines}
EOF
    mv "$tmp_file" "$override_file"
}
