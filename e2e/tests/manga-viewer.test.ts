// マンガモードテスト
// 仕様出典: plan-phase3.md, initial-architecture.md §マンガモード

import { test, expect } from "@playwright/test";
import { clickFileCard } from "./helpers/navigation";

// pictures ディレクトリでマンガモードを開くヘルパー
async function openMangaViewer(page: import("@playwright/test").Page) {
  await page.goto("/");

  // pictures マウントポイントカードをクリック
  const picturesCard = page.locator("[data-testid^='mount-']", {
    hasText: "pictures",
  });
  await expect(picturesCard).toBeVisible();
  await picturesCard.click();
  await expect(page).toHaveURL(/\/browse\//);

  // ツールバーでマンガモードを選択
  await page.getByTestId("mode-toggle-manga").click();
  await expect(page).toHaveURL(/mode=manga/);

  // 画像タブに切り替え
  const imagesTab = page.locator("[data-testid='tab-images']");
  await expect(imagesTab).toBeVisible();
  await imagesTab.click();

  // 画像カードをクリック
  await clickFileCard(page.locator("[data-testid^='file-card-']").first());

  // マンガビューワーが表示される
  await expect(page.locator("[data-testid='manga-viewer']")).toBeVisible();
}

test.describe("マンガモード", () => {
  test("マンガモードで縦スクロール表示される", async ({ page }) => {
    await openMangaViewer(page);
    // マンガビューワー内に画像が存在する
    const viewer = page.locator("[data-testid='manga-viewer']");
    const images = viewer.locator("img");
    await expect(images.first()).toBeVisible();
  });

  test("B キーでビューワーを閉じる", async ({ page }) => {
    await openMangaViewer(page);
    await page.keyboard.press("b");
    await expect(
      page.locator("[data-testid='manga-viewer']"),
    ).not.toBeVisible();
  });

  test("ページカウンターが表示される", async ({ page }) => {
    await openMangaViewer(page);
    const counter = page.locator("[data-testid='page-counter']");
    await expect(counter).toBeVisible();
  });
});
