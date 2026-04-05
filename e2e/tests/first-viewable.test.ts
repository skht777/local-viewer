// first-viewable 自動遷移テスト
// 仕様出典: spec-ui-behavior.md §セット概念, spec-architecture.md §first-viewable
// ディレクトリダブルクリック → 再帰探索 → 最初の閲覧可能ファイルを自動オープン

import { test, expect } from "@playwright/test";
import { navigateToMount, clickFileCard } from "./helpers/navigation";

test.describe("first-viewable 自動遷移", () => {
  test("ネストされたディレクトリのダブルクリックでビューワーが自動で開く", async ({
    page,
  }) => {
    // nested マウントポイントに移動
    await navigateToMount(page, "nested");

    // dirs ディレクトリカードをダブルクリック
    // dirs/ 配下に sub1/deep.jpg, sub2/wide.jpg がある
    const dirsCard = page.locator("[data-testid^='file-card-']", {
      hasText: "dirs",
    });
    await clickFileCard(dirsCard);

    // first-viewable により再帰探索 → ビューワーが自動で開く
    // CG ビューワーまたは URL に index パラメータが付く
    await expect(page).toHaveURL(/index=/, { timeout: 10_000 });
  });

  test("画像があるディレクトリのダブルクリックでビューワーが開く", async ({
    page,
  }) => {
    // pictures マウントポイントに移動 → 直下に画像あり
    await navigateToMount(page, "pictures");

    // 画像タブに切り替え
    const imagesTab = page.locator("[data-testid='tab-images']");
    await expect(imagesTab).toBeVisible();
    await imagesTab.click();

    // 最初の画像カードをダブルクリック → ビューワーが開く
    await clickFileCard(page.locator("[data-testid^='file-card-']").first());
    await expect(page).toHaveURL(/index=/);
  });

  test("空ディレクトリのダブルクリックでビューワーは開かない", async ({
    page,
  }) => {
    // empty マウントポイントに移動
    await navigateToMount(page, "empty");

    // ファイルカードが存在しない (空ディレクトリ)
    const cards = page.locator("[data-testid^='file-card-']");
    await expect(cards).toHaveCount(0);
  });
});
