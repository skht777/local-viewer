// マンガモード操作テスト
// P1: MC-1(+ズーム), MC-2(-ズーム), MC-3(0リセット), MC-6(Sスクロール)
// P2: MC-4(ズームスライダー), MC-5(+/-ボタン), MC-7(Home先頭),
//     MC-8(End末尾), MC-10(ページセレクト), MC-11(速度効果), MC-12(ズーム位置保持)
// P3: MC-9(スクロール速度スライダー)

import { test, expect } from "@playwright/test";
import { openMangaViewer } from "./helpers/navigation";

test.describe("マンガモード — キーバインド", () => {
  test("MC-1: + キーでズームインする", async ({ page }) => {
    await openMangaViewer(page);

    await page.keyboard.press("Equal"); // = キー (+ のバインド)

    const zoomLevel = page.getByTestId("manga-zoom-level");
    await expect(zoomLevel).toHaveText("125%");
  });

  test("MC-2: - キーでズームアウトする", async ({ page }) => {
    await openMangaViewer(page);

    await page.keyboard.press("Minus");

    const zoomLevel = page.getByTestId("manga-zoom-level");
    await expect(zoomLevel).toHaveText("75%");
  });

  test("MC-3: 0 キーでズームリセットする", async ({ page }) => {
    await openMangaViewer(page);

    // まずズーム変更
    await page.keyboard.press("Equal");
    await expect(page.getByTestId("manga-zoom-level")).toHaveText("125%");

    // 0 でリセット
    await page.keyboard.press("Digit0");
    await expect(page.getByTestId("manga-zoom-level")).toHaveText("100%");
  });

  test("MC-6: S キーで下にスクロールする", async ({ page }) => {
    await openMangaViewer(page);

    const scrollArea = page.getByTestId("manga-scroll-area");
    const initialScroll = await scrollArea.evaluate((el) => el.scrollTop);

    await page.keyboard.press("s");

    await expect.poll(
      () => scrollArea.evaluate((el) => el.scrollTop),
      { message: "S キーで scrollTop が増加するはず" },
    ).toBeGreaterThan(initialScroll);
  });
});

test.describe("マンガモード — ツールバー操作", () => {
  test("MC-4: ズームスライダー操作で manga-zoom-level が更新される", async ({ page }) => {
    await openMangaViewer(page);

    const zoomSlider = page.getByRole("slider", { name: "ズーム" });
    await expect(zoomSlider).toBeVisible();

    // スライダーを 150 に変更
    await zoomSlider.fill("150");

    const zoomLevel = page.getByTestId("manga-zoom-level");
    await expect(zoomLevel).toHaveText("150%");
  });

  test("MC-5: +/- ボタンクリックでズームが 25% 変化する", async ({ page }) => {
    await openMangaViewer(page);
    await expect(page.getByTestId("manga-zoom-level")).toHaveText("100%");

    // + ボタンクリック → 125%
    const zoomInBtn = page.getByRole("button", { name: "ズームイン" });
    await zoomInBtn.click();
    await expect(page.getByTestId("manga-zoom-level")).toHaveText("125%");

    // - ボタンクリック → 100%
    const zoomOutBtn = page.getByRole("button", { name: "ズームアウト" });
    await zoomOutBtn.click();
    await expect(page.getByTestId("manga-zoom-level")).toHaveText("100%");
  });
});

