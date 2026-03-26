// エラー系テスト
// 仕様出典: initial-architecture.md §エラー契約, 09_security.md

import { test, expect } from "@playwright/test";

test.describe("エラー", () => {
  test("存在しない node_id でアクセスすると 404 になる", async ({
    request,
  }) => {
    const response = await request.get("/api/browse/nonexistent-node-id");
    expect(response.status()).toBe(404);
  });

  test("存在しない browse URL でエラーメッセージが表示される", async ({
    page,
  }) => {
    await page.goto("/browse/invalid-node-id-12345");
    // エラーメッセージが画面に表示される（or リダイレクトされる）
    // 具体的なメッセージは実装依存だが、空白ページにならないことを確認
    const body = page.locator("body");
    await expect(body).not.toBeEmpty();
  });
});
