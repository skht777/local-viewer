// CG 画像操作テスト
// P1: CI-1(右クリック次ページ), CI-2(左クリック前ページ)
// P2: CI-3(先頭で左クリック境界), CI-4(末尾で右クリック境界)
// P3: CI-5(カーソル非表示), CI-6(カーソル復元)

import { test, expect } from "@playwright/test";
import { openCgViewer } from "./helpers/navigation";

test.describe("CG 画像クリックナビゲーション", () => {
  test("CI-1: 画像右半分クリックで次ページに進む", async ({ page }) => {
    await openCgViewer(page);
    await expect(page).toHaveURL(/index=0/);

    // cg-image-area の右半分をクリック
    const imageArea = page.getByTestId("cg-image-area");
    const box = await imageArea.boundingBox();
    if (!box) throw new Error("cg-image-area が見つかりません");

    await page.mouse.click(box.x + box.width * 0.75, box.y + box.height / 2);
    await expect(page).toHaveURL(/index=1/);
  });

  test("CI-2: 画像左半分クリックで前ページに戻る", async ({ page }) => {
    await openCgViewer(page);

    // まず D キーで index=1 に進める
    await page.keyboard.press("d");
    await expect(page).toHaveURL(/index=1/);

    // cg-image-area の左半分をクリック
    const imageArea = page.getByTestId("cg-image-area");
    const box = await imageArea.boundingBox();
    if (!box) throw new Error("cg-image-area が見つかりません");

    await page.mouse.click(box.x + box.width * 0.25, box.y + box.height / 2);
    await expect(page).toHaveURL(/index=0/);
  });

  test("CI-3: 先頭ページで左クリックしても index=0 のまま", async ({ page }) => {
    await openCgViewer(page);
    await expect(page).toHaveURL(/index=0/);

    // 左半分をクリック — 先頭なので動かないはず
    const imageArea = page.getByTestId("cg-image-area");
    const box = await imageArea.boundingBox();
    if (!box) throw new Error("cg-image-area が見つかりません");

    await page.mouse.click(box.x + box.width * 0.25, box.y + box.height / 2);
    await expect(page).toHaveURL(/index=0/);
  });

  test("CI-4: 末尾ページで右クリックしても最大 index を維持する", async ({ page }) => {
    await openCgViewer(page);

    // End キーで末尾に移動
    await page.keyboard.press("End");
    const urlAfterEnd = page.url();
    const match = urlAfterEnd.match(/index=(\d+)/);
    if (!match) throw new Error("URL に index が見つかりません");
    const lastIndex = match[1];

    // 右半分をクリック — 末尾なので動かないはず
    const imageArea = page.getByTestId("cg-image-area");
    const box = await imageArea.boundingBox();
    if (!box) throw new Error("cg-image-area が見つかりません");

    await page.mouse.click(box.x + box.width * 0.75, box.y + box.height / 2);
    await expect(page).toHaveURL(new RegExp(`index=${lastIndex}`));
  });
});

test.describe("CG カーソルオートハイド", () => {
  test("CI-5: 1秒アイドルでカーソルが非表示になる", async ({ page }) => {
    await openCgViewer(page);
    const imageArea = page.getByTestId("cg-image-area");

    // マウスを画像エリア上に移動してからアイドル待ち
    const box = await imageArea.boundingBox();
    if (!box) throw new Error("cg-image-area が見つかりません");
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);

    // 1秒アイドル後に cursor: none になるのを expect.poll で待機
    await expect.poll(
      () => imageArea.evaluate((el) => getComputedStyle(el).cursor),
      { timeout: 5_000, message: "1秒アイドル後に cursor が none になるはず" },
    ).toBe("none");
  });

  test("CI-6: マウス移動でカーソルが復元する", async ({ page }) => {
    await openCgViewer(page);
    const imageArea = page.getByTestId("cg-image-area");

    // まずマウスを画像エリアに移動
    const box = await imageArea.boundingBox();
    if (!box) throw new Error("cg-image-area が見つかりません");
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);

    // アイドル → cursor: none を待つ
    await expect.poll(
      () => imageArea.evaluate((el) => getComputedStyle(el).cursor),
      { timeout: 5_000, message: "cursor が none になるのを待つ" },
    ).toBe("none");

    // マウスを少し移動 → カーソル復元
    await page.mouse.move(box.x + box.width / 2 + 10, box.y + box.height / 2 + 10);

    await expect.poll(
      () => imageArea.evaluate((el) => getComputedStyle(el).cursor),
      { timeout: 3_000, message: "マウス移動後に cursor が none でなくなるはず" },
    ).not.toBe("none");
  });
});
