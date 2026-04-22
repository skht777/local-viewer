// セット間ジャンプ UI ボタンの E2E テスト
// - CG / マンガ両モードで ⏪ / ⏩ ボタン経由のセット間ジャンプを検証
// - topDir 変更時に NavigationPrompt が表示され、表示中はボタンが disabled になる
// - キーボード経路（set-jump.test.ts）と独立して UI ボタン経路の回帰を担保
//
// フィクスチャ契約（set-jump.test.ts と同一）:
//   nested/dirs/ に sub1/ (JPEG) と sub2/ (JPEG)
//   nested/extra/ に sub3/ (JPEG)  ← topDir 変更テスト用

import type { Page } from "@playwright/test";
import { expect, test } from "@playwright/test";
import { clickFileCard, showToolbar } from "./helpers/navigation";

// nested/dirs/{subName} を開いて CG ビューワーを表示、ツールバーも表示済みにする
async function openCgInNestedDirs(page: Page, subName: "sub1" | "sub2") {
  await page.goto("/");

  const nestedCard = page.locator("[data-testid^='mount-']", { hasText: "nested" });
  await expect(nestedCard).toBeVisible();
  await nestedCard.click();
  await expect(page).toHaveURL(/\/browse\//);

  const dirsDir = page.locator("[data-testid^='file-card-']", { hasText: "dirs" });
  await expect(dirsDir).toBeVisible();
  await dirsDir.dblclick();
  await expect(page).toHaveURL(/\/browse\//);

  const sub = page.locator("[data-testid^='file-card-']", { hasText: subName });
  await expect(sub).toBeVisible();
  await sub.dblclick();
  await expect(page).toHaveURL(/\/browse\//);

  const imagesTab = page.locator("[data-testid='tab-images']");
  await expect(imagesTab).toBeVisible();
  await imagesTab.click();

  await clickFileCard(page.locator("[data-testid^='file-card-']").first());

  await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
  await showToolbar(page);
}

// nested/dirs/{subName} をマンガモードで開く
async function openMangaInNestedDirs(page: Page, subName: "sub1" | "sub2") {
  await page.goto("/");

  const nestedCard = page.locator("[data-testid^='mount-']", { hasText: "nested" });
  await expect(nestedCard).toBeVisible();
  await nestedCard.click();
  await expect(page).toHaveURL(/\/browse\//);

  // マンガモードに切替
  await page.getByTestId("mode-toggle-manga").click();
  await expect(page).toHaveURL(/mode=manga/);

  const dirsDir = page.locator("[data-testid^='file-card-']", { hasText: "dirs" });
  await expect(dirsDir).toBeVisible();
  await dirsDir.dblclick();
  await expect(page).toHaveURL(/\/browse\//);

  const sub = page.locator("[data-testid^='file-card-']", { hasText: subName });
  await expect(sub).toBeVisible();
  await sub.dblclick();
  await expect(page).toHaveURL(/\/browse\//);

  const imagesTab = page.locator("[data-testid='tab-images']");
  await expect(imagesTab).toBeVisible();
  await imagesTab.click();

  await clickFileCard(page.locator("[data-testid^='file-card-']").first());

  await expect(page.locator("[data-testid='manga-viewer']")).toBeVisible();
  await showToolbar(page);
}

test.describe("CG ツールバー — セット間ジャンプボタン", () => {
  test("⏩ ボタンで sub1 → sub2 へ遷移する（同親）", async ({ page }) => {
    await openCgInNestedDirs(page, "sub1");
    const initialUrl = page.url();

    await page.getByTestId("cg-next-set-btn").click();

    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).not.toBeVisible();
    await expect(page).not.toHaveURL(initialUrl, { timeout: 5000 });
    await expect(page).toHaveURL(/\/browse\//);
  });

  test("⏪ ボタンで sub2 → sub1 へ遷移する（同親）", async ({ page }) => {
    await openCgInNestedDirs(page, "sub2");
    const initialUrl = page.url();

    await page.getByTestId("cg-prev-set-btn").click();

    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).not.toBeVisible();
    await expect(page).not.toHaveURL(initialUrl, { timeout: 5000 });
    await expect(page).toHaveURL(/\/browse\//);
  });

  test("⏩ ボタン押下で topDir 変更時に NavigationPrompt が表示される", async ({ page }) => {
    // sub2 は dirs/ の最後のセット → ⏩ で extra/sub3 へクロスジャンプ（確認あり）
    await openCgInNestedDirs(page, "sub2");

    await page.getByTestId("cg-next-set-btn").click();

    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).toBeVisible({ timeout: 5000 });
    await expect(prompt).toContainText("次のディレクトリに移動しますか？");
  });

  test("NavigationPrompt 表示中はセット間ジャンプボタンが disabled になる", async ({ page }) => {
    await openCgInNestedDirs(page, "sub2");

    await page.getByTestId("cg-next-set-btn").click();

    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).toBeVisible({ timeout: 5000 });

    await expect(page.getByTestId("cg-prev-set-btn")).toBeDisabled();
    await expect(page.getByTestId("cg-next-set-btn")).toBeDisabled();
  });
});

test.describe("マンガツールバー — セット間ジャンプボタン", () => {
  test("⏩ ボタンで sub1 → sub2 へ遷移する（同親）", async ({ page }) => {
    await openMangaInNestedDirs(page, "sub1");
    const initialUrl = page.url();

    await page.getByTestId("manga-next-set-btn").click();

    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).not.toBeVisible();
    await expect(page).not.toHaveURL(initialUrl, { timeout: 5000 });
    await expect(page).toHaveURL(/\/browse\//);
  });

  test("⏪ ボタンで sub2 → sub1 へ遷移する（同親）", async ({ page }) => {
    await openMangaInNestedDirs(page, "sub2");
    const initialUrl = page.url();

    await page.getByTestId("manga-prev-set-btn").click();

    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).not.toBeVisible();
    await expect(page).not.toHaveURL(initialUrl, { timeout: 5000 });
    await expect(page).toHaveURL(/\/browse\//);
  });
});
