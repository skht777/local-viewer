// smoke テスト — 基本動作の確認
// 仕様出典: initial-architecture.md §画面構成

import { test, expect } from "@playwright/test";

test.describe("smoke テスト", () => {
  test("トップページが表示される", async ({ page }) => {
    await page.goto("/");
    await expect(page).toHaveTitle(/Local Content Viewer/);
  });

  test("ヘルスチェック API が応答する", async ({ request }) => {
    const response = await request.get("/api/health");
    expect(response.ok()).toBeTruthy();
    const body = await response.json();
    // status は "ok" を維持。registry_populate 等の追加フィールドは検証対象外
    expect(body).toMatchObject({ status: "ok" });
  });

  test("マウントポイントカードが表示される", async ({ page }) => {
    await page.goto("/");
    const cards = page.locator("[data-testid^='mount-']");
    await expect(cards.first()).toBeVisible();
    expect(await cards.count()).toBeGreaterThanOrEqual(1);
  });
});
