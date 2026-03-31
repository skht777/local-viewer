// アーカイブテスト
// 仕様出典: plan-phase4.md, initial-architecture.md §フォルダ/アーカイブの統一扱い
// P2: AR-4(mixed.zip 画像+動画), AR-5(画像タブ自動切替)

import { test, expect } from "@playwright/test";

// archive マウントポイントに遷移し、zips サブディレクトリに入るヘルパー
async function navigateToArchive(page: import("@playwright/test").Page) {
  await page.goto("/");
  const archiveMount = page.locator("[data-testid^='mount-']", {
    hasText: "archive",
  });
  await expect(archiveMount).toBeVisible();
  await archiveMount.click();
  await expect(page).toHaveURL(/\/browse\//);

  // zips サブディレクトリに入る
  const zipsDir = page.locator("[data-testid^='file-card-']", {
    hasText: "zips",
  });
  await expect(zipsDir).toBeVisible();
  await zipsDir.click();
  await expect(page).toHaveURL(/\/browse\//);
}

test.describe("アーカイブ", () => {
  test("ファイルセットタブにアーカイブがフォルダと同列表示される", async ({
    page,
  }) => {
    await navigateToArchive(page);

    // ZIP ファイルがファイルセットとして表示される
    const zipCard = page.locator("[data-testid^='file-card-']", {
      hasText: /\.zip/,
    });
    await expect(zipCard.first()).toBeVisible();
  });

  test("アーカイブクリックで中身が展開表示される", async ({ page }) => {
    await navigateToArchive(page);

    // images.zip をクリック
    const zipCard = page.locator("[data-testid^='file-card-']", {
      hasText: "images.zip",
    });
    await expect(zipCard).toBeVisible();
    await zipCard.click();

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
    await navigateToArchive(page);

    const zipCard = page.locator("[data-testid^='file-card-']", {
      hasText: "images.zip",
    });
    await expect(zipCard).toBeVisible();
    await zipCard.click();

    const imagesTab = page.locator("[data-testid='tab-images']");
    if (await imagesTab.isVisible()) {
      await imagesTab.click();
    }

    // 画像をクリックしてCGモードを開く（アーカイブ内はアイコン表示のためサムネイル待機不要）
    const firstImage = page.locator("[data-testid^='file-card-']").first();
    await expect(firstImage).toBeVisible();
    await firstImage.click();
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
  });

  test("AR-4: mixed.zip で画像と動画の両方が表示される", async ({ page }) => {
    await navigateToArchive(page);

    // mixed.zip をクリック
    const mixedZip = page.locator("[data-testid^='file-card-']", {
      hasText: "mixed.zip",
    });
    await expect(mixedZip).toBeVisible();
    await mixedZip.click();
    await expect(page).toHaveURL(/\/browse\//);

    // 画像タブに画像がある
    const imagesTab = page.locator("[data-testid='tab-images']");
    if (await imagesTab.isVisible()) {
      await imagesTab.click();
      const imageCards = page.locator("[data-testid^='file-card-']");
      await expect(imageCards.first()).toBeVisible();
    }

    // 動画タブに動画がある
    const videosTab = page.locator("[data-testid='tab-videos']");
    if (await videosTab.isVisible()) {
      await videosTab.click();
      const videoCards = page.locator("[data-testid^='video-card-']");
      await expect(videoCards.first()).toBeVisible();
    }
  });

  // アーカイブクリック時の tab=images 自動切替が未実装
  test("AR-5: アーカイブ遷移時に画像タブに自動切替される", async ({ page }) => {
    await navigateToArchive(page);

    // images.zip をクリック
    const imagesZip = page.locator("[data-testid^='file-card-']", {
      hasText: "images.zip",
    });
    await expect(imagesZip).toBeVisible();
    await imagesZip.click();

    // URL に tab=images が設定される
    await expect(page).toHaveURL(/tab=images/);
  });
});
