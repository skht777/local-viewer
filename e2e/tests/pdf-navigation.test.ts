// PDF ナビゲーションテスト (P1)
// PN-1: D 次ページ、PN-2: A 前ページ、PN-3: M マンガ切替
// PN-4: M CG 復帰、PN-5: Escape 閉じ

import { test, expect } from "@playwright/test";
import { openPdfViewer } from "./helpers/navigation";

test.describe("PDF ナビゲーション", () => {
  test("PN-1: D キーで次ページに進む", async ({ page }) => {
    await openPdfViewer(page);
    await expect(page).toHaveURL(/page=1/);

    await page.keyboard.press("d");
    await expect(page).toHaveURL(/page=2/);
  });

  test("PN-2: A キーで前ページに戻る", async ({ page }) => {
    await openPdfViewer(page);

    // まず次ページへ
    await page.keyboard.press("d");
    await expect(page).toHaveURL(/page=2/);

    await page.keyboard.press("a");
    await expect(page).toHaveURL(/page=1/);
  });

  test("PN-3: M キーでマンガモードに切り替わる", async ({ page }) => {
    await openPdfViewer(page);

    await page.keyboard.press("m");

    await expect(page.getByTestId("pdf-manga-viewer")).toBeVisible();
    await expect(page).toHaveURL(/mode=manga/);
  });

  // PDF マンガ → CG 切替で pdf-cg-viewer の描画が遅延する問題を調査中
  test.fixme("PN-4: M キーで CG モードに復帰する", async ({ page }) => {
    await openPdfViewer(page);

    // CG → マンガ
    await page.keyboard.press("m");
    await expect(page.getByTestId("pdf-manga-viewer")).toBeVisible();
    await expect(page).toHaveURL(/mode=manga/);

    // マンガ → CG (URL パラメータで確認)
    await page.keyboard.press("m");
    await expect(page).toHaveURL(/mode=cg/);
    // PDF CG ビューワーの canvas 描画を待機
    await expect(page.getByTestId("pdf-cg-viewer")).toBeVisible({ timeout: 15_000 });
  });

  test("PN-5: Escape で CG ビューワーを閉じる", async ({ page }) => {
    await openPdfViewer(page);
    await expect(page).toHaveURL(/pdf=/);

    await page.keyboard.press("Escape");

    // URL から pdf/page/mode が消去される
    await expect(page).not.toHaveURL(/pdf=/);
    await expect(page).not.toHaveURL(/page=/);
    await expect(page.getByTestId("pdf-cg-viewer")).not.toBeVisible();
  });
});
