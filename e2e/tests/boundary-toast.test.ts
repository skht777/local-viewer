// 画像/ページ境界トースト通知 E2E テスト
// - 最初/最後の画像でナビゲーション試行時にトースト表示
// - 2秒後に自動消去
// - 中間画像ではトーストが表示されない

import { test, expect } from "@playwright/test";
import { openCgViewer, openPdfViewer } from "./helpers/navigation";

test.describe("画像境界トースト — CG モード", () => {
  test("最初の画像で A キーを押すと「最初の画像です」トーストが表示される", async ({
    page,
  }) => {
    await openCgViewer(page);
    await expect(page).toHaveURL(/index=0/);

    await page.keyboard.press("a");

    const toast = page.getByTestId("viewer-toast");
    await expect(toast).toBeVisible();
    await expect(toast).toContainText("最初の画像です");
  });

  test("最後の画像で D キーを押すと「最後の画像です」トーストが表示される", async ({
    page,
  }) => {
    await openCgViewer(page);

    // End キーで最後の画像に移動し、URL が更新されるのを待つ
    await page.keyboard.press("End");
    await expect(page).toHaveURL(/index=3/);

    await page.keyboard.press("d");

    const toast = page.getByTestId("viewer-toast");
    await expect(toast).toBeVisible();
    await expect(toast).toContainText("最後の画像です");
  });

  test("トーストが自動消去される", async ({ page }) => {
    await openCgViewer(page);
    await expect(page).toHaveURL(/index=0/);

    await page.keyboard.press("a");

    const toast = page.getByTestId("viewer-toast");
    await expect(toast).toBeVisible();

    // 2秒のタイマーで自動消去
    await expect(toast).not.toBeVisible({ timeout: 3000 });
  });

  test("中間画像ではトーストが表示されない", async ({ page }) => {
    await openCgViewer(page);
    await expect(page).toHaveURL(/index=0/);

    // D キーで次の画像に移動（中間画像）
    await page.keyboard.press("d");
    await expect(page).toHaveURL(/index=1/);

    const toast = page.getByTestId("viewer-toast");
    await expect(toast).not.toBeVisible();
  });
});

test.describe("ページ境界トースト — PDF CG モード", () => {
  test("最初のページで A キーを押すと「最初のページです」トーストが表示される", async ({
    page,
  }) => {
    await openPdfViewer(page);

    await page.keyboard.press("a");

    const toast = page.getByTestId("viewer-toast");
    await expect(toast).toBeVisible();
    await expect(toast).toContainText("最初のページです");
  });
});
