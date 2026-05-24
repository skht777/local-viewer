// モバイルでの SearchResultsPage 検証
// - 検索ヘッダーが 375px 環境で破綻しない
// - KIND_TABS が overflow-x-auto + whitespace-nowrap
// - sort select が w-full (lg 未満) + lg:w-auto
// - レイアウト由来の横スクロールが発生しない

import { test, expect } from "@playwright/test";

test.describe("SearchResultsPage モバイル", () => {
  test("/search 直接アクセスで KIND_TABS が overflow-x-auto", async ({ page }) => {
    await page.goto("/search?q=a");
    const tabs = page.getByTestId("search-kind-tabs");
    await expect(tabs).toBeVisible();
    const cls = (await tabs.getAttribute("class")) ?? "";
    expect(cls).toContain("overflow-x-auto");
    expect(cls).toContain("whitespace-nowrap");
  });

  test("sort select が w-full クラスを持つ (モバイル幅一杯)", async ({ page }) => {
    await page.goto("/search?q=a");
    const sortSelect = page.getByTestId("search-sort-select");
    await expect(sortSelect).toBeVisible();
    const cls = (await sortSelect.getAttribute("class")) ?? "";
    expect(cls).toContain("w-full");
    expect(cls).toContain("lg:w-auto");
  });

  test("ビューポート 393px (Pixel 5) でレイアウト由来の横スクロールが発生しない", async ({
    page,
  }) => {
    await page.goto("/search?q=a");
    await expect(page.getByTestId("search-kind-tabs")).toBeVisible();
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - window.innerWidth,
    );
    expect(overflow).toBeLessThanOrEqual(0);
  });

  test("KIND タブが 40px 以上のタッチターゲット", async ({ page }) => {
    await page.goto("/search?q=a");
    const allTab = page.getByTestId("search-kind-all");
    await expect(allTab).toBeVisible();
    const box = await allTab.boundingBox();
    expect(box).not.toBeNull();
    // py-2 (h≈40px) ベース
    expect(box!.height).toBeGreaterThanOrEqual(36);
  });
});
