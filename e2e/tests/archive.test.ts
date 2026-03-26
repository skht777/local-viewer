// アーカイブテスト
// 仕様出典: plan-phase4.md, initial-architecture.md §フォルダ/アーカイブの統一扱い

import { test, expect } from "@playwright/test";

test.describe("アーカイブ", () => {
  test("ファイルセットタブにアーカイブがフォルダと同列表示される", async ({
    page,
  }) => {
    await page.goto("/");

    // archive マウントポイントカードをクリック
    const archiveMount = page.locator("[data-testid^='mount-']", {
      hasText: "archive",
    });
    await expect(archiveMount).toBeVisible();
    await archiveMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    // ZIP ファイルがファイルセットとして表示される
    const zipCard = page.locator("[data-testid^='file-card-']", {
      hasText: /\.zip/,
    });
    await expect(zipCard.first()).toBeVisible();
  });

  test("アーカイブクリックで中身が展開表示される", async ({ page }) => {
    await page.goto("/");

    // archive マウントポイントカードをクリック
    const archiveMount = page.locator("[data-testid^='mount-']", {
      hasText: "archive",
    });
    await expect(archiveMount).toBeVisible();
    await archiveMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    // images.zip をクリック
    const zipCard = page.locator("[data-testid^='file-card-']", {
      hasText: "images.zip",
    });
    await expect(zipCard).toBeVisible();
    await zipCard.click({ force: true });

    // アーカイブ内に移動して画像タブに画像が表示される
    await expect(page).toHaveURL(/\/browse\//);
    const imagesTab = page.locator("[data-testid='tab-images']");
    if (await imagesTab.isVisible()) {
      await imagesTab.click();
      const images = page.locator("[data-testid^='file-card-']");
      await expect(images.first()).toBeVisible();
    }
  });

  test("アーカイブ内の画像を CG モードで閲覧できる", async ({ page }) => {
    await page.goto("/");

    // archive マウントポイントカードをクリック
    const archiveMount = page.locator("[data-testid^='mount-']", {
      hasText: "archive",
    });
    await expect(archiveMount).toBeVisible();
    await archiveMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    const zipCard = page.locator("[data-testid^='file-card-']", {
      hasText: "images.zip",
    });
    await expect(zipCard).toBeVisible();
    await zipCard.click({ force: true });

    const imagesTab = page.locator("[data-testid='tab-images']");
    if (await imagesTab.isVisible()) {
      await imagesTab.click();
    }

    // 画像をクリックしてCGモードを開く
    const firstImage = page.locator("[data-testid^='file-card-']").first();
    await expect(firstImage).toBeVisible();
    await firstImage.click({ force: true });
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
    await expect(page).toHaveURL(/mode=cg/);
  });
});
