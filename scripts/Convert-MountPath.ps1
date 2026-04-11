# マウントパス変換と docker-compose.override.yml 生成 (PowerShell 版)
#
# このファイルは dot-source 専用:
#   . "${PSScriptRoot}\scripts\Convert-MountPath.ps1"
#
# 提供する関数:
#   Convert-HostPathToDockerFormat  - Windows パス → C:/... 正規形へ順変換
#   Test-HostPath                   - YAML インジェクション検証 (true/false)
#   Test-Slug                       - slug 再検証 (true/false)
#   Sync-ComposeOverride            - mounts.json → override YAML 生成

# Windows 形式のパスを Docker Desktop 互換の C:/<drive>/... 形式に順変換する。
# Linux パス・UNC パスは変換せずそのまま返す。
#
# 許容入力:
#   C:\Users\foo       → C:/Users/foo
#   C:/Users/foo       → C:/Users/foo
#   c:\data            → C:/data
#   /mnt/d/AI          → D:/AI
#   \\nas\share\pics   → \\nas\share\pics (UNC 不変)
#   /home/user/pics    → /home/user/pics  (Linux 不変)
#   /a/foo             → /a/foo           (Git Bash 非サポート → Linux 扱い)
function Convert-HostPathToDockerFormat {
    param([Parameter(Mandatory)][string]$Path)

    # UNC パスはそのまま
    if ($Path -match '^\\\\') {
        return $Path
    }

    # WSL2 /mnt/<drive>/... → <DRIVE>:/...
    # /mnt/c 単体のようにルート直下のみの場合は "C:/" に正規化する
    if ($Path -match '^/mnt/([a-zA-Z])(/.*)?$') {
        $drive = $Matches[1].ToUpper()
        $rest = if ($Matches[2]) { $Matches[2] } else { '/' }
        return "${drive}:${rest}"
    }

    # Windows ドライブ (C:\ or C:/) → <DRIVE>:/...
    if ($Path -match '^([a-zA-Z]):[\\/](.*)$') {
        $drive = $Matches[1].ToUpper()
        $rest = $Matches[2] -replace '\\', '/'
        return "${drive}:/${rest}"
    }

    # Linux 絶対パスはそのまま
    return $Path
}

# host_path の安全性を検証する (YAML インジェクション対策)。
# 許可文字のみで構成されていれば $true、それ以外は $false を返す。
#
# この関数は Convert-HostPathToDockerFormat による変換**後**のパスを検証する。
# 変換後の形式は <DRIVE>:/... / /home/... / \\server\share のいずれか。
#
# 拒否条件:
#   - 空文字列
#   - 改行 / タブ / NUL / 制御文字
#   - 空白
#   - compose 変数展開 ($VAR / ${VAR})
#   - YAML コメント開始 (#)
#   - チルダ展開 (~)
#   - YAML 特殊な先頭文字 (- ? : ! & * | > % @ `)
#   - ドライブ区切り以外の余分なコロン
#   - 相対パス (絶対パス / UNC 以外)
function Test-HostPath {
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Path)

    if ([string]::IsNullOrEmpty($Path)) { return $false }

    # 制御文字 (0x00-0x1F, 0x7F)
    if ($Path -match '[\x00-\x1F\x7F]') { return $false }

    # 空白文字
    if ($Path.Contains(' ')) { return $false }

    # compose 変数展開 ($VAR / ${VAR} の両方を拒否)
    if ($Path.Contains('$')) { return $false }

    # YAML コメント開始
    if ($Path.Contains('#')) { return $false }

    # チルダ展開
    if ($Path.StartsWith('~')) { return $false }

    # YAML 特殊な先頭文字
    $specialLeading = @('-', '?', ':', '!', '&', '*', '|', '>', '%', '@', '`')
    if ($specialLeading -contains $Path[0].ToString()) { return $false }

    # 絶対パスホワイトリスト: UNC (\\) / Windows 正規形 (<DRIVE>:/) / Unix (/)
    if ($Path -notmatch '^(\\\\|[A-Za-z]:/|/)') { return $false }

    # ドライブ区切り以外の余分なコロン
    # 許容: "<DRIVE>:/..." のようにドライブ直後の 1 つだけ
    $colonCount = ([regex]::Matches($Path, ':')).Count
    if ($colonCount -gt 1) { return $false }
    if ($colonCount -eq 1 -and $Path -notmatch '^[A-Za-z]:/') { return $false }

    return $true
}

# slug の安全性を再検証する。
#
# 許可: 英数字 + . _ -
# 拒否: 空文字列, ".", "..", /, \, 制御文字, その他記号
function Test-Slug {
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Slug)

    if ([string]::IsNullOrEmpty($Slug)) { return $false }
    if ($Slug -eq '.' -or $Slug -eq '..') { return $false }
    if ($Slug -match '[\\/]') { return $false }
    if ($Slug -match '[\x00-\x1F\x7F]') { return $false }
    if ($Slug -notmatch '^[A-Za-z0-9._-]+$') { return $false }
    return $true
}

# mounts.json から docker-compose.override.yml を生成する。
#
# 各エントリに対して以下を順に実行:
#   1. host_path を Convert-HostPathToDockerFormat で正規化
#   2. Test-Slug で slug 再検証 → NG なら警告してスキップ
#   3. Test-HostPath で YAML インジェクション検証 → NG なら警告してスキップ
#   4. "      - <host>:${MOUNT_BASE_DIR:-<base>}/<slug>:ro" の形式で YAML に追記
function Sync-ComposeOverride {
    param(
        [Parameter(Mandatory)][string]$MountsJson,
        [Parameter(Mandatory)][string]$OverrideFile,
        [Parameter(Mandatory)][string]$MountBaseDir,
        [Parameter(Mandatory)][string]$CallerName
    )

    if (-not (Test-Path -LiteralPath $MountsJson)) {
        if (Test-Path -LiteralPath $OverrideFile) {
            Remove-Item -LiteralPath $OverrideFile
        }
        return
    }

    $data = Get-Content -Raw -LiteralPath $MountsJson | ConvertFrom-Json
    $lines = [System.Collections.Generic.List[string]]::new()

    foreach ($mount in $data.mounts) {
        if (-not (Test-Slug $mount.slug)) {
            Write-Warning "slug を拒否しました: $($mount.slug)"
            continue
        }

        $converted = Convert-HostPathToDockerFormat $mount.host_path

        if (-not (Test-HostPath $converted)) {
            Write-Warning "host_path を拒否しました: $($mount.host_path)"
            continue
        }

        # ${MOUNT_BASE_DIR:-<base>} の部分は YAML リテラル。PowerShell の
        # 文字列補間と衝突しないよう format 演算子で組み立てる。
        $line = '      - {0}:${{MOUNT_BASE_DIR:-{1}}}/{2}:ro' -f `
            $converted, $MountBaseDir, $mount.slug
        $lines.Add($line)
    }

    if ($lines.Count -eq 0) {
        if (Test-Path -LiteralPath $OverrideFile) {
            Remove-Item -LiteralPath $OverrideFile
        }
        return
    }

    $timestamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    $header = "# Auto-generated by $CallerName at $timestamp -- do not edit manually"
    $volumesBlock = ($lines -join "`n")

    $content = @"
$header
services:
  viewer:
    volumes:
$volumesBlock
"@ + "`n"

    # BOM なし UTF-8 で書き出し (アトミックに: tmp → move)
    $tmpFile = "$OverrideFile.tmp"
    [System.IO.File]::WriteAllText($tmpFile, $content, [System.Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $tmpFile -Destination $OverrideFile -Force
}
