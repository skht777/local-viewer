// 履歴モデル: ブラウザバックと B キー閉じの遷移先一致を保証する E2E テスト
// - open 系を push 化したことで、ビューワー画面のブラウザバックは履歴を 1 つ pop して
//   open 直前の URL（呼び出し元）に戻る。B キー閉じは viewerOrigin 経由で同じ URL に戻る
// - 両者の遷移先が一致することがユーザー体験の核心

import { expect, test } from "@playwright/test";
import { clickFileCard, navigateToMount } from "./helpers/navigation";

test.describe("ブラウザバック == B キー閉じる の遷移先一致", () => {
  test("画像直クリック → ブラウザバックで呼び出し元に戻る", async ({ page }) => {
    await navigateToMount(page, "pictures");

    // 画像タブに切り替えて、その時点の URL を呼び出し元として記録
    const imagesTab = page.locator("[data-testid='tab-images']");
    await expect(imagesTab).toBeVisible();
    await imagesTab.click();
    await expect(page).toHaveURL(/tab=images/);
    const originalUrl = page.url();

    await test.step("画像をダブルクリックでビューワー起動", async () => {
      await clickFileCard(page.locator("[data-testid^='file-card-']").first());
      await expect(page).toHaveURL(/index=/, { timeout: 10_000 });
      await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
    });

    await test.step("ブラウザバックで呼び出し元に戻る", async () => {
      await page.goBack();
      await expect(page).not.toHaveURL(/index=/);
      await expect(page).toHaveURL(originalUrl);
    });
  });

  test("画像直クリック → B キーとブラウザバックの遷移先が一致する", async ({ page }) => {
    await navigateToMount(page, "pictures");
    const imagesTab = page.locator("[data-testid='tab-images']");
    await imagesTab.click();
    await expect(page).toHaveURL(/tab=images/);
    const originalUrl = page.url();

    // 1 回目: 画像 → B キー閉じ
    await test.step("画像 → B キー閉じ", async () => {
      await clickFileCard(page.locator("[data-testid^='file-card-']").first());
      await expect(page).toHaveURL(/index=/, { timeout: 10_000 });
      await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
      await page.keyboard.press("b");
      await expect(page).not.toHaveURL(/index=/);
    });
    const urlAfterClose = page.url();

    // 2 回目: 画像 → ブラウザバック
    await test.step("画像 → ブラウザバック", async () => {
      await clickFileCard(page.locator("[data-testid^='file-card-']").first());
      await expect(page).toHaveURL(/index=/, { timeout: 10_000 });
      await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
      await page.goBack();
      await expect(page).not.toHaveURL(/index=/);
    });
    const urlAfterBack = page.url();

    // 両者が一致し、かつ呼び出し元 URL であることを確認
    await expect(page).toHaveURL(originalUrl);
    expect(urlAfterClose).toBe(urlAfterBack);
    expect(urlAfterClose).toBe(originalUrl);
  });
});
