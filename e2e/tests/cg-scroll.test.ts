// CG スクロールテスト (P1)
// CS-1: S キーで下スクロール、CS-2: W キーで上スクロール
// CgViewer の scrollUp/scrollDown が空関数のため、現在は fixme

import { test, expect } from "@playwright/test";
import { openCgViewer } from "./helpers/navigation";

test.use({ viewport: { width: 1024, height: 200 } });

test.describe("CG スクロール", () => {
  // CgViewer.tsx L100-101: scrollUp/scrollDown が空関数で未実装
  test.fixme("CS-1: S キーで下にスクロールする", async ({ page }) => {
    await openCgViewer(page);
    const imageArea = page.getByTestId("cg-image-area");

    await page.keyboard.press("s");

    await expect.poll(
      () => imageArea.evaluate((el) => el.scrollTop),
      { message: "S キーで scrollTop が増加するはず", timeout: 5000 },
    ).toBeGreaterThan(0);
  });

  test.fixme("CS-2: W キーで上にスクロールする", async ({ page }) => {
    await openCgViewer(page);
    const imageArea = page.getByTestId("cg-image-area");

    await page.keyboard.press("s");
    await expect.poll(
      () => imageArea.evaluate((el) => el.scrollTop),
    ).toBeGreaterThan(0);

    const scrollAfterS = await imageArea.evaluate((el) => el.scrollTop);

    await page.keyboard.press("w");
    await expect.poll(
      () => imageArea.evaluate((el) => el.scrollTop),
      { message: "W キーで scrollTop が減少するはず" },
    ).toBeLessThan(scrollAfterS);
  });
});
