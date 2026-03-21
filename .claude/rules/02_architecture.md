# アーキテクチャ

## ディレクトリ構造
```
local-viewer/
├── backend/
│   ├── main.py              # FastAPI エントリポイント
│   ├── routers/             # APIルーター (1リソース1ファイル)
│   └── services/            # ビジネスロジック
├── frontend/
│   ├── src/
│   │   ├── pages/           # ページコンポーネント (1ルート1ファイル)
│   │   ├── components/      # UIコンポーネント
│   │   ├── hooks/           # カスタムフック
│   │   └── stores/          # zustand ストア (UI状態のみ)
│   └── tests/
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
- path_security は全てのファイルアクセスの前に必ず経由する

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
