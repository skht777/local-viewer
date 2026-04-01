// セット間ジャンプ E2E テスト
// - X/PageDown: 確認ダイアログ付きで次のセットへ移動
// - Z/PageUp: 確認ダイアログ付きで前のセットへ移動
// - 境界: 最初/最後のセットでは何も起きない
// - ディレクトリ間ジャンプ: nested/sub1 → nested/sub2
//
// フィクスチャ契約:
//   archive/zips/ に images.zip (3 JPEG) と mixed.zip (1 JPEG + 1 MP4)
//   nested/dirs/ に sub1/ (1 JPEG) と sub2/ (1 JPEG)
//   ※ root 直下は parentNodeId=null のため set-jump 不可 → zips/, dirs/ でネスト

import { test, expect } from "@playwright/test";
import { clickFileCard } from "./helpers/navigation";

// archive/zips 内の images.zip で CG モードを開く
// ※ set-jump にはルート直下でない位置が必要 (parentNodeId != null)
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

// nested/dirs/sub1 で CG モードを開く (ディレクトリ間ジャンプ用)
// ※ set-jump にはルート直下でない位置が必要 (parentNodeId != null)
async function openCgInNestedSub1(page: import("@playwright/test").Page) {
  await page.goto("/");

  // nested マウントポイントへ
  const nestedCard = page.locator("[data-testid^='mount-']", {
    hasText: "nested",
  });
  await expect(nestedCard).toBeVisible();
  await nestedCard.click();
  await expect(page).toHaveURL(/\/browse\//);

  // dirs サブディレクトリへ
  const dirsDir = page.locator("[data-testid^='file-card-']", {
    hasText: "dirs",
  });
  await expect(dirsDir).toBeVisible();
  await dirsDir.dblclick();
  await expect(page).toHaveURL(/\/browse\//);

  // sub1 ディレクトリへ
  const sub1 = page.locator("[data-testid^='file-card-']", {
    hasText: "sub1",
  });
  await expect(sub1).toBeVisible();
  await sub1.dblclick();
  await expect(page).toHaveURL(/\/browse\//);

  // 画像タブ
  const imagesTab = page.locator("[data-testid='tab-images']");
  await expect(imagesTab).toBeVisible();
  await imagesTab.click();

  // 画像をクリック → CG ビューワー
  await clickFileCard(page.locator("[data-testid^='file-card-']").first());

  await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
}

test.describe("セット間ジャンプ — アーカイブ間", () => {
  test("X キーで NavigationPrompt が表示される", async ({ page }) => {
    await openCgInArchiveZip(page);

    await page.keyboard.press("x");

    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).toBeVisible({ timeout: 5000 });
    await expect(prompt).toContainText("次のディレクトリに移動しますか？");
  });

  test("X → はいボタンで次のセットに遷移する", async ({ page }) => {
    await openCgInArchiveZip(page);
    const initialUrl = page.url();

    await page.keyboard.press("x");
    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).toBeVisible({ timeout: 5000 });

    // 「はい」ボタンをクリック → 遷移 (images.zip → mixed.zip)
    await prompt.locator("button", { hasText: "はい" }).click();
    await expect(page).not.toHaveURL(initialUrl);
    await expect(page).toHaveURL(/\/browse\//);
  });

  test("X → いいえボタンでキャンセルされる", async ({ page }) => {
    await openCgInArchiveZip(page);

    await page.keyboard.press("x");
    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).toBeVisible({ timeout: 5000 });

    await prompt.locator("button", { hasText: "いいえ" }).click();
    await expect(prompt).not.toBeVisible();
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
  });

  test("最初のセットで Z を押してもプロンプトが出ない", async ({ page }) => {
    await openCgInArchiveZip(page);

    await page.keyboard.press("z");
    await page.waitForTimeout(1000);

    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).not.toBeVisible();
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
  });
});

test.describe("セット間ジャンプ — ディレクトリ間", () => {
  test("X で sub1 → sub2 に遷移する", async ({ page }) => {
    await openCgInNestedSub1(page);
    const initialUrl = page.url();

    await page.keyboard.press("x");
    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).toBeVisible({ timeout: 5000 });

    // 「はい」で遷移 → URL が変わる
    await prompt.locator("button", { hasText: "はい" }).click();
    await expect(page).not.toHaveURL(initialUrl);
    await expect(page).toHaveURL(/\/browse\//);
  });

  test("Shift+X で確認なしに sub1 → sub2 へ直接遷移する", async ({
    page,
  }) => {
    await openCgInNestedSub1(page);
    const initialUrl = page.url();

    // Shift+X はプロンプトなしで即座に遷移
    await page.keyboard.press("Shift+x");

    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).not.toBeVisible();
    await expect(page).not.toHaveURL(initialUrl, { timeout: 5000 });
    await expect(page).toHaveURL(/\/browse\//);
  });
});

test.describe("NavigationPrompt キーボード操作", () => {
  // Y/N キーバインドが NavigationPrompt に未実装 (UIテキストのみ)
  // 実装されたら fixme を解除する
  test("Y キーで次のセットに遷移する", async ({ page }) => {
    await openCgInArchiveZip(page);
    const initialUrl = page.url();

    await page.keyboard.press("x");
    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).toBeVisible({ timeout: 5000 });

    await page.keyboard.press("y");
    await expect(page).not.toHaveURL(initialUrl);
    await expect(page).toHaveURL(/\/browse\//);
  });

  test("N キーでキャンセルされる", async ({ page }) => {
    await openCgInArchiveZip(page);

    await page.keyboard.press("x");
    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).toBeVisible({ timeout: 5000 });

    await page.keyboard.press("n");
    await expect(prompt).not.toBeVisible();
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
  });

  // SJ-8: Enter キーも Y と同様に未実装
  test("SJ-8: Enter キーで次のセットに遷移する", async ({ page }) => {
    await openCgInArchiveZip(page);
    const initialUrl = page.url();

    await page.keyboard.press("x");
    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).toBeVisible({ timeout: 5000 });

    await page.keyboard.press("Enter");
    await expect(page).not.toHaveURL(initialUrl);
    await expect(page).toHaveURL(/\/browse\//);
  });
});

test.describe("NavigationPrompt 自動消去", () => {
  test("SJ-10: 5秒で NavigationPrompt が自動消去される", async ({ page }) => {
    await openCgInArchiveZip(page);

    await page.keyboard.press("x");
    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).toBeVisible({ timeout: 5000 });

    // 5秒のタイマーで自動消去 — waitForTimeout 禁止のため toBeHidden のタイムアウトで待機
    await expect(prompt).not.toBeVisible({ timeout: 6000 });

    // CG ビューワーは維持されている
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
  });
});
