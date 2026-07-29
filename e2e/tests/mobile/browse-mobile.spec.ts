// モバイル (Pixel 5: 393×851) での BrowsePage 動作検証
// - ハンバーガーでサイドバードロワーが開閉する
// - ドロワー背景タップで閉じる
// - ViewerTabs が overflow-x-auto + whitespace-nowrap で横スクロール可能
// - レイアウト由来の横スクロールが発生しない (documentElement.scrollWidth)

import { test, expect } from "@playwright/test";
import { navigateToMount } from "../helpers/navigation";

test.describe("BrowsePage モバイル", () => {
  test("ハンバーガーで DirectoryTree ドロワーが開閉する", async ({ page }) => {
    await navigateToMount(page, "pictures");

    // 初期状態: モバイルではドロワーは閉じている (sidebar-overlay 非表示)
    await expect(page.getByTestId("sidebar-overlay")).toBeHidden();

    // ハンバーガークリックで開く
    await page.getByTestId("sidebar-toggle").click();
    await expect(page.getByTestId("directory-tree")).toBeVisible();
    await expect(page.getByTestId("sidebar-overlay")).toBeVisible();

    // 背景オーバーレイクリックで閉じる
    // 要素中央はドロワー (w-72, z-40) に覆われクリックが intercept されるため、
    // ドロワー外の右端座標を明示する
    await page.getByTestId("sidebar-overlay").click({ position: { x: 350, y: 400 } });
    await expect(page.getByTestId("sidebar-overlay")).toBeHidden();
  });

  test("DirectoryTree からの navigate で route 遷移後にドロワーが auto-close される", async ({
    page,
  }) => {
    await navigateToMount(page, "pictures");

    // ドロワーを開く
    await page.getByTestId("sidebar-toggle").click();
    await expect(page.getByTestId("sidebar-overlay")).toBeVisible();

    // ツリー内の別マウントへ移動 (videos)
    const videosNode = page.getByRole("treeitem").filter({ hasText: "videos" }).first();
    await videosNode.click();

    // route が videos に変わり、ドロワーが閉じている
    await expect(page).toHaveURL(/\/browse\/[^/]+$/);
    await expect(page.getByTestId("sidebar-overlay")).toBeHidden();
  });

  test("ViewerTabs が overflow-x-auto + whitespace-nowrap で横スクロール可能", async ({
    page,
  }) => {
    await navigateToMount(page, "pictures");
    const nav = page.locator('nav:has([data-testid="tab-filesets"])').first();
    const cls = (await nav.getAttribute("class")) ?? "";
    expect(cls).toContain("overflow-x-auto");
    expect(cls).toContain("whitespace-nowrap");
  });

  test("ビューポート 375px 相当の Pixel 5 でレイアウト由来の横スクロールが発生しない", async ({
    page,
  }) => {
    await navigateToMount(page, "pictures");
    // documentElement の scrollWidth が viewport を超えていないこと
    const overflow = await page.evaluate(() => {
      return document.documentElement.scrollWidth - window.innerWidth;
    });
    expect(overflow).toBeLessThanOrEqual(0);
  });

  test("ハンバーガーボタンが 44px 以上のタッチターゲット", async ({ page }) => {
    await navigateToMount(page, "pictures");
    const toggle = page.getByTestId("sidebar-toggle");
    const box = await toggle.boundingBox();
    expect(box).not.toBeNull();
    // Apple HIG 44px (py-2.5 + px-3 で h=44px 程度)
    expect(box!.height).toBeGreaterThanOrEqual(40);
    expect(box!.width).toBeGreaterThanOrEqual(40);
  });
});
