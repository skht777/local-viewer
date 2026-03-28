// CG スクロールテスト
// P1: CS-1(S下スクロール), CS-2(W上スクロール)
// P2: CS-3(↓下スクロール), CS-4(↑上スクロール), CS-5(ページスライダー)
// CgViewer の scrollUp/scrollDown が空関数のため、CS-1〜4 は fixme

import { test, expect } from "@playwright/test";
import { openCgViewer } from "./helpers/navigation";

test.use({ viewport: { width: 1024, height: 200 } });

test.describe("CG スクロール", () => {
  // CgViewer.tsx L100-101: scrollUp/scrollDown が空関数で未実装
  test("CS-1: S キーで下にスクロールする", async ({ page }) => {
    await openCgViewer(page);
    const imageArea = page.getByTestId("cg-image-area");

    await page.keyboard.press("s");

    await expect.poll(
      () => imageArea.evaluate((el) => el.scrollTop),
      { message: "S キーで scrollTop が増加するはず", timeout: 5000 },
    ).toBeGreaterThan(0);
  });

  test("CS-2: W キーで上にスクロールする", async ({ page }) => {
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

  // CS-3, CS-4: ↓↑ キーも scrollUp/scrollDown と同じ空関数を呼ぶため fixme
  test("CS-3: ↓キーで下にスクロールする", async ({ page }) => {
    await openCgViewer(page);
    const imageArea = page.getByTestId("cg-image-area");

    await page.keyboard.press("ArrowDown");

    await expect.poll(
      () => imageArea.evaluate((el) => el.scrollTop),
      { message: "↓キーで scrollTop が増加するはず", timeout: 5000 },
    ).toBeGreaterThan(0);
  });

  test("CS-4: ↑キーで上にスクロールする", async ({ page }) => {
    await openCgViewer(page);
    const imageArea = page.getByTestId("cg-image-area");

    await page.keyboard.press("ArrowDown");
    await expect.poll(
      () => imageArea.evaluate((el) => el.scrollTop),
    ).toBeGreaterThan(0);

    const scrollAfterDown = await imageArea.evaluate((el) => el.scrollTop);

    await page.keyboard.press("ArrowUp");
    await expect.poll(
      () => imageArea.evaluate((el) => el.scrollTop),
      { message: "↑キーで scrollTop が減少するはず" },
    ).toBeLessThan(scrollAfterDown);
  });

  // CS-5: ページスライダーの存在を確認 — CgToolbar には <select> のみでスライダー無し
  test("CS-5: ページスライダーでページが変更される", async ({ page }) => {
    await openCgViewer(page);

    // CgToolbar にはページスライダー (range input) が未実装
    const slider = page.getByTestId("cg-viewer").locator("input[type='range']");
    await expect(slider).toBeVisible();

    // スライダーを操作して index が変化するか確認
    await slider.fill("2");
    await expect(page).toHaveURL(/index=2/);
  });
});
