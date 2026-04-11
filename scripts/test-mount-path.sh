#!/usr/bin/env bash
# convert-mount-path.sh の単体テストランナー
#
# 正常系: scripts/mount-path-cases.tsv の 3 列目 (sh_expected) を検証
# 不正系: scripts/mount-path-invalid-cases.tsv + ハードコード制御文字ケースで
#         validate_host_path が非 0 を返すことを検証
#
# 終了コード: 0=全 pass, 1=1 件以上失敗
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=convert-mount-path.sh
source "${SCRIPT_DIR}/convert-mount-path.sh"

CASES_TSV="${SCRIPT_DIR}/mount-path-cases.tsv"
INVALID_TSV="${SCRIPT_DIR}/mount-path-invalid-cases.tsv"

pass=0
fail=0

# 正常系 (convert_host_path_to_wsl_format)
while IFS=$'\t' read -r input _ps_expected sh_expected; do
    [[ -z "$input" ]] && continue
    actual=$(convert_host_path_to_wsl_format "$input")
    if [[ "$actual" == "$sh_expected" ]]; then
        pass=$((pass + 1))
    else
        printf 'FAIL [convert]: %s\n  expected: %s\n  actual:   %s\n' \
            "$input" "$sh_expected" "$actual" >&2
        fail=$((fail + 1))
    fi
done < "$CASES_TSV"

# 正常系パスは validate_host_path も通ること (converted 後)
while IFS=$'\t' read -r input _ps_expected sh_expected; do
    [[ -z "$input" ]] && continue
    if validate_host_path "$sh_expected"; then
        pass=$((pass + 1))
    else
        printf 'FAIL [validate-valid]: %s (converted=%s) が拒否された\n' \
            "$input" "$sh_expected" >&2
        fail=$((fail + 1))
    fi
done < "$CASES_TSV"

# 不正系 TSV (validate_host_path)
while IFS=$'\t' read -r input reason; do
    [[ -z "$input" ]] && continue
    if validate_host_path "$input"; then
        printf 'FAIL [validate-invalid]: %q (%s) が拒否されなかった\n' \
            "$input" "$reason" >&2
        fail=$((fail + 1))
    else
        pass=$((pass + 1))
    fi
done < "$INVALID_TSV"

# 不正系 (制御文字 - TSV では表現しにくいためハードコード)
declare -a control_cases=(
    $'C:\\foo\nevil: bar'
    $'C:\\foo\tBAD'
    $'C:\\foo\rCR'
)
for input in "${control_cases[@]}"; do
    if validate_host_path "$input"; then
        printf 'FAIL [validate-ctrl]: 制御文字入力が拒否されなかった\n' >&2
        fail=$((fail + 1))
    else
        pass=$((pass + 1))
    fi
done

# slug 検証
declare -a valid_slugs=("ai" "photos" "my-data" "set.01" "A_B_C")
declare -a invalid_slugs=("" "." ".." "foo/bar" "foo\\bar" "foo bar" "foo#bar")

for slug in "${valid_slugs[@]}"; do
    if validate_slug "$slug"; then
        pass=$((pass + 1))
    else
        printf 'FAIL [slug-valid]: %q が拒否された\n' "$slug" >&2
        fail=$((fail + 1))
    fi
done

for slug in "${invalid_slugs[@]}"; do
    if validate_slug "$slug"; then
        printf 'FAIL [slug-invalid]: %q が拒否されなかった\n' "$slug" >&2
        fail=$((fail + 1))
    else
        pass=$((pass + 1))
    fi
done

echo ""
echo "結果: ${pass} passed, ${fail} failed"
exit $((fail > 0 ? 1 : 0))
