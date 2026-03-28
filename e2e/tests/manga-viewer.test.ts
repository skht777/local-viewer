// マンガモードテスト
// 仕様出典: plan-phase3.md, initial-architecture.md §マンガモード

import { test, expect } from "@playwright/test";

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

  // 画像カードが安定するまで待つ
  const firstImage = page.locator("[data-testid^='file-card-']").first();
  await expect(firstImage).toBeVisible();

  // サムネイル読み込みによるDOM再構築を待つ
  await firstImage.click({ force: true });

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

  test("Escape でビューワーを閉じる", async ({ page }) => {
    await openMangaViewer(page);
    await page.keyboard.press("Escape");
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
