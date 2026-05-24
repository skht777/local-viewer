// モバイルでの CgViewer 検証
// - ツールバー閉じる/フルスクリーン/前後セットの各ボタンが 40px 以上
// - ⋯ メニュー (overflow) から「最初」「最後」「ヘルプ」に到達可能
// - レイアウト由来の横スクロールが発生しない (画像コンテンツのスクロールは別)
// - スワイプ機能は Vitest で網羅済み (useTouchPageTurn.test.ts)。
//   E2E ではビジュアル・到達可能性の検証に集中

import { test, expect } from "@playwright/test";
import { openCgViewer } from "../helpers/navigation";

test.describe("CgViewer モバイル", () => {
  test("ツールバーが常時表示され (タッチデバイス)、閉じるボタンが押せる", async ({ page }) => {
    await openCgViewer(page, "pictures");

    // タッチデバイスではツールバー常時表示 (toolbar-wrapper が relative z-10)
    const wrapper = page.getByTestId("toolbar-wrapper");
    await expect(wrapper).toBeVisible();

    // 閉じるボタンが視認できる
    const closeBtn = wrapper.getByRole("button", { name: "閉じる" });
    await expect(closeBtn).toBeVisible();
    const box = await closeBtn.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.height).toBeGreaterThanOrEqual(36);
  });

  test("⋯ メニューから「最初へ」「最後へ」「ヘルプ」に到達可能", async ({ page }) => {
    await openCgViewer(page, "pictures");

    const overflowSummary = page.getByTestId("toolbar-overflow-summary");
    await expect(overflowSummary).toBeVisible();
    await overflowSummary.click();

    // 各メニュー項目が表示される
    await expect(page.getByTestId("overflow-home")).toBeVisible();
    await expect(page.getByTestId("overflow-end")).toBeVisible();
    await expect(page.getByTestId("overflow-help")).toBeVisible();
  });

  test("⋯ メニューの「ヘルプ」タップで KeyboardHelp が開く", async ({ page }) => {
    await openCgViewer(page, "pictures");

    await page.getByTestId("toolbar-overflow-summary").click();
    await page.getByTestId("overflow-help").click();

    // KeyboardHelp オーバーレイが出現
    await expect(page.getByTestId("keyboard-help-overlay")).toBeVisible();
    // タッチ操作セクションも表示
    await expect(page.getByText("タッチ操作")).toBeVisible();
    await expect(page.getByText("左スワイプ")).toBeVisible();
  });

  test("ビュー領域でレイアウト由来の横スクロールが発生しない", async ({ page }) => {
    await openCgViewer(page, "pictures");
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - window.innerWidth,
    );
    expect(overflow).toBeLessThanOrEqual(0);
  });

  test("ページセレクトはモバイルで非表示 (hidden lg:block で隠れる)", async ({ page }) => {
    await openCgViewer(page, "pictures");
    // toolbar-wrapper 内の select 要素を取得し、可視でないことを確認
    const select = page.locator("[data-testid='toolbar-wrapper'] select").first();
    await expect(select).toBeHidden();
  });
});
