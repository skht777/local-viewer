// 検索機能 拡張テスト (P2/P3)
// SE-2: 2文字未満非表示、SE-4: kind All復帰、SE-7: ↓選択、SE-8: ↓+Enter遷移
// SE-9: Escape閉じ、SE-11: 0件メッセージ、SE-12: 外クリック閉じ
// SE-13: 相対パス表示、SE-14: selectパラメータでカードハイライト

import { test, expect } from "@playwright/test";
import { navigateToMount, waitForSearchIndex } from "./helpers/navigation";

test.describe("検索機能 — 拡張", () => {
  // 検索インデックス構築を待つため、各テストのタイムアウトを延長
  test.beforeEach(async ({ request }, testInfo) => {
    testInfo.setTimeout(60_000);
    await waitForSearchIndex(request);
  });

  test("SE-2: 2文字未満ではドロップダウンが表示されない", async ({ page }) => {
    await page.goto("/");

    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("p");

    // search-results が表示されないこと
    const results = page.getByTestId("search-results");
    await expect(results).not.toBeVisible();
  });

  test("SE-4: kind All フィルタで全種類に戻る", async ({ page }) => {
    await page.goto("/");

    // ※ 画像はインデックス対象外のため、動画フィルタで検証
    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("clip");
    await expect(page.getByTestId("search-results")).toBeVisible();

    // まず動画フィルタに絞る
    await page.getByTestId("kind-filter-video").click();
    const filteredCount = await page.getByTestId("search-results").locator("li").count();

    // All に戻す
    await page.getByTestId("kind-filter-all").click();
    const allCount = await page.getByTestId("search-results").locator("li").count();

    expect(allCount).toBeGreaterThanOrEqual(filteredCount);
  });

  test("SE-7: ↓キーで結果が選択される (aria-selected)", async ({ page }) => {
    await page.goto("/");

    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("clip");
    await expect(page.getByTestId("search-results")).toBeVisible();

    // ↓キーで最初の結果を選択
    await searchInput.press("ArrowDown");

    const firstResult = page.getByTestId("search-result-0");
    await expect(firstResult).toHaveAttribute("aria-selected", "true");
  });

  test("SE-8: ↓ + Enter で結果に遷移する", async ({ page }) => {
    await page.goto("/");

    // サブディレクトリ "sub1" を検索 (directory は直接遷移、parent_node_id 不要)
    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("sub1");
    await expect(page.getByTestId("search-results")).toBeVisible();

    // ↓キーで選択 → Enter で遷移
    await searchInput.press("ArrowDown");
    await searchInput.press("Enter");

    await expect(page).toHaveURL(/\/browse\//);
  });

  test("SE-9: Escape でドロップダウンが閉じる", async ({ page }) => {
    await page.goto("/");

    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("clip");
    await expect(page.getByTestId("search-results")).toBeVisible();

    // Escape で閉じる
    await searchInput.press("Escape");
    await expect(page.getByTestId("search-results")).not.toBeVisible();
  });

  test("SE-11: 0件で「結果が見つかりません」が表示される", async ({ page }) => {
    await page.goto("/");

    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("zzzznotexist");

    // 0件メッセージが表示される
    await expect(page.getByText("結果が見つかりません")).toBeVisible();
  });

  test("SE-12: ドロップダウン外クリックで閉じる", async ({ page }) => {
    await page.goto("/");

    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("clip");
    await expect(page.getByTestId("search-results")).toBeVisible();

    // 検索バー外の領域をクリック
    await page.locator("body").click({ position: { x: 10, y: 10 } });
    await expect(page.getByTestId("search-results")).not.toBeVisible();
  });

  test("SE-13: 検索結果にファイルの相対パスが表示される", async ({ page }) => {
    await page.goto("/");

    // サブディレクトリ "sub1" を検索 (directory でインデックス対象)
    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("sub1");
    await expect(page.getByTestId("search-results")).toBeVisible();

    // 相対パスに "dirs/sub1" が含まれる (mount_id プレフィックスは除外して検証)
    const results = page.getByTestId("search-results");
    await expect(results).toContainText("dirs/sub1");
  });

  test("SE-14: 検索結果クリックでブラウズ画面に遷移する", async ({ page }) => {
    await page.goto("/");

    // サブディレクトリ "sub1" を検索 (directory は直接遷移)
    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("sub1");
    await expect(page.getByTestId("search-results")).toBeVisible();

    // 結果クリックで遷移
    await page.getByTestId("search-results").locator("li").first().click();
    await expect(page).toHaveURL(/\/browse\//);

    // ブラウズ画面のタブが表示される
    await expect(page.locator("[data-testid='tab-images']")).toBeVisible();
  });
});
