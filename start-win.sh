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

# PowerShell 7+ (pwsh.exe) を優先し、無ければ Windows PowerShell (powershell.exe) にフォールバック
# ただし start.ps1 は #Requires -Version 7.0 のため、5.1 だと実行時エラーになる。
if command -v pwsh.exe >/dev/null 2>&1; then
    PS_EXE="pwsh.exe"
elif command -v powershell.exe >/dev/null 2>&1; then
    PS_EXE="powershell.exe"
else
    echo "エラー: pwsh.exe または powershell.exe が見つかりません。" >&2
    echo "WSL2 interop が有効で、Docker Desktop + PowerShell 7+ がインストールされているか確認してください。" >&2
    exit 1
fi

WIN_PATH=$(wslpath -w "$START_PS1")

exec "$PS_EXE" -NoProfile -ExecutionPolicy Bypass -File "$WIN_PATH"
