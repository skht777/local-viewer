// マンガモード操作テスト (P1)
// MC-1: + ズームイン、MC-2: - ズームアウト、MC-3: 0 リセット、MC-6: S スクロール

import { test, expect } from "@playwright/test";
import { openMangaViewer } from "./helpers/navigation";

test.describe("マンガモード — キーバインド", () => {
  test("MC-1: + キーでズームインする", async ({ page }) => {
    await openMangaViewer(page);

    await page.keyboard.press("Equal"); // = キー (+ のバインド)

    const zoomLevel = page.getByTestId("manga-zoom-level");
    await expect(zoomLevel).toHaveText("125%");
  });

  test("MC-2: - キーでズームアウトする", async ({ page }) => {
    await openMangaViewer(page);

    await page.keyboard.press("Minus");

    const zoomLevel = page.getByTestId("manga-zoom-level");
    await expect(zoomLevel).toHaveText("75%");
  });

  test("MC-3: 0 キーでズームリセットする", async ({ page }) => {
    await openMangaViewer(page);

    // まずズーム変更
    await page.keyboard.press("Equal");
    await expect(page.getByTestId("manga-zoom-level")).toHaveText("125%");

    // 0 でリセット
    await page.keyboard.press("Digit0");
    await expect(page.getByTestId("manga-zoom-level")).toHaveText("100%");
  });

  test("MC-6: S キーで下にスクロールする", async ({ page }) => {
    await openMangaViewer(page);

    const scrollArea = page.getByTestId("manga-scroll-area");
    const initialScroll = await scrollArea.evaluate((el) => el.scrollTop);

    await page.keyboard.press("s");

    await expect.poll(
      () => scrollArea.evaluate((el) => el.scrollTop),
      { message: "S キーで scrollTop が増加するはず" },
    ).toBeGreaterThan(initialScroll);
  });
});
