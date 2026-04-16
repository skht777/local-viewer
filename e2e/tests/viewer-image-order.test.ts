// ビューワー画像表示順の検証
// 仕様: ビューワー内の画像は常に名前昇順で表示する（ブラウズソート順と独立）
// - pictures フィクスチャの mtime:
//   - large.png  : 最新（alphabetical: 1 番目）
//   - photo1.jpg : 古い（alphabetical: 2 番目）
//   - photo2.jpg : 古い（alphabetical: 3 番目）
//   - photo3.jpg : 古い（alphabetical: 4 番目）
// date-asc では photo1→photo2→photo3→large の順で表示されるが、
// その中から photo1 をクリックしてビューワーを開き Home キーで先頭に移動すると、
// 名前昇順の先頭である large.png が表示されることを検証する。

import { expect, test } from "@playwright/test";
import { clickFileCard, navigateToMount } from "./helpers/navigation";

test.describe("ビューワー画像表示順", () => {
  test("date-asc ソート下でも Home キーで name-asc 先頭 (large.png) が表示される", async ({
    page,
  }) => {
    await navigateToMount(page, "pictures");

    // 画像タブ + date-asc ソートに切り替え
    await page.locator("[data-testid='tab-images']").click();
    await expect(page).toHaveURL(/tab=images/);
    // ソート切替トグルは data-testid='sort-toggle-*' 想定。URL に直接付与して安定化
    await page.goto(`${page.url()}&sort=date-asc`);
    await expect(page).toHaveURL(/sort=date-asc/);

    // date-asc では photo1.jpg が先頭に来る（同日時は name 昇順タイブレーカー）
    const firstCard = page.locator("[data-testid^='file-card-']").first();
    await expect(firstCard).toContainText(/photo1\.jpg/);

    await clickFileCard(firstCard);
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();

    // ビューワーの Home キーで先頭へ
    await page.keyboard.press("Home");
    await expect(page).toHaveURL(/index=0/);

    // 先頭には name-asc の 1 番目 large.png が表示される
    const image = page.locator("[data-testid='cg-image-area'] img").first();
    await expect(image).toHaveAttribute("alt", "large.png");
  });

  test("date-asc ソート下でビューワー内を順送りすると名前昇順の順序になる", async ({
    page,
  }) => {
    await navigateToMount(page, "pictures");
    await page.locator("[data-testid='tab-images']").click();
    await page.goto(`${page.url()}&sort=date-asc`);

    const firstCard = page.locator("[data-testid^='file-card-']").first();
    await clickFileCard(firstCard);
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();

    // index=0 → large.png（name-asc 1 番目）
    await page.keyboard.press("Home");
    const image = page.locator("[data-testid='cg-image-area'] img").first();
    await expect(image).toHaveAttribute("alt", "large.png");

    // D で次 → photo1.jpg
    await page.keyboard.press("d");
    await expect(page).toHaveURL(/index=1/);
    await expect(image).toHaveAttribute("alt", "photo1.jpg");

    // D で次 → photo2.jpg
    await page.keyboard.press("d");
    await expect(page).toHaveURL(/index=2/);
    await expect(image).toHaveAttribute("alt", "photo2.jpg");
  });

  test("ビューワーから B で閉じるとブラウズのソート順 (date-asc) は維持される", async ({
    page,
  }) => {
    await navigateToMount(page, "pictures");
    await page.locator("[data-testid='tab-images']").click();
    await page.goto(`${page.url()}&sort=date-asc`);

    await clickFileCard(page.locator("[data-testid^='file-card-']").first());
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();

    await page.keyboard.press("b");
    await expect(page.locator("[data-testid='cg-viewer']")).not.toBeVisible();
    await expect(page).toHaveURL(/sort=date-asc/);
  });
});
