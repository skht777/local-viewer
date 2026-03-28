// モード切替トグルテスト
// ブラウズツールバーの CG/マンガ切替トグルの動作を検証する

import { test, expect } from "@playwright/test";
import { navigateToMount } from "./helpers/navigation";

test.describe("モード切替トグル", () => {
  test("MT-1: トグルクリックで URL と aria-pressed が更新される", async ({ page }) => {
    await navigateToMount(page, "pictures");

    // デフォルトは CG がアクティブ
    const cgBtn = page.getByTestId("mode-toggle-cg");
    const mangaBtn = page.getByTestId("mode-toggle-manga");
    await expect(cgBtn).toHaveAttribute("aria-pressed", "true");
    await expect(mangaBtn).toHaveAttribute("aria-pressed", "false");
    await expect(page).not.toHaveURL(/mode=/);

    // マンガを選択 → URL に mode=manga
    await mangaBtn.click();
    await expect(page).toHaveURL(/mode=manga/);
    await expect(mangaBtn).toHaveAttribute("aria-pressed", "true");
    await expect(cgBtn).toHaveAttribute("aria-pressed", "false");

    // CG に戻す → mode= が URL から消える
    await cgBtn.click();
    await expect(page).not.toHaveURL(/mode=/);
    await expect(cgBtn).toHaveAttribute("aria-pressed", "true");
  });

  test("MT-2: マンガ選択後に画像を開くとマンガビューワーが表示される", async ({ page }) => {
    await navigateToMount(page, "pictures");

    // マンガモードを選択
    await page.getByTestId("mode-toggle-manga").click();
    await expect(page).toHaveURL(/mode=manga/);

    // 画像タブに切り替えて画像クリック
    await page.getByTestId("tab-images").click();
    const firstImage = page.locator("[data-testid^='file-card-']").first();
    await expect(firstImage).toBeVisible();
    await firstImage.click({ force: true });

    // マンガビューワーが開く（CG ではない）
    await expect(page.getByTestId("manga-viewer")).toBeVisible();
    await expect(page).toHaveURL(/mode=manga/);
  });

  test("MT-3: マンガ選択後に PDF を開くと PDF マンガビューワーが表示される", async ({ page }) => {
    await navigateToMount(page, "docs");

    // マンガモードを選択
    await page.getByTestId("mode-toggle-manga").click();

    // PDF をクリック
    const pdfCard = page.locator("[data-testid^='file-card-']", { hasText: "sample.pdf" });
    await expect(pdfCard).toBeVisible();
    await pdfCard.click();

    // PDF マンガビューワーが開く
    await expect(page.getByTestId("pdf-manga-viewer")).toBeVisible();
  });

  test("MT-4: ビューワーを閉じても mode が URL に残る", async ({ page }) => {
    await navigateToMount(page, "pictures");

    // マンガモードを選択してビューワーを開く
    await page.getByTestId("mode-toggle-manga").click();
    await page.getByTestId("tab-images").click();
    const firstImage = page.locator("[data-testid^='file-card-']").first();
    await expect(firstImage).toBeVisible();
    await firstImage.click({ force: true });
    await expect(page.getByTestId("manga-viewer")).toBeVisible();

    // Escape で閉じる
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("manga-viewer")).not.toBeVisible();

    // mode=manga が URL に残っている
    await expect(page).toHaveURL(/mode=manga/);
    // トグルもマンガがアクティブ
    await expect(page.getByTestId("mode-toggle-manga")).toHaveAttribute("aria-pressed", "true");
  });

  test("MT-5: ディレクトリ遷移で mode が保持される", async ({ page }) => {
    // nested マウントにはサブディレクトリ (sub1, sub2) がある
    await navigateToMount(page, "nested");

    // マンガモードを選択
    await page.getByTestId("mode-toggle-manga").click();
    await expect(page).toHaveURL(/mode=manga/);

    // サブディレクトリに遷移（ファイルセットタブでディレクトリをクリック）
    const dirCard = page.locator("[data-testid^='file-card-']").first();
    await expect(dirCard).toBeVisible();
    await dirCard.click();

    // mode=manga が保持されている
    await expect(page).toHaveURL(/mode=manga/);
    await expect(page.getByTestId("mode-toggle-manga")).toHaveAttribute("aria-pressed", "true");
  });
});
