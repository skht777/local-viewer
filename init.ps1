#Requires -Version 7.0
# 初回セットアップ (Windows / PowerShell 7+ 版)
#
# 1. .env が存在しなければ .env.example からコピー
# 2. 論理 CPU 数に基づいて CPUS / SCAN_WORKERS を最適化
# 3. NODE_SECRET が未設定なら 32 バイトのランダム hex を自動生成
# 4. config/mounts.json が無ければ空スケルトンを作成
#
# TUI は起動しない。マウントポイント追加はユーザーが config\mounts.json を
# 手動編集する (または WSL2 から ./manage_mounts.sh を実行する)。
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$scriptDir = $PSScriptRoot
$envFile = Join-Path $scriptDir '.env'
$envExample = Join-Path $scriptDir '.env.example'
$configDir = Join-Path $scriptDir 'config'
$mountsJson = Join-Path $configDir 'mounts.json'

$utf8NoBom = [System.Text.UTF8Encoding]::new($false)

# .env を .env.example からコピー
if (-not (Test-Path -LiteralPath $envFile)) {
    Copy-Item -LiteralPath $envExample -Destination $envFile
    Write-Host '.env を作成しました。必要に応じて編集してください。'
}

# ホスト CPU 数に基づいて CPUS / SCAN_WORKERS を書き換える
$cpuCount = [int]$env:NUMBER_OF_PROCESSORS
if (-not $cpuCount -or $cpuCount -lt 1) { $cpuCount = 2 }
$scanWorkers = $cpuCount * 2

$envContent = [System.IO.File]::ReadAllText($envFile, $utf8NoBom)

function Set-EnvVar {
    param(
        [string]$Content,
        [string]$Key,
        [string]$Value
    )
    $pattern = "(?m)^$([regex]::Escape($Key))=.*$"
    if ($Content -match $pattern) {
        return ($Content -replace $pattern, "$Key=$Value")
    }
    $trimmed = $Content.TrimEnd("`n", "`r")
    return "$trimmed`n$Key=$Value`n"
}

$envContent = Set-EnvVar $envContent 'CPUS' "$cpuCount.0"
$envContent = Set-EnvVar $envContent 'SCAN_WORKERS' "$scanWorkers"

# NODE_SECRET が空なら 32 バイトの hex をランダム生成
# - 既に値が入っている場合は尊重し上書きしない
$currentSecret = ''
if ($envContent -match '(?m)^NODE_SECRET=(.*)$') {
    $currentSecret = $Matches[1]
}
if ([string]::IsNullOrEmpty($currentSecret)) {
    $bytes = New-Object byte[] 32
    [System.Security.Cryptography.RandomNumberGenerator]::Fill($bytes)
    $secret = -join ($bytes | ForEach-Object { $_.ToString('x2') })
    $envContent = Set-EnvVar $envContent 'NODE_SECRET' $secret
    Write-Host 'NODE_SECRET を自動生成しました。'
}

[System.IO.File]::WriteAllText($envFile, $envContent, $utf8NoBom)

Write-Host "ホスト CPU: $cpuCount コア → CPUS=$cpuCount.0, SCAN_WORKERS=$scanWorkers"

# config/mounts.json の初期化
if (-not (Test-Path -LiteralPath $configDir)) {
    New-Item -ItemType Directory -Path $configDir | Out-Null
}
if (-not (Test-Path -LiteralPath $mountsJson)) {
    $skeleton = @"
{
  "version": 2,
  "mounts": []
}
"@
    [System.IO.File]::WriteAllText($mountsJson, $skeleton, $utf8NoBom)
    Write-Host "config\mounts.json を作成しました。"
}

Write-Host ''
Write-Host '次の手順:'
Write-Host '  1. config\mounts.json を編集し、host_path / slug を追加してください'
Write-Host '     例: {"mount_id": "xxx", "name": "Pictures", "slug": "pictures", "host_path": "C:\\Users\\foo\\Pictures"}'
Write-Host '  2. .\start.ps1 を実行してコンテナを起動します'
