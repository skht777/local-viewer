// 検索機能テスト (P1)
// SE-1: 入力→結果表示、SE-3: kind フィルタ、SE-5: 結果クリック遷移
// SE-6: ディレクトリ結果遷移、SE-10: フォーカス中キーバインド無効

import { test, expect } from "@playwright/test";
import { navigateToMount, waitForSearchIndex } from "./helpers/navigation";

test.describe("検索機能", () => {
  test.beforeEach(async ({ request }) => {
    await waitForSearchIndex(request);
  });

  test("SE-1: 文字入力で結果ドロップダウンが表示される", async ({ page }) => {
    await page.goto("/");

    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("photo");

    const results = page.getByTestId("search-results");
    await expect(results).toBeVisible();
    expect(await results.locator("li").count()).toBeGreaterThanOrEqual(1);
  });

  test("SE-3: kind フィルタで画像のみに絞り込める", async ({ page }) => {
    await page.goto("/");

    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("photo");
    await expect(page.getByTestId("search-results")).toBeVisible();

    const imageFilter = page.getByTestId("kind-filter-image");
    await imageFilter.click();

    const results = page.getByTestId("search-results");
    await expect(results).toBeVisible();
    expect(await results.locator("li").count()).toBeGreaterThanOrEqual(1);
  });

  test("SE-5: 検索結果クリックで親ディレクトリに遷移する", async ({ page }) => {
    await page.goto("/");

    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("photo1");

    const results = page.getByTestId("search-results");
    await expect(results).toBeVisible();

    await results.locator("li").first().click();

    await expect(page).toHaveURL(/\/browse\//);
    await expect(page).toHaveURL(/tab=images/);
  });

  test("SE-6: ディレクトリ検索結果クリックでそのディレクトリに遷移する", async ({ page }) => {
    await page.goto("/");

    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("pictures");

    const results = page.getByTestId("search-results");
    await expect(results).toBeVisible();

    await results.locator("li").first().click();

    await expect(page).toHaveURL(/\/browse\//);
  });

  test("SE-10: 検索バーフォーカス中にビューワーキーバインドが無効", async ({ page }) => {
    // CG ビューワーでは検索バーが隠れているため、ブラウズ画面でテスト
    // 画像タブで D キーを押しても何も起きないことを確認
    await navigateToMount(page, "pictures");

    const imagesTab = page.locator("[data-testid='tab-images']");
    await expect(imagesTab).toBeVisible();
    await imagesTab.click();

    // 検索バーにフォーカス
    const searchInput = page.getByTestId("search-input");
    await searchInput.focus();
    await searchInput.fill("d");

    // URL に index が追加されていないことを確認 (ビューワーが開かない)
    const url = page.url();
    expect(url).not.toContain("index=");
    expect(url).not.toContain("mode=cg");
  });
});
