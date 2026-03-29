# 技術スタック

## バックエンド
- Python 3.14
- FastAPI + uvicorn
- Pillow (画像処理)
- zipfile (標準), rarfile + unrar (RAR), py7zr (7z)
- SQLite FTS5 (検索インデックス)
- watchdog (ファイル監視)

## フロントエンド
- React 19 + TypeScript
- Vite (ビルド + 開発サーバー)
- Tailwind CSS v4 (@tailwindcss/vite プラグイン)
- TanStack Query (サーバー状態管理)
- zustand (UI状態管理)
- react-hotkeys-hook (キーボードショートカット)
- @tanstack/react-virtual (仮想スクロール)
- pdfjs-dist (PDF描画)
- react-router-dom (ルーティング)

## 開発ツール
- Ruff (Python lint + format)
- mypy (Python 型チェック)
- oxlint (TypeScript lint)
- oxfmt (TypeScript format)
- Vitest + Testing Library (フロントエンドテスト)
- pytest + httpx (バックエンドテスト)
- Husky + lint-staged (pre-commit)

## コンテナ
- Docker (マルチステージビルド)
- docker-compose

## 禁止事項
- 上記以外のUIフレームワーク（Angular, Vue等）を提案しない
- ESLint, Prettier, Biome を提案しない（oxlint + oxfmt を使用）
- Black, isort を提案しない（Ruff を使用）
- react-pdf を提案しない（pdfjs-dist を直接使用）
- CSS Modules, styled-components, Emotion を提案しない（Tailwind のみ）
- Redux, MobX, Recoil を提案しない（zustand + TanStack Query）
