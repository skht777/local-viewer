# アーキテクチャ

## ディレクトリ構造
```
local-viewer/
├── manage_mounts.py         # マウントポイント管理 TUI
├── config/
│   └── mounts.json          # マウントポイント定義 (Docker: viewer-config volume)
├── backend/
│   ├── main.py              # FastAPI エントリポイント
│   ├── routers/             # APIルーター (1リソース1ファイル)
│   └── services/            # ビジネスロジック
│       └── mount_config.py  # MountConfigService (mounts.json 読み書き)
├── frontend/
│   ├── src/
│   │   ├── pages/           # ページコンポーネント (1ルート1ファイル)
│   │   ├── components/      # UIコンポーネント
│   │   ├── hooks/           # カスタムフック
│   │   ├── stores/          # zustand ストア (UI状態のみ)
│   │   ├── lib/             # 外部ライブラリ設定 (pdfjs等)
│   │   ├── types/           # API型定義
│   │   └── utils/           # ユーティリティ関数
│   └── tests/
├── e2e/
│   ├── playwright.config.ts   # E2E テスト設定
│   ├── fixtures/              # テスト用フィクスチャ
│   └── tests/                 # Playwright テストファイル
├── Dockerfile               # マルチステージビルド
└── docker-compose.yml
```

## 依存関係ルール

### Backend
```
routers → services → 外部ライブラリ/stdlib
```
- routers は services を呼ぶ。直接ファイルシステムやDBにアクセスしない
- services は他の services に依存してよいが、routers に依存しない
- path_security は全てのファイルアクセスの前に必ず経由する（全マウントポイント root_dirs を管理し、find_root_for() で対象ルートを特定）

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
- Backend: レイヤードアーキテクチャ (Router → Service → Infrastructure)
- Frontend: Feature-based + Hooks パターン
- Docker: フロントエンドビルド → Python ランタイムが静的ファイル + API を配信
