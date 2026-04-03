# CLAUDE.md -- local-viewer プロジェクト規約

ローカルディレクトリの画像・動画・PDFを閲覧する Web アプリ。
FastAPI バックエンド + React フロントエンド、Docker で配布。

## 技術スタック
- バックエンド: FastAPI + uvicorn (Python 3.14)
- フロントエンド: React + Vite + TypeScript
- スタイリング: Tailwind CSS v4 (Vite プラグイン、PostCSS 設定なし)
- 状態管理: TanStack Query (サーバー) + zustand (UI のみ)
- Lint/Format: Ruff (Python), oxlint + oxfmt (TypeScript)
- コンテナ: Docker マルチステージビルド

## 環境構築
- Python 3.14: pyenv (`pyenv install 3.14`)
- Node.js 24: nodenv (`nodenv install 24.11.1`)
- バージョンファイル (`.python-version`, `.node-version`) はリポジトリルートに配置

## Commands

```bash
# 初回セットアップ (.env コピー + マウントポイント設定)
./init.sh

# Docker コンテナ起動 (ビルド + 起動)
./start.sh

# マウントポイント管理（Bash TUI、ホスト側で実行）
./manage_mounts.sh

# ローカル開発用セットアップ
python -m venv backend/.venv && source backend/.venv/bin/activate && pip install -r backend/requirements-dev.txt
cd frontend && npm install && cd ..

# Lint（バックエンド）
source backend/.venv/bin/activate && ruff check backend/ && ruff format --check backend/ && mypy backend/

# Lint（フロントエンド）
npx oxlint frontend/src/ && npx oxfmt --check frontend/src/

# Test
source backend/.venv/bin/activate && pytest
cd frontend && npx vitest run

# E2E Test
cd e2e && npx playwright test
cd e2e && npx playwright test --ui   # UI モード
```

## 主要ファイル
- `pyproject.toml` — Ruff + mypy + pytest 設定（リポジトリルート、backend/ 配下ではない）
- `.oxlintrc.json` — oxlint 設定（react/typescript プラグイン）
- `backend/main.py` — FastAPI エントリポイント
- `backend/config.py` — 環境変数ベースの設定モジュール（MOUNT_BASE_DIR, MOUNT_CONFIG_PATH 等）
- `backend/errors.py` — 共通エラーモデル
- `backend/services/mount_config.py` — マウントポイント設定の読み書き（mounts.json v2 スキーマ: slug + host_path）
- `init.sh` — 初回セットアップ（.env コピー + manage_mounts.sh）
- `start.sh` — Docker コンテナ起動（docker compose up --build）
- `manage_mounts.sh` — マウントポイント管理 Bash TUI（ホスト側で実行、docker-compose.override.yml + mounts.json を更新）
- `docker-compose.override.yml` — manage_mounts.sh が自動生成するマウント定義（gitignored）
- `config/mounts.json` — マウントポイント定義ファイル（Docker ではバインドマウント ./config:/app/config）
- `frontend/vite.config.ts` — Vite + Tailwind v4 + /api プロキシ + Vitest
- `frontend/src/index.css` — Tailwind v4 `@theme` カスタムトークン定義
- `.env.example` — Docker ボリューム/ポート/リソース設定テンプレート
- `e2e/playwright.config.ts` — E2E テスト設定

## 注意事項
- **uvicorn はプロジェクトルートから実行** — Docker 内で自動実行。ローカル開発時は `uvicorn backend.main:app`
- **Tailwind v4** — `tailwind.config.js` や `postcss.config.js` は不要、`@tailwindcss/vite` プラグインを使用
- **lint-staged は venv パスを使用** — `package.json` 内で `backend/.venv/bin/ruff` として呼び出し
- **node_id 不透明ID** — API はクライアントに実ファイルパスを公開しない。生成時にルートパスを含めて複数マウントポイント間の衝突を回避
- **デフォルト 127.0.0.1 バインド** — LAN アクセスには `.env` で `BIND_HOST=0.0.0.0` を明示指定

## 実装時に特に気を付けたいこと

### TDD
- Red → Green → Refactor を小刻みに回す
- 失敗するテストを先に書いてから最小限の実装
- テスト名は日本語で振る舞いを記述

### Git Workflow
- 1つの論理的な変更単位ごとにコミットする（テストが通る状態で）
- リファクタリングと機能追加は別コミット（Tidy First）
- 作業完了時にコミットを忘れずに行う

### ドキュメント
- 仕様書: `docs/spec-*.md`（アーキテクチャ、UI、パフォーマンス）
- 実装計画: `docs/plan-*.md` に保存（gitignored、ローカル保全のみ）
- 完了済み計画: `docs/archive/` に移動済み
