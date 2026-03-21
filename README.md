# Local Content Viewer

ローカルディレクトリの画像・動画・PDFを閲覧するWebアプリケーション。

## Features

- ZIP/RAR/7z内の画像を直接表示（展開不要）
- MP4等の動画を埋め込みプレイヤーで再生
- PDFをページ単位で画像として表示
- CGモード（1枚ずつ表示）/ マンガモード（縦スクロール連続表示）
- キーボードナビゲーション
- ファイル名検索

## Tech Stack

- **Backend**: FastAPI (Python 3.13)
- **Frontend**: React + Vite + TypeScript + Tailwind CSS v4
- **Container**: Docker

## Quick Start

### Docker（推奨）

```bash
cp .env.example .env
# .env の DATA_DIR を閲覧対象ディレクトリに変更
docker compose up --build
```

http://localhost:8000 にアクセス。

### ローカル開発

```bash
# Backend
python -m venv backend/.venv
source backend/.venv/bin/activate
pip install -r backend/requirements-dev.txt

# Frontend
cd frontend && npm install && cd ..

# 起動
./start.sh
```

- Backend: http://localhost:8000
- Frontend: http://localhost:5173

## Lint & Test

```bash
# Backend
ruff check backend/ && ruff format --check backend/ && mypy backend/
source backend/.venv/bin/activate && pytest

# Frontend
npx oxlint frontend/src/ && npx oxfmt --check frontend/src/
cd frontend && npx vitest run
```

## License

MIT
