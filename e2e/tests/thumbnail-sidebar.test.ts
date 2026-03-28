// サムネイルサイドバーテスト
// P1: TS-1(Tabトグル — fixme), TS-2(クリックジャンプ)
// P2: TS-3(aria-current), TS-4(ページ送りで追従)

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

  test("TS-3: アクティブ画像に aria-current が設定される", async ({ page }) => {
    await openCgViewer(page);
    await expect(page).toHaveURL(/index=0/);

    const sidebar = page.locator("[data-testid='cg-viewer'] aside");
    await expect(sidebar).toBeVisible();

    // 1番目のサムネイルが aria-current="true"
    const firstThumb = sidebar.locator("button").first();
    await expect(firstThumb).toHaveAttribute("aria-current", "true");

    // 2番目は aria-current を持たない
    const secondThumb = sidebar.locator("button").nth(1);
    await expect(secondThumb).not.toHaveAttribute("aria-current", "true");
  });

  test("TS-4: ページ送りで aria-current が追従する", async ({ page }) => {
    await openCgViewer(page);
    await expect(page).toHaveURL(/index=0/);

    const sidebar = page.locator("[data-testid='cg-viewer'] aside");
    await expect(sidebar).toBeVisible();

    // D キーで次ページに進む
    await page.keyboard.press("d");
    await expect(page).toHaveURL(/index=1/);

    // 2番目のサムネイルに aria-current が移動する
    const secondThumb = sidebar.locator("button").nth(1);
    await expect(secondThumb).toHaveAttribute("aria-current", "true");

    // 1番目から aria-current が消える
    const firstThumb = sidebar.locator("button").first();
    await expect(firstThumb).not.toHaveAttribute("aria-current", "true");
  });
});
