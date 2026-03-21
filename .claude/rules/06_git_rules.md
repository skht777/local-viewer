# Git 規約

## Conventional Commits
```
<type>(<scope>): <summary>

<body (optional)>
```

### Type 一覧
| Type | 用途 |
|------|------|
| `feat` | 新機能追加 |
| `fix` | バグ修正 |
| `refactor` | リファクタリング（振る舞い変更なし） |
| `chore` | ビルド・設定・依存関係の変更 |
| `test` | テストの追加・修正 |
| `docs` | ドキュメント変更 |
| `style` | フォーマット変更（コードの意味に影響しない） |
| `perf` | パフォーマンス改善 |

### Scope 例
`feat(viewer)`, `fix(archive)`, `chore(docker)`, `refactor(api)`

### 原則
- summary は50文字以内
- リファクタリングと機能追加は別コミットにする（Tidy First）
- Phase番号があれば参照する: `feat(viewer): CGモード実装 [Phase 2]`

## Pre-commit
- Husky + lint-staged が自動実行
- Frontend: oxlint + oxfmt --write
- Backend: ruff format + ruff check --fix

## ブランチ戦略
- `main` — 安定版、デプロイ可能
- `feat/<description>` — 機能開発
- `phase/<N>-<description>` — フェーズ単位の開発
