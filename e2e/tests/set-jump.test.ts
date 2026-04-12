// セット間ジャンプ E2E テスト
// - 同親兄弟ジャンプ: 確認なしで即遷移
// - topDir 変更ジャンプ: 確認ダイアログ表示
// - 境界: 最初/最後のセットでは何も起きない
// - Shift+X: 常に確認なし
//
// フィクスチャ契約:
//   archive/zips/ に images.zip (3 JPEG) と mixed.zip (1 JPEG + 1 MP4)
//   nested/dirs/ に sub1/ (1 JPEG) と sub2/ (1 JPEG)
//   nested/extra/ に sub3/ (1 JPEG)  ← topDir 変更テスト用
//   ※ root 直下は parentNodeId=null → zips/, dirs/ でネスト

import { test, expect } from "@playwright/test";
import { clickFileCard } from "./helpers/navigation";

// archive/zips 内の images.zip で CG モードを開く
async function openCgInArchiveZip(page: import("@playwright/test").Page) {
  await page.goto("/");

  const archiveCard = page.locator("[data-testid^='mount-']", {
    hasText: "archive",
  });
  await expect(archiveCard).toBeVisible();
  await archiveCard.click();
  await expect(page).toHaveURL(/\/browse\//);

  // zips サブディレクトリに入る
  const zipsDir = page.locator("[data-testid^='file-card-']", {
    hasText: "zips",
  });
  await expect(zipsDir).toBeVisible();
  await zipsDir.dblclick();
  await expect(page).toHaveURL(/\/browse\//);

  const imagesZip = page.locator("[data-testid^='file-card-']", {
    hasText: "images.zip",
  });
  await expect(imagesZip).toBeVisible();
  await imagesZip.dblclick();
  await expect(page).toHaveURL(/\/browse\//);

  const imagesTab = page.locator("[data-testid='tab-images']");
  await expect(imagesTab).toBeVisible();
  await imagesTab.click();

  await clickFileCard(page.locator("[data-testid^='file-card-']").first());

  await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
}

// nested/dirs/sub1 で CG モードを開く
async function openCgInNestedSub1(page: import("@playwright/test").Page) {
  await page.goto("/");

  const nestedCard = page.locator("[data-testid^='mount-']", {
    hasText: "nested",
  });
  await expect(nestedCard).toBeVisible();
  await nestedCard.click();
  await expect(page).toHaveURL(/\/browse\//);

  const dirsDir = page.locator("[data-testid^='file-card-']", {
    hasText: "dirs",
  });
  await expect(dirsDir).toBeVisible();
  await dirsDir.dblclick();
  await expect(page).toHaveURL(/\/browse\//);

  const sub1 = page.locator("[data-testid^='file-card-']", {
    hasText: "sub1",
  });
  await expect(sub1).toBeVisible();
  await sub1.dblclick();
  await expect(page).toHaveURL(/\/browse\//);

  const imagesTab = page.locator("[data-testid='tab-images']");
  await expect(imagesTab).toBeVisible();
  await imagesTab.click();

  await clickFileCard(page.locator("[data-testid^='file-card-']").first());

  await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
}

// nested/dirs/sub2 で CG モードを開く (topDir 変更テスト: dirs → extra)
async function openCgInNestedSub2(page: import("@playwright/test").Page) {
  await page.goto("/");

  const nestedCard = page.locator("[data-testid^='mount-']", {
    hasText: "nested",
  });
  await expect(nestedCard).toBeVisible();
  await nestedCard.click();
  await expect(page).toHaveURL(/\/browse\//);

  const dirsDir = page.locator("[data-testid^='file-card-']", {
    hasText: "dirs",
  });
  await expect(dirsDir).toBeVisible();
  await dirsDir.dblclick();
  await expect(page).toHaveURL(/\/browse\//);

  const sub2 = page.locator("[data-testid^='file-card-']", {
    hasText: "sub2",
  });
  await expect(sub2).toBeVisible();
  await sub2.dblclick();
  await expect(page).toHaveURL(/\/browse\//);

  const imagesTab = page.locator("[data-testid='tab-images']");
  await expect(imagesTab).toBeVisible();
  await imagesTab.click();

  await clickFileCard(page.locator("[data-testid^='file-card-']").first());

  await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
}

test.describe("同親兄弟ジャンプ — 確認なし", () => {
  test("X キーで同親のアーカイブに確認なしで遷移する", async ({ page }) => {
    await openCgInArchiveZip(page);
    const initialUrl = page.url();

    await page.keyboard.press("x");

    // 確認ダイアログなしで即遷移
    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).not.toBeVisible();
    await expect(page).not.toHaveURL(initialUrl, { timeout: 5000 });
    await expect(page).toHaveURL(/\/browse\//);
  });

  test("X キーで同親ディレクトリ sub1 → sub2 に確認なしで遷移する", async ({ page }) => {
    await openCgInNestedSub1(page);
    const initialUrl = page.url();

    await page.keyboard.press("x");

    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).not.toBeVisible();
    await expect(page).not.toHaveURL(initialUrl, { timeout: 5000 });
    await expect(page).toHaveURL(/\/browse\//);
  });

  test("Z キーで同親ディレクトリ sub2 → sub1 に確認なしで遷移する", async ({ page }) => {
    await openCgInNestedSub2(page);
    const initialUrl = page.url();

    await page.keyboard.press("z");

    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).not.toBeVisible();
    await expect(page).not.toHaveURL(initialUrl, { timeout: 5000 });
    await expect(page).toHaveURL(/\/browse\//);
  });

  test("最初のセットで Z を押すと境界トーストが表示される", async ({ page }) => {
    await openCgInArchiveZip(page);
    const initialUrl = page.url();

    await page.keyboard.press("z");

    // URL は変わらない
    await expect(page).toHaveURL(initialUrl);
    // 境界トーストが表示される
    const toast = page.getByText("最初のセットです");
    await expect(toast).toBeVisible({ timeout: 3000 });
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
  });
});

test.describe("topDir 変更ジャンプ — 確認あり", () => {
  test("X キーで topDir 変更時に確認ダイアログが表示される", async ({ page }) => {
    // sub2 は dirs/ の最後のセット → X で nested root → extra/ にクロスジャンプ
    await openCgInNestedSub2(page);

    await page.keyboard.press("x");

    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).toBeVisible({ timeout: 5000 });
    await expect(prompt).toContainText("次のディレクトリに移動しますか？");
  });

  test("確認ダイアログで「はい」を押すと遷移する", async ({ page }) => {
    await openCgInNestedSub2(page);
    const initialUrl = page.url();

    await page.keyboard.press("x");
    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).toBeVisible({ timeout: 5000 });

    await prompt.locator("button", { hasText: "はい" }).click();
    await expect(page).not.toHaveURL(initialUrl);
    await expect(page).toHaveURL(/\/browse\//);
  });

  test("確認ダイアログで「いいえ」を押すとキャンセルされる", async ({ page }) => {
    await openCgInNestedSub2(page);

    await page.keyboard.press("x");
    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).toBeVisible({ timeout: 5000 });

    await prompt.locator("button", { hasText: "いいえ" }).click();
    await expect(prompt).not.toBeVisible();
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
  });

  test("Y キーで確認ダイアログを承認して遷移する", async ({ page }) => {
    await openCgInNestedSub2(page);
    const initialUrl = page.url();

    await page.keyboard.press("x");
    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).toBeVisible({ timeout: 5000 });

    await page.keyboard.press("y");
    await expect(page).not.toHaveURL(initialUrl);
  });

  test("N キーで確認ダイアログをキャンセルする", async ({ page }) => {
    await openCgInNestedSub2(page);

    await page.keyboard.press("x");
    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).toBeVisible({ timeout: 5000 });

    await page.keyboard.press("n");
    await expect(prompt).not.toBeVisible();
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
  });
});

test.describe("Shift+X/Z — 常に確認なし", () => {
  test("Shift+X で確認なしに sub1 → sub2 へ直接遷移する", async ({ page }) => {
    await openCgInNestedSub1(page);
    const initialUrl = page.url();

    await page.keyboard.press("Shift+x");

    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).not.toBeVisible();
    await expect(page).not.toHaveURL(initialUrl, { timeout: 5000 });
    await expect(page).toHaveURL(/\/browse\//);
  });

  test("Shift+X で topDir 変更でも確認なしで遷移する", async ({ page }) => {
    await openCgInNestedSub2(page);
    const initialUrl = page.url();

    await page.keyboard.press("Shift+x");

    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).not.toBeVisible();
    await expect(page).not.toHaveURL(initialUrl, { timeout: 5000 });
    await expect(page).toHaveURL(/\/browse\//);
  });
});

test.describe("NavigationPrompt 自動消去", () => {
  test("5秒で NavigationPrompt が自動消去される", async ({ page }) => {
    await openCgInNestedSub2(page);

    await page.keyboard.press("x");
    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).toBeVisible({ timeout: 5000 });

    // 5秒のタイマーで自動消去 — toBeHidden のタイムアウトで待機
    await expect(prompt).not.toBeVisible({ timeout: 6000 });

    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
  });
});
