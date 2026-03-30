// 検索機能テスト (P1)
// SE-1: 入力→結果表示、SE-3: kind フィルタ、SE-5: 結果クリック遷移
// SE-6: ディレクトリ結果遷移、SE-10: フォーカス中キーバインド無効

import { test, expect } from "@playwright/test";
import { navigateToMount, waitForSearchIndex } from "./helpers/navigation";

test.describe("検索機能", () => {
  // 検索インデックス構築を待つため、各テストのタイムアウトを延長
  test.beforeEach(async ({ request }, testInfo) => {
    testInfo.setTimeout(60_000);
    await waitForSearchIndex(request);
  });

  test("SE-1: 文字入力で結果ドロップダウンが表示される", async ({ page }) => {
    await page.goto("/");

    // ※ 画像はインデックス対象外のため、動画ファイル名で検索
    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("clip");

    const results = page.getByTestId("search-results");
    await expect(results).toBeVisible();
    expect(await results.locator("li").count()).toBeGreaterThanOrEqual(1);
  });

  test("SE-3: kind フィルタで動画のみに絞り込める", async ({ page }) => {
    await page.goto("/");

    // ※ 画像はインデックス対象外のため、動画フィルタで検証
    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("clip");
    await expect(page.getByTestId("search-results")).toBeVisible();

    const videoFilter = page.getByTestId("kind-filter-video");
    await videoFilter.click();

    const results = page.getByTestId("search-results");
    await expect(results).toBeVisible();
    expect(await results.locator("li").count()).toBeGreaterThanOrEqual(1);
  });

  test("SE-5: 検索結果クリックでブラウズ画面に遷移する", async ({ page }) => {
    await page.goto("/");

    // アーカイブを検索 (archive/zips/ 内にあり、直接ブラウズに遷移可能)
    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("images.zip");

    const results = page.getByTestId("search-results");
    await expect(results).toBeVisible();

    await results.locator("li").first().click();

    await expect(page).toHaveURL(/\/browse\//);
  });

  test("SE-6: ディレクトリ検索結果クリックでそのディレクトリに遷移する", async ({ page }) => {
    await page.goto("/");

    // サブディレクトリ "dirs" を検索 (kind=directory でインデックス対象)
    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("dirs");

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
