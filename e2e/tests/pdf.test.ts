// PDF テスト
// 仕様出典: plan-phase6.md, initial-architecture.md §PDF

import { test, expect } from "@playwright/test";

test.describe("PDF ビューワー", () => {
  test("PDF クリックで PDF ビューワーが開く", async ({ page }) => {
    await page.goto("/");

    // docs マウントポイントカードをクリック
    const docsMount = page.locator("[data-testid^='mount-']", {
      hasText: "docs",
    });
    await expect(docsMount).toBeVisible();
    await docsMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    // PDF ファイルカードをクリック
    const pdfCard = page.locator("[data-testid^='file-card-']", {
      hasText: "sample.pdf",
    });
    await expect(pdfCard).toBeVisible();
    await pdfCard.click();

    // PDF ビューワーが開く
    const pdfViewer = page
      .locator("[data-testid='pdf-cg-viewer'], [data-testid='pdf-manga-viewer']")
      .first();
    await expect(pdfViewer).toBeVisible();

    // URL に pdf パラメータが含まれる
    await expect(page).toHaveURL(/pdf=/);
  });

  test("PDF ビューワーの URL に page パラメータが含まれる", async ({
    page,
  }) => {
    await page.goto("/");

    // docs マウントポイントカードをクリック
    const docsMount = page.locator("[data-testid^='mount-']", {
      hasText: "docs",
    });
    await expect(docsMount).toBeVisible();
    await docsMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    const pdfCard = page.locator("[data-testid^='file-card-']", {
      hasText: "sample.pdf",
    });
    await expect(pdfCard).toBeVisible();
    await pdfCard.click();

    await expect(page).toHaveURL(/page=\d+/);
  });

  test("PDF ビューワーでページカウンターが表示される", async ({ page }) => {
    await page.goto("/");

    // docs マウントポイントカードをクリック
    const docsMount = page.locator("[data-testid^='mount-']", {
      hasText: "docs",
    });
    await expect(docsMount).toBeVisible();
    await docsMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    const pdfCard = page.locator("[data-testid^='file-card-']", {
      hasText: "sample.pdf",
    });
    await expect(pdfCard).toBeVisible();
    await pdfCard.click();

    const counter = page.locator("[data-testid='page-counter']");
    await expect(counter).toBeVisible();
  });
});
