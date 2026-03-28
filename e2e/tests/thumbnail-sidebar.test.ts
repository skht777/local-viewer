// サムネイルサイドバーテスト (P1)
// TS-1: Tab キートグル (未実装 → fixme)
// TS-2: サムネイルクリックジャンプ

import { test, expect } from "@playwright/test";
import { openCgViewer } from "./helpers/navigation";

test.describe("サムネイルサイドバー", () => {
  // Tab キーによるサイドバートグルが useCgKeyboard に未実装
  test.fixme("TS-1: Tab キーでサイドバーが表示/非表示になる", async ({ page }) => {
    await openCgViewer(page);

    // デフォルトでサイドバー表示
    const sidebar = page.locator("aside");
    await expect(sidebar).toBeVisible();

    // Tab キーでトグル → 非表示
    await page.keyboard.press("Tab");
    await expect(sidebar).not.toBeVisible();

    // 再度 Tab → 表示
    await page.keyboard.press("Tab");
    await expect(sidebar).toBeVisible();
  });

  test("TS-2: サムネイルクリックで対象ページにジャンプする", async ({ page }) => {
    await openCgViewer(page);
    await expect(page).toHaveURL(/index=0/);

    // サイドバーの2番目のサムネイルボタンをクリック
    const sidebar = page.locator("[data-testid='cg-viewer'] aside");
    await expect(sidebar).toBeVisible();

    const secondThumb = sidebar.locator("button").nth(1);
    await secondThumb.click();

    await expect(page).toHaveURL(/index=1/);
  });
});
