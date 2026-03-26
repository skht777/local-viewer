// ナビゲーションテスト
// 仕様出典: initial-architecture.md §画面構成, plan-phase1.md

import { test, expect } from "@playwright/test";

test.describe("ナビゲーション", () => {
  test("マウントポイントカードクリックでブラウズ画面に遷移する", async ({
    page,
  }) => {
    await page.goto("/");
    const picturesCard = page.locator("[data-testid^='mount-']", {
      hasText: "pictures",
    });
    await expect(picturesCard).toBeVisible();
    await picturesCard.click();
    await expect(page).toHaveURL(/\/browse\//);
  });

  test("ディレクトリツリーが表示される", async ({ page }) => {
    await page.goto("/");
    const picturesCard = page.locator("[data-testid^='mount-']", {
      hasText: "pictures",
    });
    await expect(picturesCard).toBeVisible();
    await picturesCard.click();
    // ツリーノードが表示される
    const treeNodes = page.locator("[data-testid^='tree-node-']");
    await expect(treeNodes.first()).toBeVisible();
  });

  test("タブ切り替えが機能し URL に反映される", async ({ page }) => {
    await page.goto("/");
    const picturesCard = page.locator("[data-testid^='mount-']", {
      hasText: "pictures",
    });
    await expect(picturesCard).toBeVisible();
    await picturesCard.click();
    await expect(page).toHaveURL(/\/browse\//);

    // 画像タブをクリック
    const imagesTab = page.locator("[data-testid='tab-images']");
    if (await imagesTab.isVisible()) {
      await imagesTab.click();
      await expect(page).toHaveURL(/tab=images/);
    }
  });

  test("動画タブに切り替えられる", async ({ page }) => {
    await page.goto("/");
    // videos マウントポイントに移動
    const videosCard = page.locator("[data-testid^='mount-']", {
      hasText: "videos",
    });
    await expect(videosCard).toBeVisible();
    await videosCard.click();
    await expect(page).toHaveURL(/\/browse\//);

    const videosTab = page.locator("[data-testid='tab-videos']");
    if (await videosTab.isVisible()) {
      await videosTab.click();
      await expect(page).toHaveURL(/tab=videos/);
    }
  });

  test("BrowseHeader でトップページに戻れる", async ({ page }) => {
    await page.goto("/");
    const picturesCard = page.locator("[data-testid^='mount-']", {
      hasText: "pictures",
    });
    await expect(picturesCard).toBeVisible();
    await picturesCard.click();
    await expect(page).toHaveURL(/\/browse\//);

    // 「← トップ」ボタンをクリック
    const backButton = page.getByRole("button", { name: /トップ/ });
    await expect(backButton).toBeVisible();
    await backButton.click();
    await expect(page).toHaveURL("/");
  });
});
