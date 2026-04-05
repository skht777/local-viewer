# CLAUDE.md -- local-viewer プロジェクト規約

ローカルディレクトリの画像・動画・PDFを閲覧する Web アプリ。
Rust バックエンド (移行中) + React フロントエンド、Docker で配布。

## 技術スタック
- バックエンド (Rust — 移行中): axum + tokio, rusqlite (FTS5), image (サムネイル)
- バックエンド (Python — レガシー): FastAPI + uvicorn (Python 3.14)
- フロントエンド: React + Vite + TypeScript
- スタイリング: Tailwind CSS v4 (Vite プラグイン、PostCSS 設定なし)
- 状態管理: TanStack Query (サーバー) + zustand (UI のみ)
- Lint/Format: clippy + rustfmt (Rust), Ruff (Python), oxlint + oxfmt (TypeScript)
- コンテナ: Docker マルチステージビルド

## 環境構築
- Rust: rustup (rust-toolchain.toml で自動バージョン管理)
- Python 3.14: pyenv (`pyenv install 3.14`) — レガシーバックエンド用
- Node.js 24: nodenv (`nodenv install 24.11.1`)
- バージョンファイル (`.python-version`, `.node-version`, `backend/rust-toolchain.toml`) はリポジトリに配置

## Commands

```bash
# 初回セットアップ (.env コピー + マウントポイント設定)
./init.sh

# Docker コンテナ起動 (ビルド + 起動)
./start.sh

# マウントポイント管理（Bash TUI、ホスト側で実行）
./manage_mounts.sh

# ローカル開発用セットアップ (Rust バックエンド)
cd backend && cargo build

# ローカル開発用セットアップ (Python レガシー)
python -m venv py_backend/.venv && source py_backend/.venv/bin/activate && pip install -r py_backend/requirements-dev.txt

# ローカル開発用セットアップ (フロントエンド)
cd frontend && npm install

# 型チェック（Rust、編集ループ中はこちらが高速）
cd backend && cargo check

# Lint（Rust バックエンド）
cd backend && cargo clippy --all-targets --all-features -- -D warnings && cargo fmt --check

# Lint（Python レガシー）
source py_backend/.venv/bin/activate && ruff check py_backend/ && ruff format --check py_backend/ && mypy py_backend/

# Lint（フロントエンド）
npx oxlint frontend/src/ && npx oxfmt --check frontend/src/

# Test（Rust バックエンド）
cd backend && cargo test

# Test（Python レガシー）
source py_backend/.venv/bin/activate && pytest

# Test（フロントエンド）
cd frontend && npx vitest run

# E2E Test
cd e2e && npx playwright test
cd e2e && npx playwright test --ui   # UI モード
```

## 主要ファイル

### Rust バックエンド (移行中)
- `backend/Cargo.toml` — 依存クレート定義
- `backend/src/main.rs` — エントリポイント（AppState 初期化、ルーター登録）
- `backend/src/config.rs` — 環境変数ベースの設定（Python 版 config.py と同一変数名）
- `backend/src/routers/` — API ルーター（browse, file, mounts, search, thumbnail）
- `backend/src/services/` — ビジネスロジック（node_registry, path_security, archive, indexer 等）
- `backend/rust-toolchain.toml` — Rust ツールチェーン固定
- `backend/clippy.toml` — Clippy 設定
- `backend/rustfmt.toml` — rustfmt 設定

### Python バックエンド (レガシー)
- `pyproject.toml` — Ruff + mypy + pytest 設定（リポジトリルート、py_backend/ 配下ではない）
- `py_backend/main.py` — FastAPI エントリポイント（DI 登録: DirIndex, ThumbnailWarmer 等）
- `py_backend/config.py` — 環境変数ベースの設定モジュール（MOUNT_BASE_DIR, MOUNT_CONFIG_PATH 等）
- `py_backend/errors.py` — 共通エラーモデル + 独自例外 (NotADirectoryApiError, InvalidArchiveError, InvalidCursorError 等) + ハンドラ
- `py_backend/services/mount_config.py` — マウントポイント設定の読み書き（mounts.json v2 スキーマ: slug + host_path）
- `py_backend/services/dir_index.py` — DirIndex サービス（SQLite ディレクトリリスティング専用インデックス、browse 高速化）
- `py_backend/services/browse_cursor.py` — カーソルベースページネーション（HMAC 署名、SortOrder、keyset ページング）
- `py_backend/services/thumbnail_warmer.py` — サムネイルプリウォーム（バックグラウンド生成、重複排除）

### 共通
- `init.sh` — 初回セットアップ（.env コピー + manage_mounts.sh）
- `start.sh` — Docker コンテナ起動（docker compose up --build）
- `manage_mounts.sh` — マウントポイント管理 Bash TUI（ホスト側で実行、docker-compose.override.yml + mounts.json を更新）
- `docker-compose.override.yml` — manage_mounts.sh が自動生成するマウント定義（gitignored）
- `config/mounts.json` — マウントポイント定義ファイル（Docker ではバインドマウント ./config:/app/config）
- `frontend/vite.config.ts` — Vite + Tailwind v4 + /api プロキシ + Vitest
- `frontend/src/index.css` — Tailwind v4 `@theme` カスタムトークン定義
- `frontend/src/hooks/api/browseQueries.ts` — TanStack Query（browseNodeOptions, browseInfiniteOptions, searchOptions）
- `frontend/src/hooks/api/thumbnailQueries.ts` — バッチサムネイルフック（useBatchThumbnails）
- `.env.example` — Docker ボリューム/ポート/リソース設定テンプレート
- `e2e/playwright.config.ts` — E2E テスト設定

## 注意事項
- **Rust バックエンド移行中** — `backend/` が新バックエンド、`py_backend/` は参照用レガシー。E2E テスト全通過をもって切替完了
- **Rust バイナリはプロジェクトルートから実行** — `MOUNT_BASE_DIR` 等の環境変数で設定。CLI: `./backend/target/release/local-viewer-backend --port 8000`
- **uvicorn はプロジェクトルートから実行** — Docker 内で自動実行。ローカル開発時は `uvicorn py_backend.main:app` (レガシー)
- **Tailwind v4** — `tailwind.config.js` や `postcss.config.js` は不要、`@tailwindcss/vite` プラグインを使用
- **lint-staged は venv パスを使用** — `package.json` 内で `py_backend/.venv/bin/ruff` として呼び出し
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
