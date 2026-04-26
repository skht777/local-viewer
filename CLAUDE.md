# CLAUDE.md -- local-viewer プロジェクト規約

ローカルディレクトリの画像・動画・PDFを閲覧する Web アプリ。
Rust バックエンド + React フロントエンド、Docker で配布。

## 技術スタック
- バックエンド: axum + tokio (Rust), rusqlite bundled (FTS5), image (サムネイル), moka (キャッシュ)
- フロントエンド: React + Vite + TypeScript
- スタイリング: Tailwind CSS v4 (Vite プラグイン、PostCSS 設定なし)
- 状態管理: TanStack Query (サーバー) + zustand (UI のみ)
- PWA: vite-plugin-pwa (サムネイル CacheFirst, API NetworkFirst)
- Lint/Format: clippy + rustfmt (Rust), oxlint + oxfmt (TypeScript)
- コンテナ: Docker マルチステージビルド

## 環境構築
- Rust: rustup (rust-toolchain.toml で自動バージョン管理)
- Node.js 24: nodenv (`nodenv install 24.11.1`)
- バージョンファイル (`.node-version`, `backend/rust-toolchain.toml`) はリポジトリに配置

## Commands

```bash
# 初回セットアップ (.env コピー + マウントポイント設定)
./init.sh              # Linux/macOS
.\init.ps1             # Windows PowerShell

# Docker コンテナ起動 (ビルド + 起動)
./start.sh             # Linux/macOS
.\start.ps1            # Windows PowerShell
./start-win.sh         # WSL2 経由

# マウントポイント管理（Bash TUI、ホスト側で実行）
./manage_mounts.sh

# ローカル開発用セットアップ (Rust バックエンド)
cd backend && cargo build

# ローカル開発用セットアップ (フロントエンド)
cd frontend && npm install

# 型チェック（Rust、編集ループ中はこちらが高速）
cd backend && cargo check

# Lint（Rust バックエンド）
cd backend && cargo clippy --all-targets --all-features -- -D warnings && cargo fmt --check

# Lint（フロントエンド、tests と vite.config.ts も対象）
npx oxlint frontend/ && npx oxfmt --check --ignore-path .gitignore frontend/

# Test（Rust バックエンド）
cd backend && cargo test

# Test（フロントエンド）
cd frontend && npx vitest run

# E2E Test
cd e2e && npx playwright test
cd e2e && npx playwright test --ui   # UI モード
```

## 主要ファイル

### Rust バックエンド
- `backend/Cargo.toml` — 依存クレート定義
- `backend/src/main.rs` — エントリポイント（CLI パース + `axum::serve` のみ。組み立ては `bootstrap` に委譲）
- `backend/src/bootstrap/` — 起動時組み立て（`state` で `AppState` 構築、`api_router` で `/api/health` + `/api/ready` 分離ルート登録、`http_layers`/`static_files`/`background_tasks`）
- `backend/src/config.rs` — 環境変数ベースの設定
- `backend/src/routers/` — API ルーター（browse/, file/, thumbnail/ はサブモジュール分割済み。browse は `/sibling`（単方向）と `/siblings`（双方向 combined）を提供）
- `backend/src/services/` — ビジネスロジック（node_registry/, archive/, dir_index/, indexer/ はサブモジュール分割済み）
- `backend/src/services/dir_index/dirty_state.rs` — FileWatcher 連動の dirty セット + 世代カウンタ（TOCTOU 対策）
- `backend/src/middleware/` — カスタムミドルウェア（skip_gzip_binary）
- `backend/rust-toolchain.toml` — Rust ツールチェーン固定
- `backend/clippy.toml` — Clippy 設定
- `backend/rustfmt.toml` — rustfmt 設定

