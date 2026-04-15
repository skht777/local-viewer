// ▶ 開く → B で閉じる → 元のディレクトリに戻る E2E テスト
// フィクスチャ契約:
//   nested/dirs/ に sub1/ (deep.jpg) と sub2/ (wide.jpg) がある
//   「▶ 開く」(Space) で first-viewable 再帰探索 → ビューワー自動起動

import { test, expect } from "@playwright/test";
import { navigateToMount } from "./helpers/navigation";

test.describe("▶ 開く → B で閉じる → 元のディレクトリに戻る", () => {
  test("ディレクトリを開いて B で閉じると元のディレクトリに戻る", async ({ page }) => {
    await test.step("nested マウントポイントに移動", async () => {
      await navigateToMount(page, "nested");
    });

    // 元の URL を記録
    const originalUrl = page.url();

    await test.step("dirs ディレクトリを選択して Space で「▶ 開く」", async () => {
      const dirsCard = page.locator("[data-testid^='file-card-']", {
        hasText: "dirs",
      });
      await expect(dirsCard).toBeVisible();
      await dirsCard.click();
      await page.keyboard.press("Space");
    });

    await test.step("ビューワーが開くことを確認", async () => {
      await expect(page).toHaveURL(/index=/, { timeout: 10_000 });
    });

    await test.step("B キーで閉じて元のディレクトリに戻ることを確認", async () => {
      await page.keyboard.press("b");
      // ビューワーが閉じる（index= が消える）
      await expect(page).not.toHaveURL(/index=/);
      // 元の URL に戻る
      expect(page.url()).toBe(originalUrl);
    });
  });

  test("▶ 開く → セットジャンプ → B で元のディレクトリに戻る", async ({ page }) => {
    await test.step("nested マウントポイントに移動", async () => {
      await navigateToMount(page, "nested");
    });

    const originalUrl = page.url();

    await test.step("dirs ディレクトリを選択して Space で「▶ 開く」", async () => {
      const dirsCard = page.locator("[data-testid^='file-card-']", {
        hasText: "dirs",
      });
      await expect(dirsCard).toBeVisible();
      await dirsCard.click();
      await page.keyboard.press("Space");
    });

    await test.step("ビューワーが開くことを確認", async () => {
      await expect(page).toHaveURL(/index=/, { timeout: 10_000 });
    });

    // ビューワーが開いた時点の URL を記録（セットジャンプ後に変わることを確認）
    const viewerUrl = page.url();

    await test.step("PageDown でセットジャンプ", async () => {
      await page.keyboard.press("PageDown");
      // URL が変わるのを待つ（別のセットに遷移）
      await expect
        .poll(() => page.url(), { timeout: 10_000 })
        .not.toBe(viewerUrl);
    });

    await test.step("B キーで閉じて元のディレクトリに戻ることを確認", async () => {
      await page.keyboard.press("b");
      await expect(page).not.toHaveURL(/index=/);
      expect(page.url()).toBe(originalUrl);
    });
  });
});
