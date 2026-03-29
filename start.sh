#!/usr/bin/env bash
# Docker コンテナ起動 (ビルド + 起動)
set -euo pipefail
docker compose up --build
