// ディレクトリツリーテスト
// P1: DT-1(カードクリック遷移), DT-2(遷移後画像表示)
// P2: DT-4(空ディレクトリ), DT-5(サイドバートグル)
// DT-3 は DT-2 でカバー済みのためスキップ

import { test, expect } from "@playwright/test";
import { navigateToMount } from "./helpers/navigation";

test.describe("ディレクトリツリー", () => {
  test("DT-1: ディレクトリカードクリックでサブディレクトリに遷移する", async ({ page }) => {
    await navigateToMount(page, "nested");

    // dirs サブディレクトリに入る
    const dirsCard = page.locator("[data-testid^='file-card-']", { hasText: "dirs" });
    await expect(dirsCard).toBeVisible();
    await dirsCard.dblclick();
    await expect(page).toHaveURL(/\/browse\//);

    const initialUrl = page.url();

    // sub1 ディレクトリカードをダブルクリック（C2: ダブルクリック=進入）
    const sub1Card = page.locator("[data-testid^='file-card-']", { hasText: "sub1" });
    await expect(sub1Card).toBeVisible();
    await sub1Card.dblclick();

    // URL が変わる
    await expect(page).not.toHaveURL(initialUrl);
    await expect(page).toHaveURL(/\/browse\//);
  });

  test("DT-2: サブディレクトリ遷移後に画像が表示される", async ({ page }) => {
    await navigateToMount(page, "nested");

    // dirs サブディレクトリに入る
    const dirsCard = page.locator("[data-testid^='file-card-']", { hasText: "dirs" });
    await expect(dirsCard).toBeVisible();
    await dirsCard.dblclick();
    await expect(page).toHaveURL(/\/browse\//);

    // sub1 に遷移
    const sub1Card = page.locator("[data-testid^='file-card-']", { hasText: "sub1" });
    await expect(sub1Card).toBeVisible();
    await sub1Card.dblclick();
    await expect(page).toHaveURL(/\/browse\//);

    // 画像タブに切り替え
    const imagesTab = page.locator("[data-testid='tab-images']");
    await expect(imagesTab).toBeVisible();
    await imagesTab.click();

    // sub1 内の画像が表示される
    const imageCard = page.locator("[data-testid^='file-card-']", { hasText: "deep" });
    await expect(imageCard).toBeVisible();
  });

  test("DT-4: 空ディレクトリで「ファイルがありません」が表示される", async ({ page }) => {
    await navigateToMount(page, "empty");

    // 空ディレクトリでは「ファイルがありません」テキストが表示される
    await expect(page.getByText("ファイルがありません")).toBeVisible();
  });

  test("DT-5: サイドバートグルボタンでツリーが表示/非表示になる", async ({ page }) => {
    await navigateToMount(page, "nested");

    // サイドバーはデフォルトで表示
    const sidebar = page.locator("aside");
    await expect(sidebar.first()).toBeVisible();

    // トグルボタンクリック → 非表示
    const toggleBtn = page.getByRole("button", { name: "サイドバー切替" });
    await toggleBtn.click();

    // サイドバーが非表示になることを確認
    // DirectoryTree は aside 要素で、isSidebarOpen=false で条件レンダリング除外
    await expect(sidebar).not.toBeVisible();

    // 再度クリック → 表示復帰
    await toggleBtn.click();
    await expect(sidebar.first()).toBeVisible();
  });
});
