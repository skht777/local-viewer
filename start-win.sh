#!/usr/bin/env bash
# WSL2 から Docker Desktop (Windows) で起動するための薄いラッパー
#
# powershell.exe -File $(wslpath -w ./start.ps1) を exec で呼ぶだけ。
# Ctrl+C を PowerShell 側に届けるため exec 必須。
#
# 前提: WSL2 interop が有効で、powershell.exe (または pwsh.exe) が PATH 上にあること。
#       Docker Desktop + PowerShell 7+ のインストールが必要。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
START_PS1="${SCRIPT_DIR}/start.ps1"

if [[ ! -f "$START_PS1" ]]; then
    echo "エラー: start.ps1 が見つかりません: $START_PS1" >&2
    exit 1
fi

# start.ps1 は #Requires -Version 7.0 のため PowerShell 7+ (pwsh.exe) のみ受け付ける。
# Windows PowerShell 5.1 (powershell.exe) へのフォールバックはしない。
if ! command -v pwsh.exe >/dev/null 2>&1; then
    echo "エラー: pwsh.exe が見つかりません。" >&2
    echo "WSL2 interop が有効で、Windows 側に PowerShell 7+ がインストールされているか確認してください。" >&2
    echo "  インストール: https://learn.microsoft.com/powershell/scripting/install/installing-powershell-on-windows" >&2
    exit 1
fi
PS_EXE="pwsh.exe"

WIN_PATH=$(wslpath -w "$START_PS1")

exec "$PS_EXE" -NoProfile -ExecutionPolicy Bypass -File "$WIN_PATH"
