// CG モードテスト
// 仕様出典: plan-phase2.md, initial-architecture.md §CGモード

import { test, expect } from "@playwright/test";

// pictures ディレクトリに移動して画像タブでCGモードを開くヘルパー
async function openCgViewer(page: import("@playwright/test").Page) {
  await page.goto("/");

  // pictures マウントポイントカードをクリック
  const picturesCard = page.locator("[data-testid^='mount-']", {
    hasText: "pictures",
  });
  await expect(picturesCard).toBeVisible();
  await picturesCard.click();
  await expect(page).toHaveURL(/\/browse\//);

  // 画像タブに切り替え
  const imagesTab = page.locator("[data-testid='tab-images']");
  await expect(imagesTab).toBeVisible();
  await imagesTab.click();

  // 画像カードが安定するまで待つ
  const firstImage = page.locator("[data-testid^='file-card-']").first();
  await expect(firstImage).toBeVisible();

  // サムネイル読み込みによるDOM再構築を待つ
  await firstImage.click({ force: true });

  // CGビューワーが表示される
  await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
}

test.describe("CGモード", () => {
  test("画像クリックで CG ビューワーが開き URL が更新される", async ({
    page,
  }) => {
    await openCgViewer(page);
    await expect(page).toHaveURL(/mode=cg/);
    await expect(page).toHaveURL(/index=0/);
  });

  test("ページカウンターが正しく表示される", async ({ page }) => {
    await openCgViewer(page);
    const counter = page.locator("[data-testid='page-counter']");
    await expect(counter).toBeVisible();
    await expect(counter).toHaveText(/1\s*\/\s*\d+/);
  });

  test("D キーで次ページに進める", async ({ page }) => {
    await openCgViewer(page);
    await expect(page).toHaveURL(/index=0/);
    await page.keyboard.press("d");
    await expect(page).toHaveURL(/index=1/);
  });

  test("A キーで前ページに戻れる", async ({ page }) => {
    await openCgViewer(page);
    // D で次ページに進む
    await page.keyboard.press("d");
    await expect(page).toHaveURL(/index=1/);
    // ページ遷移のアニメーション後に A で戻る
    await page.waitForTimeout(200);
    await page.keyboard.press("a");
    await expect(page).toHaveURL(/index=0/);
  });

  test("Escape でビューワーを閉じてブラウズに戻る", async ({ page }) => {
    await openCgViewer(page);
    await page.keyboard.press("Escape");
    await expect(page.locator("[data-testid='cg-viewer']")).not.toBeVisible();
    await expect(page).not.toHaveURL(/index=/);
    await expect(page).not.toHaveURL(/mode=/);
  });

  test("M キーでマンガモードに切り替わる", async ({ page }) => {
    await openCgViewer(page);
    await page.keyboard.press("m");
    await expect(page).toHaveURL(/mode=manga/);
    await expect(
      page.locator("[data-testid='manga-viewer']"),
    ).toBeVisible();
  });
});
