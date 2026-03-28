// URL SSOT（Single Source of Truth）テスト
// 仕様出典: plan-phase2.md §ルーティング, plan-phase6.md §URL設計

import { test, expect } from "@playwright/test";

test.describe("URL SSOT", () => {
  test("ビューワーを閉じると index と mode が URL から削除される", async ({
    page,
  }) => {
    await page.goto("/");

    // pictures マウントポイントカードをクリック
    const picturesMount = page.locator("[data-testid^='mount-']", {
      hasText: "pictures",
    });
    await expect(picturesMount).toBeVisible();
    await picturesMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    // 画像タブに切り替え
    const imagesTab = page.locator("[data-testid='tab-images']");
    await expect(imagesTab).toBeVisible();
    await imagesTab.click();

    // 画像カードが安定するまで待つ
    const firstImage = page.locator("[data-testid^='file-card-']").first();
    await expect(firstImage).toBeVisible();

    // サムネイル読み込みによるDOM再構築を待つ
    await firstImage.click({ force: true });
    await expect(page).toHaveURL(/index=0/);
    await expect(page).toHaveURL(/mode=cg/);

    // ビューワーを閉じる（Escape キー）
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.locator("[data-testid='cg-viewer']")).not.toBeVisible({ timeout: 10_000 });
    await expect(page).not.toHaveURL(/index=/);
    await expect(page).not.toHaveURL(/mode=/);
  });

  test("タブ切り替えで URL の tab パラメータが更新される", async ({
    page,
  }) => {
    await page.goto("/");

    // pictures マウントポイントカードをクリック
    const picturesMount = page.locator("[data-testid^='mount-']", {
      hasText: "pictures",
    });
    await expect(picturesMount).toBeVisible();
    await picturesMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    const imagesTab = page.locator("[data-testid='tab-images']");
    if (await imagesTab.isVisible()) {
      await imagesTab.click();
      await expect(page).toHaveURL(/tab=images/);
    }

    const filesetsTab = page.locator("[data-testid='tab-filesets']");
    if (await filesetsTab.isVisible()) {
      await filesetsTab.click();
      await expect(page).toHaveURL(/tab=filesets/);
    }
  });

  test("ページリロードでビューワー状態が復元される", async ({ page }) => {
    await page.goto("/");

    // pictures マウントポイントカードをクリック
    const picturesMount = page.locator("[data-testid^='mount-']", {
      hasText: "pictures",
    });
    await expect(picturesMount).toBeVisible();
    await picturesMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    // 画像タブに切り替え
    const imagesTab = page.locator("[data-testid='tab-images']");
    await expect(imagesTab).toBeVisible();
    await imagesTab.click();

    // 画像カードが安定するまで待つ
    const firstImage = page.locator("[data-testid^='file-card-']").first();
    await expect(firstImage).toBeVisible();

    // CG モードで 2 ページ目を開く
    await firstImage.click({ force: true });
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
    await page.keyboard.press("d");
    await expect(page).toHaveURL(/index=1/);

    // リロード
    const currentUrl = page.url();
    await page.reload();

    // 同じビューワー状態が復元される
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
    await expect(page).toHaveURL(/index=1/);
    await expect(page).toHaveURL(/mode=cg/);
  });
});
