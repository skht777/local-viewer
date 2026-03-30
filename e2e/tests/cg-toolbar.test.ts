// CG ツールバー操作テスト
// P1: CT-1(V幅), CT-2(H高さ), CT-5(Q見開き)
// P2: CT-3(Wボタン幅), CT-4(Hボタン高さ), CT-6(見開きボタン), CT-7(ページセレクト), CT-8(閉じる)

import { test, expect } from "@playwright/test";
import { openCgViewer, showToolbar } from "./helpers/navigation";

test.describe("CG ツールバー — キーバインド", () => {
  test("CT-1: V キーで幅フィットに切り替わる", async ({ page }) => {
    await openCgViewer(page);

    await page.keyboard.press("v");

    // キーボード操作後にツールバーを表示して状態確認
    await showToolbar(page);
    const wBtn = page.getByRole("button", { name: "幅フィット" });
    await expect(wBtn).toHaveAttribute("aria-pressed", "true");
  });

  test("CT-2: H キーで高さフィットに切り替わる", async ({ page }) => {
    await openCgViewer(page);

    await page.keyboard.press("h");

    await showToolbar(page);
    const hBtn = page.getByRole("button", { name: "高さフィット" });
    await expect(hBtn).toHaveAttribute("aria-pressed", "true");
  });

  test("CT-5: Q キーで見開きモードがサイクルする", async ({ page }) => {
    await openCgViewer(page);
    await showToolbar(page);
    const spreadBtn = page.getByTestId("cg-spread-btn");

    // single → spread → spread-offset → single
    await expect(spreadBtn).toHaveText("1");
    await page.keyboard.press("q");
    await expect(spreadBtn).toHaveText("2");
    await page.keyboard.press("q");
    await expect(spreadBtn).toHaveText("2+");
    await page.keyboard.press("q");
    await expect(spreadBtn).toHaveText("1");
  });
});

test.describe("CG ツールバー — デフォルト状態", () => {
  test("初回表示で高さフィットがデフォルトになっている", async ({ page }) => {
    await openCgViewer(page);
    await showToolbar(page);

    const hBtn = page.getByRole("button", { name: "高さフィット" });
    await expect(hBtn).toHaveAttribute("aria-pressed", "true");
  });

  test("フィットボタンのラベルがアイコン表示になっている", async ({ page }) => {
    await openCgViewer(page);
    await showToolbar(page);

    const wBtn = page.getByRole("button", { name: "幅フィット" });
    const hBtn = page.getByRole("button", { name: "高さフィット" });
    await expect(wBtn).toHaveText("↔");
    await expect(hBtn).toHaveText("↕");
  });
});

test.describe("CG ツールバー — ボタンクリック", () => {
  test("CT-3: ツールバー W ボタンクリックで幅フィットになる", async ({ page }) => {
    await openCgViewer(page);
    await showToolbar(page);

    const wBtn = page.getByRole("button", { name: "幅フィット" });
    await wBtn.click();

    await expect(wBtn).toHaveAttribute("aria-pressed", "true");
  });

  test("CT-4: ツールバー H ボタンクリックで高さフィットになる", async ({ page }) => {
    await openCgViewer(page);
    await showToolbar(page);

    const hBtn = page.getByRole("button", { name: "高さフィット" });
    await hBtn.click();

    await expect(hBtn).toHaveAttribute("aria-pressed", "true");
  });

  test("CT-6: 見開きボタンクリックでモードがサイクルする", async ({ page }) => {
    await openCgViewer(page);
    await showToolbar(page);
    const spreadBtn = page.getByTestId("cg-spread-btn");

    // single → spread → spread-offset → single
    await expect(spreadBtn).toHaveText("1");
    await spreadBtn.click();
    await expect(spreadBtn).toHaveText("2");
    await spreadBtn.click();
    await expect(spreadBtn).toHaveText("2+");
    await spreadBtn.click();
    await expect(spreadBtn).toHaveText("1");
  });

  test("CT-7: ページセレクトでページに直接ジャンプする", async ({ page }) => {
    await openCgViewer(page);
    await showToolbar(page);
    await expect(page).toHaveURL(/index=0/);

    // ツールバーの <select> で Page 3 を選択 (value=2)
    const pageSelect = page.getByTestId("cg-viewer").locator("select");
    await pageSelect.selectOption("2");

    await expect(page).toHaveURL(/index=2/);
  });

  test("CT-8: 閉じるボタンでビューワーが閉じる", async ({ page }) => {
    await openCgViewer(page);
    await showToolbar(page);
    await expect(page.getByTestId("cg-viewer")).toBeVisible();

    const closeBtn = page.getByRole("button", { name: "閉じる" });
    await closeBtn.click();

    await expect(page.getByTestId("cg-viewer")).not.toBeVisible();
    await expect(page).not.toHaveURL(/index=/);
  });
});