test.describe("マンガモード — ナビゲーション", () => {
  test("MC-7: Home キーで先頭にスクロールする", async ({ page }) => {
    await openMangaViewer(page);
    const scrollArea = page.getByTestId("manga-scroll-area");

    // まず S キーでスクロール
    await page.keyboard.press("s");
    await page.keyboard.press("s");
    await page.keyboard.press("s");
    await expect.poll(
      () => scrollArea.evaluate((el) => el.scrollTop),
    ).toBeGreaterThan(0);

    // Home で先頭へ
    await page.keyboard.press("Home");
    await expect.poll(
      () => scrollArea.evaluate((el) => el.scrollTop),
      { message: "Home キーで scrollTop が 0 になるはず" },
    ).toBe(0);
  });

  test("MC-8: End キーで末尾にスクロールする", async ({ page }) => {
    await openMangaViewer(page);
    const scrollArea = page.getByTestId("manga-scroll-area");

    await page.keyboard.press("End");

    // scrollTop + clientHeight >= scrollHeight (末尾到達)
    await expect.poll(
      () =>
        scrollArea.evaluate((el) => {
          return el.scrollTop + el.clientHeight >= el.scrollHeight - 10;
        }),
      { message: "End キーで末尾にスクロールするはず" },
    ).toBe(true);
  });

  test("MC-10: ページセレクトで対象画像付近にスクロールする", async ({ page }) => {
    await openMangaViewer(page);
    const scrollArea = page.getByTestId("manga-scroll-area");

    // 初期スクロール位置を記録
    const initialScroll = await scrollArea.evaluate((el) => el.scrollTop);

    // ページセレクトで Page 3 (value=2) を選択
    const pageSelect = page.getByTestId("manga-viewer").locator("select");
    await pageSelect.selectOption("2");

    // スクロール位置が変化する
    await expect.poll(
      () => scrollArea.evaluate((el) => el.scrollTop),
      { message: "ページセレクトでスクロール位置が変化するはず" },
    ).not.toBe(initialScroll);
  });
});

test.describe("マンガモード — 速度スライダー", () => {
  test("MC-9: スクロール速度スライダー操作で manga-scroll-speed-label が更新される", async ({ page }) => {
    await openMangaViewer(page);

    const speedSlider = page.getByRole("slider", { name: "スクロール速度" });
    await expect(speedSlider).toBeVisible();

    // スライダーを 2.0 に変更
    await speedSlider.fill("2");

    const speedLabel = page.getByTestId("manga-scroll-speed-label");
    await expect(speedLabel).toHaveText("2x");
  });
});

test.describe("マンガモード — 速度・ズーム効果", () => {
  // スライダー操作後はフォーカスを外して hotkeys を有効化する
  test("MC-11: 速度変更が S キーのスクロール量に反映される", async ({ page }) => {
    await openMangaViewer(page);
    const scrollArea = page.getByTestId("manga-scroll-area");

    // デフォルト速度 (1.0x) で S キー → スクロール量計測
    await page.keyboard.press("s");
    await expect.poll(
      () => scrollArea.evaluate((el) => el.scrollTop),
    ).toBeGreaterThan(0);
    const defaultScroll = await scrollArea.evaluate((el) => el.scrollTop);

    // Home で先頭に戻す
    await page.keyboard.press("Home");
    await expect.poll(
      () => scrollArea.evaluate((el) => el.scrollTop),
    ).toBe(0);

    // 速度を 3.0x に変更
    const speedSlider = page.getByRole("slider", { name: "スクロール速度" });
    await speedSlider.fill("3");
    await expect(page.getByTestId("manga-scroll-speed-label")).toHaveText("3x");

    // スライダーからフォーカスを外して hotkeys を有効化
    // (react-hotkeys-hook は enableOnFormTags: false がデフォルト)
    await scrollArea.click();

    // 3.0x で S キー → スクロール量がデフォルトより大きい
    await page.keyboard.press("s");
    await expect.poll(
      () => scrollArea.evaluate((el) => el.scrollTop),
      { message: "3.0x のスクロール量がデフォルトより大きいはず" },
    ).toBeGreaterThan(defaultScroll);
  });

  test("MC-12: ズーム変更時にスクロール位置が保持される", async ({ page }) => {
    await openMangaViewer(page);
    const scrollArea = page.getByTestId("manga-scroll-area");

    // 3番目の画像付近にスクロール
    const pageSelect = page.getByTestId("manga-viewer").locator("select");
    await pageSelect.selectOption("2");

    // スクロール位置が変化するまで待つ
    await expect.poll(
      () => scrollArea.evaluate((el) => el.scrollTop),
    ).toBeGreaterThan(0);

    // ズーム変更 (+ キーで 125%)
    await page.keyboard.press("Equal");
    await expect(page.getByTestId("manga-zoom-level")).toHaveText("125%");

    // ズーム後もスクロール位置が 0 に戻っていないこと
    await expect.poll(
      () => scrollArea.evaluate((el) => el.scrollTop),
      { message: "ズーム変更後もスクロール位置が保持されるはず" },
    ).toBeGreaterThan(0);
  });
});
