// CG 画像操作テスト (P1)
// CI-1: 右半分クリック次ページ、CI-2: 左半分クリック前ページ

import { test, expect } from "@playwright/test";
import { openCgViewer } from "./helpers/navigation";

test.describe("CG 画像クリックナビゲーション", () => {
  test("CI-1: 画像右半分クリックで次ページに進む", async ({ page }) => {
    await openCgViewer(page);
    await expect(page).toHaveURL(/index=0/);

    // cg-image-area の右半分をクリック
    const imageArea = page.getByTestId("cg-image-area");
    const box = await imageArea.boundingBox();
    if (!box) throw new Error("cg-image-area が見つかりません");

    await page.mouse.click(box.x + box.width * 0.75, box.y + box.height / 2);
    await expect(page).toHaveURL(/index=1/);
  });

  test("CI-2: 画像左半分クリックで前ページに戻る", async ({ page }) => {
    await openCgViewer(page);

    // まず D キーで index=1 に進める
    await page.keyboard.press("d");
    await expect(page).toHaveURL(/index=1/);

    // cg-image-area の左半分をクリック
    const imageArea = page.getByTestId("cg-image-area");
    const box = await imageArea.boundingBox();
    if (!box) throw new Error("cg-image-area が見つかりません");

    await page.mouse.click(box.x + box.width * 0.25, box.y + box.height / 2);
    await expect(page).toHaveURL(/index=0/);
  });
});
