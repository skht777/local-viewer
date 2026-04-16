// ビューワー画像表示順の検証
// 仕様: ビューワー内の画像は常に名前昇順で表示する（ブラウズソート順と独立）
// - pictures フィクスチャは large.png, photo1.jpg, photo2.jpg, photo3.jpg
// - 名前昇順: large.png → photo1.jpg → photo2.jpg → photo3.jpg
// - ブラウズは date-desc などのソートを当てても、ビューワーを Home キーで
//   先頭に戻せば常に large.png（name-asc 1 番目）が表示される。
// - CI 環境では全フィクスチャの mtime が揃うため、「どのカードを先頭に
//   クリックするか」はソートに依存せず可変でよい。ビューワー起動後に
//   Home キーで index=0 に正規化してから判定する。

import { expect, test } from "@playwright/test";
import { clickFileCard, navigateToMount } from "./helpers/navigation";

test.describe("ビューワー画像表示順", () => {
  test("ブラウズのソートに関わらず Home キーで name-asc 先頭 (large.png) が表示される", async ({
    page,
  }) => {
    await navigateToMount(page, "pictures");

    // 画像タブ + date-desc ソート（name-asc と異なる並びになる可能性があるソート）
    await page.locator("[data-testid='tab-images']").click();
    await expect(page).toHaveURL(/tab=images/);
    await page.goto(`${page.url()}&sort=date-desc`);
    await expect(page).toHaveURL(/sort=date-desc/);

    // どのカードを開いてもよい。ビューワー起動後に Home で先頭に正規化する
    await clickFileCard(page.locator("[data-testid^='file-card-']").first());
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();

    await page.keyboard.press("Home");
    await expect(page).toHaveURL(/index=0/);

    // 先頭は name-asc の 1 番目 large.png
    const image = page.locator("[data-testid='cg-image-area'] img").first();
    await expect(image).toHaveAttribute("alt", "large.png");
  });

  test("ビューワー内を順送りすると名前昇順の順序になる", async ({ page }) => {
    await navigateToMount(page, "pictures");
    await page.locator("[data-testid='tab-images']").click();
    await page.goto(`${page.url()}&sort=date-desc`);

    await clickFileCard(page.locator("[data-testid^='file-card-']").first());
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();

    // Home で name-asc の先頭 (large.png) に正規化
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

  test("ビューワーから B で閉じるとブラウズのソート順 (date-desc) は維持される", async ({
    page,
  }) => {
    await navigateToMount(page, "pictures");
    await page.locator("[data-testid='tab-images']").click();
    await page.goto(`${page.url()}&sort=date-desc`);

    await clickFileCard(page.locator("[data-testid^='file-card-']").first());
    await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();

    await page.keyboard.press("b");
    await expect(page.locator("[data-testid='cg-viewer']")).not.toBeVisible();
    await expect(page).toHaveURL(/sort=date-desc/);
  });
});
