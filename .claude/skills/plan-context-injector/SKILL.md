---
name: plan-context-injector
description: >
  プラン策定中に関連するプロジェクトルールを分析し、プランの Context セクションに
  制約条件として注入する。「ルールを注入して」「コンテキスト注入」等で発動。
allowed-tools: Read, Grep, Glob
---

# plan-context-injector

プラン策定中のプランファイルに、プロジェクトの重要なルールを選択的に注入するスキル。

## 動作原則

### 1. 共通注意事項の注入（常時）

CLAUDE.md の「実装時に特に気を付けたいこと」セクションを読み、その内容を必ず注入する。
これにはTDD、Git Workflow、計画書の保管などプロジェクト共通の注意事項が含まれる。

### 2. 変更スコープに応じたルール注入

プランの変更対象（Files セクション等）を分析し、関連するルールファイルを `.claude/rules/` から読む。
各ルールのキーポイントを **3行以内** で要約して注入する。

選定判断の例:
- backend 変更 → `backend-rust.md` の該当部分
- frontend 変更 → `frontend-react.md` の該当部分
- ファイルアクセス・ユーザー入力 → `09_security.md`（パストラバーサル防止等）
- テスト追加を伴う → `07_testing.md`
- UI 変更 → `05_design_settings.md`
- E2E テスト → `10_e2e_testing.md`

常に参照:
- `02_architecture.md` → 依存方向ルール
- `04_coding_principles.md` → 命名規則・関数設計

具体的な選定判断はタスクの内容に応じて柔軟に行う。

### 3. 注入形式

プランの Context セクション末尾に以下のサブセクションを追加する:

```markdown
### 適用ルール

**共通注意事項（CLAUDE.md）:**
- TDD: Red → Green → Refactor を小刻みに回す
- Git: 1つの論理的変更単位ごとにコミット、Tidy First
- 計画書: docs/plan-*.md に保存

**関連ルール:**
- 02_architecture: Backend は Router → Service → Infrastructure。Frontend は pages → components → hooks → stores
- 09_security: 全ファイルアクセスは path_security.py 経由、resolve() + ROOT_DIR 検証必須
- ...（変更スコープに応じて追加）
```

### 4. コンテキストウィンドウ効率

- ルールファイルの全文をプランに貼り付けない
- 「該当する制約」のみを3行以内の要約で注入する
- セキュリティルールのみ、影響がある場合にやや詳細に記載する

## 対象プランファイル

- ユーザーが指定したファイル、または直近で作成/編集した `docs/plan-*.md` を対象とする
- プランモード中の `~/.claude/plans/` 内のプランファイルも対象
