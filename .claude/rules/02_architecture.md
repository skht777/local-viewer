# アーキテクチャ

## ディレクトリ構造
```
local-viewer/
├── init.sh                  # 初回セットアップ
├── start.sh                 # Docker コンテナ起動
├── manage_mounts.sh         # マウントポイント管理 Bash TUI (ホスト側)
├── config/
│   └── mounts.json          # マウントポイント定義 (Docker: バインドマウント ./config:/app/config)
├── backend/
│   ├── main.py              # FastAPI エントリポイント (DI 登録: DirIndex, ThumbnailWarmer 等)
│   ├── routers/             # APIルーター (1リソース1ファイル)
│   └── services/            # ビジネスロジック
│       ├── mount_config.py  # MountConfigService (mounts.json 読み書き)
│       ├── dir_index.py     # DirIndex (SQLite ディレクトリリスティング専用インデックス)
│       ├── browse_cursor.py # カーソルベースページネーション (HMAC 署名)
│       └── thumbnail_warmer.py # サムネイルプリウォーム (バックグラウンド生成)
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
├── Dockerfile               # マルチステージビルド
├── docker-compose.yml           # 静的設定 (git tracked)
└── docker-compose.override.yml  # マウント定義 (manage_mounts.sh 自動生成, gitignored)
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
