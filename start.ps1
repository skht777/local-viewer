#Requires -Version 7.0
# Docker コンテナ起動 (Windows / PowerShell 7+ 版)
#
# 起動前に config\mounts.json から docker-compose.override.yml を再生成する。
# bash 版 start.sh と対称で、Docker Desktop (Windows) 向けにパスを
# C:/<drive>/... 形式に順変換する。
#
# WSL2 からの呼び出しにも対応: powershell.exe -File .\start.ps1 で起動すると
# $PSScriptRoot を基準に動作し、呼び出し元 CWD に依存しない。
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$scriptDir = $PSScriptRoot
Set-Location -LiteralPath $scriptDir

. (Join-Path $scriptDir 'scripts\Convert-MountPath.ps1')

$overrideFile = Join-Path $scriptDir 'docker-compose.override.yml'
$mountsJson = Join-Path $scriptDir 'config\mounts.json'
$envFile = Join-Path $scriptDir '.env'

# 依存コマンドを確認する
if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    Write-Error 'docker コマンドが見つかりません。Docker Desktop をインストールしてください。'
    exit 1
}

# .env から MOUNT_BASE_DIR を読み込む (デフォルト: /mnt-host)
$mountBaseDir = '/mnt-host'
if (Test-Path -LiteralPath $envFile) {
    $match = Select-String -Path $envFile -Pattern '^MOUNT_BASE_DIR=(.*)$' | Select-Object -First 1
    if ($match -and $match.Matches[0].Groups[1].Value) {
        $mountBaseDir = $match.Matches[0].Groups[1].Value
    }
}

Sync-ComposeOverride `
    -MountsJson $mountsJson `
    -OverrideFile $overrideFile `
    -MountBaseDir $mountBaseDir `
    -CallerName 'start.ps1'

# Docker Desktop の WSL 統合環境では start.sh (WSL) と同じデーモンを共有するため、
# ボリューム/コンテナ名が衝突しないよう Windows 側は専用プロジェクト名を使う。
# ユーザーが既に COMPOSE_PROJECT_NAME を設定している場合は尊重する。
if (-not $env:COMPOSE_PROJECT_NAME) {
    $env:COMPOSE_PROJECT_NAME = 'local-viewer-win'
}

docker compose up --build
