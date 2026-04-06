# Local Content Viewer

ローカルディレクトリの画像・動画・PDFを閲覧するWebアプリケーション。

## Features

- ZIP/RAR/7z内の画像を直接表示（展開不要）
- MP4等の動画を埋め込みプレイヤーで再生
- PDFをページ単位で画像として表示
- CGモード（1枚ずつ表示）/ マンガモード（縦スクロール連続表示）
- 見開き表示（通常 / 表紙単独）
- キーボードナビゲーション（WASD + 矢印キー）
- ファイル名キーワード検索（SQLite FTS5）
- MKV動画の自動MP4変換再生
- マルチマウントポイント管理（Bash TUI）

## Tech Stack

- **Backend**: axum (Rust)
- **Frontend**: React + Vite + TypeScript + Tailwind CSS v4
- **Container**: Docker

## Quick Start

### Docker（推奨）

```bash
# 初回セットアップ（.env コピー + マウントポイント設定）
./init.sh

# コンテナ起動
./start.sh
```

http://localhost:8000 にアクセス。

マウントポイントの追加・削除は `./manage_mounts.sh` で管理。

### ローカル開発

```bash
# Backend
cd backend && cargo build

# Frontend
cd frontend && npm install

# 起動（バックエンド）
cd backend && cargo run -- --port 8000

# 起動（フロントエンド）
cd frontend && npm run dev
```

- Backend: http://localhost:8000
- Frontend: http://localhost:5173（API は /api でバックエンドにプロキシ）

## Lint & Test

```bash
# Backend
cd backend && cargo clippy --all-targets --all-features -- -D warnings
cd backend && cargo fmt --check
cd backend && cargo test

# Frontend
npx oxlint frontend/src/ && npx oxfmt --check frontend/src/
cd frontend && npx vitest run
```

## E2E Test

```bash
cd e2e && npx playwright test
cd e2e && npx playwright test --ui   # UI モード
```

## License

MIT