### 共通
- `init.sh` / `init.ps1` — 初回セットアップ（.env コピー + manage_mounts.sh）
- `start.sh` / `start.ps1` / `start-win.sh` — Docker コンテナ起動（docker compose up --build）
- `manage_mounts.sh` — マウントポイント管理 Bash TUI（ホスト側で実行、docker-compose.override.yml + mounts.json を更新）
- `scripts/` — マウントパス変換・テストスクリプト（Windows/WSL2 対応）
- `docker-compose.override.yml` — manage_mounts.sh が自動生成するマウント定義（gitignored）
- `config/mounts.json` — マウントポイント定義ファイル（Docker ではバインドマウント ./config:/app/config）
- `frontend/vite.config.ts` — Vite + Tailwind v4 + /api プロキシ + Vitest
- `frontend/src/index.css` — Tailwind v4 `@theme` カスタムトークン定義
- `frontend/src/hooks/api/browseQueries.ts` — TanStack Query（browseNodeOptions, browseInfiniteOptions, searchOptions。`scope` 引数で配下検索）
- `frontend/src/hooks/api/thumbnailQueries.ts` — バッチサムネイルフック（useBatchThumbnails）
- `frontend/src/hooks/useOpenViewerFromEntry.ts` — ▶開く/Space 経路のビューワー起動（起点復帰 + トランジション + prefetch + push）
- `frontend/src/stores/viewerStore.ts` — `viewerOrigin` / `viewerTransitionId` は `partialize` で persist 除外
- `.env.example` — Docker ボリューム/ポート/リソース設定テンプレート
- `e2e/playwright.config.ts` — E2E テスト設定

## 注意事項
- **Rust バイナリはプロジェクトルートから実行** — `MOUNT_BASE_DIR` 等の環境変数で設定。CLI: `./backend/target/release/local-viewer-backend --port 8000`
- **Tailwind v4** — `tailwind.config.js` や `postcss.config.js` は不要、`@tailwindcss/vite` プラグインを使用
- **node_id 不透明ID** — API はクライアントに実ファイルパスを公開しない。生成時にルートパスを含めて複数マウントポイント間の衝突を回避
- **デフォルト 127.0.0.1 バインド** — LAN アクセスには `.env` で `BIND_HOST=0.0.0.0` を明示指定
- **ビューワー履歴モデル** — viewer 起動経路（`useOpenViewerFromEntry` / `useViewerParams.openViewer` / `useViewerParams.openPdfViewer` / `SearchBar` の PDF）はすべて push。close は現状維持（`viewerOrigin` あれば origin に navigate replace、無ければ `setSearchParams(buildCloseImageSearch)` で search 削除）。セットジャンプは `{ replace: true }` 維持（履歴汚染を避ける）。ブラウズ間 navigate（ツリー/パンくず/カード/上へ）は push のまま、`navigateBrowse` で同一 nodeId への遷移を早期 return で抑制。これによりブラウザバックと B キー閉じが同じ呼び出し元 URL に戻ることを保証
- **ビューワー閉じキー** — `B`（Esc はヘルプ/NavigationPrompt/フルスクリーン解除のみ）
- **ビューワー画像表示順** — 常に名前昇順固定（ブラウズソート順と独立、`compareEntryName` で統一）。セット間ジャンプの兄弟探索はブラウズソート順を維持
- **スコープ検索** — BrowsePage の SearchBar はスコープトグルあり（ON: 配下検索、OFF: 全体）。TopPage はトグル非表示。バックエンドは `NodeRegistry::resolve` + `PathSecurity::validate_existing` + `is_dir` 検証必須
- **DirIndex 自己修復** — FileWatcher が影響ディレクトリを dirty 化（世代カウンタ）→ fast-path は dirty チェックで fallback 行き → fallback 後に `bulk_upsert_from_scan` で write-back + 世代一致時のみ dirty 解除
- **readiness プローブ** — `/api/ready`（初回スキャン完了判定、503/200）と `/api/health`（liveness、常時 200）を分離。docker-compose healthcheck は `/api/ready` を使用
- **infinite query プリフェッチ** — `fetchQuery` 不可。`prefetchInfiniteQuery` / `fetchInfiniteQuery` を使用しキャッシュキーを `browseInfiniteOptions` に統一

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
