// セット間ジャンプ E2E テスト
// - X/PageDown: 確認ダイアログ付きで次のセットへ移動
// - Z/PageUp: 確認ダイアログ付きで前のセットへ移動
// - 境界: 最初/最後のセットでは何も起きない
// - ディレクトリ間ジャンプ: nested/sub1 → nested/sub2
//
// フィクスチャ契約:
//   archive/ に images.zip (3 JPEG) と mixed.zip (1 JPEG + 1 MP4)
//   nested/ に sub1/ (1 JPEG) と sub2/ (1 JPEG)
//   ※ root 直下は parentNodeId=null のため set-jump 不可 (仕様)

import { test, expect } from "@playwright/test";

// archive 内の images.zip で CG モードを開く
async function openCgInArchiveZip(page: import("@playwright/test").Page) {
  await page.goto("/");

  const archiveCard = page.locator("[data-testid^='mount-']", {
    hasText: "archive",
  });
  await expect(archiveCard).toBeVisible();
  await archiveCard.click();
  await expect(page).toHaveURL(/\/browse\//);

  const imagesZip = page.locator("[data-testid^='file-card-']", {
    hasText: "images.zip",
  });
  await expect(imagesZip).toBeVisible();
  await imagesZip.click();
  await expect(page).toHaveURL(/\/browse\//);

  const imagesTab = page.locator("[data-testid='tab-images']");
  await expect(imagesTab).toBeVisible();
  await imagesTab.click();

  const firstImage = page.locator("[data-testid^='file-card-']").first();
  await expect(firstImage).toBeVisible();
  await firstImage.click({ force: true });

  await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
}

// nested/sub1 で CG モードを開く (ディレクトリ間ジャンプ用)
async function openCgInNestedSub1(page: import("@playwright/test").Page) {
  await page.goto("/");

  // nested マウントポイントへ
  const nestedCard = page.locator("[data-testid^='mount-']", {
    hasText: "nested",
  });
  await expect(nestedCard).toBeVisible();
  await nestedCard.click();
  await expect(page).toHaveURL(/\/browse\//);

  // sub1 ディレクトリへ
  const sub1 = page.locator("[data-testid^='file-card-']", {
    hasText: "sub1",
  });
  await expect(sub1).toBeVisible();
  await sub1.click();
  await expect(page).toHaveURL(/\/browse\//);

  // 画像タブ
  const imagesTab = page.locator("[data-testid='tab-images']");
  await expect(imagesTab).toBeVisible();
  await imagesTab.click();

  // 画像をクリック → CG ビューワー
  const firstImage = page.locator("[data-testid^='file-card-']").first();
  await expect(firstImage).toBeVisible();
  await firstImage.click({ force: true });

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
