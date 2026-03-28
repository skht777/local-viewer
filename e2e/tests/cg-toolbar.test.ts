// CG ツールバー操作テスト (P1)
// CT-1: V キーで幅フィット、CT-2: H キーで高さフィット、CT-5: Q キーで見開きサイクル

import { test, expect } from "@playwright/test";
import { openCgViewer } from "./helpers/navigation";

test.describe("CG ツールバー — キーバインド", () => {
  test("CT-1: V キーで幅フィットに切り替わる", async ({ page }) => {
    await openCgViewer(page);

    await page.keyboard.press("v");

    const wBtn = page.getByRole("button", { name: "幅フィット" });
    await expect(wBtn).toHaveAttribute("aria-pressed", "true");
  });

  test("CT-2: H キーで高さフィットに切り替わる", async ({ page }) => {
    await openCgViewer(page);

    await page.keyboard.press("h");

    const hBtn = page.getByRole("button", { name: "高さフィット" });
    await expect(hBtn).toHaveAttribute("aria-pressed", "true");
  });

  test("CT-5: Q キーで見開きモードがサイクルする", async ({ page }) => {
    await openCgViewer(page);
    const spreadBtn = page.getByTestId("cg-spread-btn");

    // single → spread → spread-offset → single
    await expect(spreadBtn).toHaveText("1");
    await page.keyboard.press("q");
    await expect(spreadBtn).toHaveText("2");
    await page.keyboard.press("q");
    await expect(spreadBtn).toHaveText("2+");
    await page.keyboard.press("q");
    await expect(spreadBtn).toHaveText("1");
  });
});
