# アーキテクチャ

## ディレクトリ構造
```
local-viewer/
├── init.sh                  # 初回セットアップ
├── start.sh                 # Docker コンテナ起動
├── manage_mounts.sh         # マウントポイント管理 Bash TUI (ホスト側)
├── config/
│   └── mounts.json          # マウントポイント定義 (Docker: バインドマウント ./config:/app/config)
├── backend/                 # Rust バックエンド
│   ├── Cargo.toml           # 依存クレート定義
│   ├── rust-toolchain.toml  # Rust ツールチェーン固定
│   ├── clippy.toml          # Clippy 設定
│   ├── rustfmt.toml         # rustfmt 設定
│   ├── src/
│   │   ├── main.rs          # エントリポイント (AppState 初期化、ルーター登録)
│   │   ├── config.rs        # 環境変数ベースの設定
│   │   ├── errors.rs        # 共通エラー型 (IntoResponse)
│   │   ├── state.rs         # AppState (DI コンテナ相当)
│   │   ├── routers/         # API ルーター (1リソース1ファイル)
│   │   ├── services/        # ビジネスロジック
│   │   └── middleware/      # カスタムミドルウェア
│   └── tests/               # 統合テスト + fixtures
├── frontend/
│   ├── src/
│   │   ├── pages/           # ページコンポーネント (1ルート1ファイル)
│   │   ├── components/      # UIコンポーネント
│   │   ├── hooks/           # カスタムフック (api/ 配下に TanStack Query オプション)
│   │   ├── stores/          # zustand ストア (UI状態のみ)
│   │   ├── lib/             # 外部ライブラリ設定 (pdfjs等)
│   │   ├── types/           # API型定義
│   │   └── utils/           # ユーティリティ関数
│   └── tests/
├── e2e/
│   ├── playwright.config.ts   # E2E テスト設定
│   ├── fixtures/              # テスト用フィクスチャ
│   └── tests/                 # Playwright テストファイル
├── Dockerfile               # マルチステージビルド (3段構成: Node → Rust → debian-slim)
├── docker-compose.yml           # 静的設定 (git tracked)
└── docker-compose.override.yml  # マウント定義 (manage_mounts.sh 自動生成, gitignored)
```

## 依存関係ルール

### Backend (Rust)
```
routers → services → 外部クレート/std
```
- レイヤード依存方向を維持
- 状態管理: `AppState` 構造体 + `Arc<T>`
- CPU バウンド処理: `tokio::task::spawn_blocking`
- SQLite 操作: `spawn_blocking` 内で同期 rusqlite

### Frontend
```
pages → components, hooks
components → hooks (UIロジック分離時のみ)
hooks → stores, TanStack Query
stores → (外部依存なし、純粋なUI状態)
```
- pages は components と hooks を組み合わせる
- components 間の直接依存は避ける（共通化は hooks または親 page で行う）
- hooks 間の相互依存は禁止（単方向のみ）
- stores は TanStack Query のデータを複製しない

## アーキテクチャパターン
- Backend: レイヤードアーキテクチャ (Router → Service → Infrastructure)、axum + tower ミドルウェア
- Frontend: Feature-based + Hooks パターン
- Docker: フロントエンドビルド → Rust ビルド → debian-slim ランタイム
