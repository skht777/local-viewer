// ディレクトリツリーテスト (P1)
// DT-1: ファイルカードクリックでディレクトリ遷移
// DT-2: 遷移先で子ディレクトリが表示される

import { test, expect } from "@playwright/test";
import { navigateToMount } from "./helpers/navigation";

test.describe("ディレクトリツリー", () => {
  test("DT-1: ディレクトリカードクリックでサブディレクトリに遷移する", async ({ page }) => {
    await navigateToMount(page, "nested");
    const initialUrl = page.url();

    // sub1 ディレクトリカードをクリック
    const sub1Card = page.locator("[data-testid^='file-card-']", { hasText: "sub1" });
    await expect(sub1Card).toBeVisible();
    await sub1Card.click();

    // URL が変わる
    await expect(page).not.toHaveURL(initialUrl);
    await expect(page).toHaveURL(/\/browse\//);
  });

  test("DT-2: サブディレクトリ遷移後に画像が表示される", async ({ page }) => {
    await navigateToMount(page, "nested");

    // sub1 に遷移
    const sub1Card = page.locator("[data-testid^='file-card-']", { hasText: "sub1" });
    await expect(sub1Card).toBeVisible();
    await sub1Card.click();
    await expect(page).toHaveURL(/\/browse\//);

    // 画像タブに切り替え
    const imagesTab = page.locator("[data-testid='tab-images']");
    await expect(imagesTab).toBeVisible();
    await imagesTab.click();

    // sub1 内の画像が表示される
    const imageCard = page.locator("[data-testid^='file-card-']", { hasText: "deep" });
    await expect(imageCard).toBeVisible();
  });
});
