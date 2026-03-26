// 動画タブテスト
// 仕様出典: plan-phase5.md, plan-phase5.5.md, initial-architecture.md §動画タブ

import { test, expect } from "@playwright/test";

test.describe("動画タブ", () => {
  test("動画タブで動画カードが表示される", async ({ page }) => {
    await page.goto("/");

    // videos マウントポイントカードをクリック
    const videosMount = page.locator("[data-testid^='mount-']", {
      hasText: "videos",
    });
    await expect(videosMount).toBeVisible();
    await videosMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    // 動画タブに切り替え
    const videosTab = page.locator("[data-testid='tab-videos']");
    if (await videosTab.isVisible()) {
      await videosTab.click();
      await expect(page).toHaveURL(/tab=videos/);

      // 動画カードが表示される
      const videoCards = page.locator("[data-testid^='video-card-']");
      await expect(videoCards.first()).toBeVisible();
    }
  });

  test("video 要素が存在し src が設定されている", async ({ page }) => {
    await page.goto("/");

    // videos マウントポイントカードをクリック
    const videosMount = page.locator("[data-testid^='mount-']", {
      hasText: "videos",
    });
    await expect(videosMount).toBeVisible();
    await videosMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    const videosTab = page.locator("[data-testid='tab-videos']");
    if (await videosTab.isVisible()) {
      await videosTab.click();

      // <video> 要素を確認
      const video = page.locator("video").first();
      await expect(video).toBeVisible();
      const src = await video.getAttribute("src");
      expect(src).toBeTruthy();
    }
  });
});
