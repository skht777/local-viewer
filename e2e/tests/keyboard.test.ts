// キーボードナビゲーションテスト
// 仕様出典: initial-architecture.md §キーバインド

import { test, expect } from "@playwright/test";
import { clickFileCard } from "./helpers/navigation";

// pictures ディレクトリで CG モードを開くヘルパー
async function openCgInPictures(page: import("@playwright/test").Page) {
  await page.goto("/");

  // pictures マウントポイントカードをクリック
  const picturesCard = page.locator("[data-testid^='mount-']", {
    hasText: "pictures",
  });
  await expect(picturesCard).toBeVisible();
  await picturesCard.click();
  await expect(page).toHaveURL(/\/browse\//);

  // 画像タブに切り替え
  const imagesTab = page.locator("[data-testid='tab-images']");
  await expect(imagesTab).toBeVisible();
  await imagesTab.click();

  // 画像カードをクリック
  await clickFileCard(page.locator("[data-testid^='file-card-']").first());

  // CGビューワーが表示される
  await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();

  // ページカウンター (N / M) が反映されるまで待機 (keyboard ハンドラ登録と
  // 画像データ取得の完了を待ち、直後のキー送信が取りこぼされないようにする)
  await expect(page.getByTestId("page-counter")).toHaveText(/\d+\s*\/\s*\d+/);
}

test.describe("キーボード — CG モード", () => {
  test("→ キーで次ページ", async ({ page }) => {
    await openCgInPictures(page);
    await expect(page).toHaveURL(/index=0/);
    await page.keyboard.press("ArrowRight");
    await expect(page).toHaveURL(/index=1/);
  });

  test("← キーで前ページ", async ({ page }) => {
    await openCgInPictures(page);
    await page.keyboard.press("ArrowRight");
    await expect(page).toHaveURL(/index=1/);
    // 連続キー入力前に page-counter 反映まで待つ
    // (URL 更新だけでは useCgNavigation のクロージャ再バインドが間に合わず
    //  ArrowLeft が古い prevStart=null のクロージャに届くため flaky になる)
    await expect(page.getByTestId("page-counter")).toHaveText(/2\s*\/\s*\d+/);
    await page.keyboard.press("ArrowLeft");
    await expect(page).toHaveURL(/index=0/);
  });

  test("Home で先頭ページ", async ({ page }) => {
    await openCgInPictures(page);
    // 数ページ進む（URL 更新 + page-counter 反映を都度待ちながら、
    // 2 回目のキーが 1 回目の state 更新途中に重なって取りこぼされるのを防ぐ）
    await page.keyboard.press("d");
    await expect(page).toHaveURL(/index=1/);
    await expect(page.getByTestId("page-counter")).toHaveText(/2\s*\/\s*\d+/);
    await page.keyboard.press("d");
    await expect(page).toHaveURL(/index=2/);
    await page.keyboard.press("Home");
    await expect(page).toHaveURL(/index=0/);
  });

  test("End で末尾ページ", async ({ page }) => {
    await openCgInPictures(page);
    await page.keyboard.press("End");
    // pictures/ には 4 枚 (photo1-3 + large) あるので index=3
    await expect(page).toHaveURL(/index=3/);
  });

  test("B キーでビューワーを閉じる", async ({ page }) => {
    await openCgInPictures(page);
    await page.keyboard.press("b");
    await expect(page.locator("[data-testid='cg-viewer']")).not.toBeVisible();
  });

});
