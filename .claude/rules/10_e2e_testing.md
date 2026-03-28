# E2E テスト規約 (Playwright)

## 概要

`e2e/` ディレクトリで Playwright + TypeScript による E2E テストを管理する。
認証不要の個人ビューワーのため、storageState パターンは使用しない。

## ディレクトリ構成

```
e2e/
├── playwright.config.ts       # E2E専用設定（ポート8001/5174）
├── package.json               # E2E専用依存
├── tsconfig.json
├── fixtures/
│   ├── test-data/             # テスト用ファイル（画像・動画・PDF・ZIP）
│   └── generate-fixtures.ts   # フィクスチャ生成スクリプト
├── tests/
│   ├── pages/                 # Page Object Model（必要に応じて）
│   ├── helpers/               # 共通ヘルパー
│   └── *.test.ts              # テストファイル
└── test-results/              # (gitignored)
```

## コマンド

E2E テストの実行・管理には **Playwright CLI** (`npx playwright`) を使用する。
全コマンドは `e2e/` ディレクトリで実行すること。

```bash
# 全テスト実行
cd e2e && npx playwright test

# ファイル指定 / grep指定
cd e2e && npx playwright test tests/smoke.test.ts
cd e2e && npx playwright test -g "CGモード"

# デバッグモード（ブラウザ表示 + ステップ実行）
cd e2e && npx playwright test --debug

# UI モード（テスト選択・再実行・トレース表示）
cd e2e && npx playwright test --ui

# テスト一覧確認（実行せずにテスト名を表示）
cd e2e && npx playwright test --list

# HTMLレポート表示
cd e2e && npx playwright show-report

# トレースファイルの表示
cd e2e && npx playwright show-trace test-results/.../trace.zip

# ブラウザインストール（初回セットアップ / CI）
cd e2e && npx playwright install --with-deps chromium

# Codegen（操作記録 → コード生成）
cd e2e && npx playwright codegen http://localhost:5174
```

### CLI の使い分け

| 目的 | コマンド |
|------|----------|
| 通常実行 | `npx playwright test` |
| 単一テスト | `npx playwright test tests/smoke.test.ts` |
| パターン指定 | `npx playwright test -g "キーボード"` |
| 失敗テストのみ再実行 | `npx playwright test --last-failed` |
| リトライ付き実行 | `npx playwright test --retries=2` |
| ワーカー数指定 | `npx playwright test --workers=1` |
| トレース付き実行 | `npx playwright test --trace on` |
| デバッグ | `npx playwright test --debug` |
| 対話的UI | `npx playwright test --ui` |

## テストコード規約

### Locator の優先順位（厳守）

1. `data-testid` → `page.getByTestId('submit-button')` / `page.locator("[data-testid^='mount-']")`
2. ARIA Role → `page.getByRole('button', { name: '送信' })`
3. Label → `page.getByLabel('...')`
4. Text → `page.getByText('...')`
5. CSS/XPath → **最終手段**（使用時は理由をコメントに記載）

`#id` や `.class` による CSS セレクタは DOM 構造変更に脆弱なため避ける。

### 待機処理

- `page.waitForTimeout()` は **絶対に使用禁止**
- Playwright の auto-waiting を信頼する
- 明示的な待機が必要な場合:
  - `page.waitForSelector(selector, { state: 'visible' })`
  - `page.waitForResponse(url => url.includes('/api/'))`
  - `page.waitForLoadState('domcontentloaded')`
  - `expect(locator).toBeVisible({ timeout: 10_000 })`

### アサーション

- `expect(element).toBeVisible()` だけで終わらせない
- ビジネスロジックの正確性を必ず検証する:
  - URL 遷移: `await expect(page).toHaveURL(/mode=cg/)`
  - ページカウンター: `await expect(counter).toHaveText(/1\s*\/\s*\d+/)`
  - リスト件数: `expect(await cards.count()).toBeGreaterThanOrEqual(1)`
  - 状態遷移: 操作前後の状態を両方確認
- `expect.soft()` は非クリティカルな検証にのみ使用

### テスト構造

```typescript
// テストファイル: e2e/tests/{feature}.test.ts
import { test, expect } from "@playwright/test";

test.describe("CGモード", () => {
  test("画像クリックで CG ビューワーが開き URL が更新される", async ({ page }) => {
    // ステップが3つ以上ある場合は test.step() で分割
    await test.step("pictures マウントポイントに移動", async () => {
      // ...
    });
    await test.step("画像をクリックしてビューワーを開く", async () => {
      // ...
    });
    await test.step("URL パラメータを検証", async () => {
      // ...
    });
  });
});
```

### テスト名・記述

- テスト名は日本語で、検証内容を具体的に記述する
  - OK: `'D キーで次ページに進める'`
  - NG: `'キーボードテスト'`
- `test.describe()` で機能をグルーピングする
- 1テストファイル 300行以内を目安

### ヘルパー関数

- 複数テストで共通する操作（ビューワーを開く等）はヘルパー関数に抽出する
- ヘルパーはテストファイル上部、または `e2e/tests/helpers/` に配置

## テスト独立性

- 各テストは他のテストの結果に依存してはならない
- `fullyParallel` は現在無効（順序依存がないことを確認後に有効化を検討）
- テストデータは `e2e/fixtures/test-data/` の固定フィクスチャを使用
- グローバル変数でのテスト間データ受け渡し禁止

## 禁止事項

- `page.waitForTimeout()` の使用
- ハードコードされた URL（`baseURL` 設定を使用）
- テスト間の順序依存
- `test.only()` のコミット
- `console.log` の残存
- CSS セレクタの多用（`data-testid` / Role を優先）
- `force: true` による要素操作（根本原因を修正すること）
- `page.evaluate()` による直接 DOM 操作
