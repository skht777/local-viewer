// パンくずナビゲーションテスト
// 仕様出典: spec-ui-behavior.md §ブラウズページ
// パンくずクリック → 上位ディレクトリ遷移

import { test, expect } from "@playwright/test";
import { navigateToMount, clickFileCard } from "./helpers/navigation";

test.describe("パンくずナビゲーション", () => {
  test("ネストされたディレクトリで祖先パンくずが表示される", async ({
    page,
  }) => {
    // nested マウントポイントに移動
    await navigateToMount(page, "nested");

    // dirs ディレクトリに進入
    const dirsCard = page.locator("[data-testid^='file-card-']", {
      hasText: "dirs",
    });
    await clickFileCard(dirsCard);

    // URL が更新される
    await expect(page).toHaveURL(/\/browse\//);

    // パンくずにルート（nested）が表示される
    const breadcrumbNav = page.locator("nav").filter({ hasText: "nested" });
    await expect(breadcrumbNav).toBeVisible();
  });

  test("パンくずクリックで上位ディレクトリに遷移する", async ({ page }) => {
    // nested マウントポイントに移動
    await navigateToMount(page, "nested");

    // dirs ディレクトリに進入
    const dirsCard = page.locator("[data-testid^='file-card-']", {
      hasText: "dirs",
    });
    await clickFileCard(dirsCard);
    await expect(page).toHaveURL(/\/browse\//);

    // 現在の URL を記録
    const deepUrl = page.url();

    // パンくずのルート要素をクリック (最初のボタン = マウントルート)
    const breadcrumbButton = page
      .locator("nav button")
      .filter({ hasText: "nested" });
    await expect(breadcrumbButton).toBeVisible();
    await breadcrumbButton.click();

    // URL が変わる (上位ディレクトリに遷移)
    await expect(page).not.toHaveURL(deepUrl);
    await expect(page).toHaveURL(/\/browse\//);
  });
});
