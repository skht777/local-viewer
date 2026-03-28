// 検索機能 拡張テスト (P2)
// SE-2: 2文字未満非表示、SE-4: kind All復帰、SE-7: ↓選択、SE-8: ↓+Enter遷移
// SE-9: Escape閉じ、SE-11: 0件メッセージ、SE-12: 外クリック閉じ
// SE-14: selectパラメータでカードハイライト

import { test, expect } from "@playwright/test";
import { navigateToMount, waitForSearchIndex } from "./helpers/navigation";

test.describe("検索機能 — 拡張", () => {
  test.beforeEach(async ({ request }) => {
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

    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("photo");
    await expect(page.getByTestId("search-results")).toBeVisible();

    // まず画像フィルタに絞る
    await page.getByTestId("kind-filter-image").click();
    const filteredCount = await page.getByTestId("search-results").locator("li").count();

    // All に戻す
    await page.getByTestId("kind-filter-all").click();
    const allCount = await page.getByTestId("search-results").locator("li").count();

    expect(allCount).toBeGreaterThanOrEqual(filteredCount);
  });

  test("SE-7: ↓キーで結果が選択される (aria-selected)", async ({ page }) => {
    await page.goto("/");

    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("photo");
    await expect(page.getByTestId("search-results")).toBeVisible();

    // ↓キーで最初の結果を選択
    await searchInput.press("ArrowDown");

    const firstResult = page.getByTestId("search-result-0");
    await expect(firstResult).toHaveAttribute("aria-selected", "true");
  });

  test("SE-8: ↓ + Enter で結果に遷移する", async ({ page }) => {
    await page.goto("/");

    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("photo1");
    await expect(page.getByTestId("search-results")).toBeVisible();

    // ↓キーで選択 → Enter で遷移
    await searchInput.press("ArrowDown");
    await searchInput.press("Enter");

    await expect(page).toHaveURL(/\/browse\//);
  });

  test("SE-9: Escape でドロップダウンが閉じる", async ({ page }) => {
    await page.goto("/");

    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("photo");
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
    await searchInput.fill("photo");
    await expect(page.getByTestId("search-results")).toBeVisible();

    // 検索バー外の領域をクリック
    await page.locator("body").click({ position: { x: 10, y: 10 } });
    await expect(page.getByTestId("search-results")).not.toBeVisible();
  });

  // SE-14: select パラメータでカードハイライト — FileCard に aria-current 未実装
  test.fixme("SE-14: select パラメータで FileBrowser カードがハイライトされる", async ({ page }) => {
    await page.goto("/");

    const searchInput = page.getByTestId("search-input");
    await searchInput.fill("photo1");
    await expect(page.getByTestId("search-results")).toBeVisible();

    // 結果クリックで遷移
    await page.getByTestId("search-results").locator("li").first().click();
    await expect(page).toHaveURL(/\/browse\//);
    await expect(page).toHaveURL(/select=/);

    // 対象 file-card に aria-current が設定される
    const selectMatch = page.url().match(/select=([^&]+)/);
    if (!selectMatch) throw new Error("URL に select パラメータが見つかりません");
    const selectedId = selectMatch[1];

    const card = page.getByTestId(`file-card-${selectedId}`);
    await expect(card).toHaveAttribute("aria-current", "true");
  });
});
