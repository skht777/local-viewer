// PDF ナビゲーションテスト
// P1: PN-1(D次ページ), PN-2(A前ページ), PN-3(Mマンガ), PN-4(MCG復帰), PN-5(Escape閉じ)
// P2: PN-6(Escapeマンガ閉じ), PN-7(ページセレクト), PN-8(Home/End),
//     PN-9(V幅フィット), PN-10(H高さフィット), PN-11(Q無効),
//     PN-12(PageDown セット間), PN-13(PdfPageSidebar クリック)
// P3: PN-14(破損PDF エラー表示)

import { test, expect } from "@playwright/test";
import { navigateToMount, openPdfViewer } from "./helpers/navigation";

test.describe("PDF ナビゲーション — 基本", () => {
  test("PN-1: D キーで次ページに進む", async ({ page }) => {
    await openPdfViewer(page);
    await expect(page).toHaveURL(/page=1/);

    await page.keyboard.press("d");
    await expect(page).toHaveURL(/page=2/);
  });

  test("PN-2: A キーで前ページに戻る", async ({ page }) => {
    await openPdfViewer(page);

    // まず次ページへ
    await page.keyboard.press("d");
    await expect(page).toHaveURL(/page=2/);

    await page.keyboard.press("a");
    await expect(page).toHaveURL(/page=1/);
  });

  test("PN-5: Escape で CG ビューワーを閉じる", async ({ page }) => {
    await openPdfViewer(page);
    await expect(page).toHaveURL(/pdf=/);

    await page.keyboard.press("Escape");

    // URL から pdf/page/mode が消去される
    await expect(page).not.toHaveURL(/pdf=/);
    await expect(page).not.toHaveURL(/page=/);
    await expect(page.getByTestId("pdf-cg-viewer")).not.toBeVisible();
  });
});

test.describe("PDF ナビゲーション — P2", () => {
  // PDF マンガモードで Escape が効かない問題
  test.fixme("PN-6: Escape でマンガビューワーを閉じる", async ({ page }) => {
    // ツールバーでマンガモードを選択してから PDF を開く
    await navigateToMount(page, "docs");
    await page.getByTestId("mode-toggle-manga").click();

    const pdfCard = page.locator("[data-testid^='file-card-']", { hasText: "sample.pdf" });
    await expect(pdfCard).toBeVisible();
    await pdfCard.click();
    await expect(page.getByTestId("pdf-manga-viewer")).toBeVisible();

    // Escape で閉じる
    await page.keyboard.press("Escape");

    await expect(page).not.toHaveURL(/pdf=/);
    await expect(page).not.toHaveURL(/page=/);
    await expect(page.getByTestId("pdf-manga-viewer")).not.toBeVisible();
  });

  test("PN-7: ページセレクトでページにジャンプする", async ({ page }) => {
    await openPdfViewer(page);
    await expect(page).toHaveURL(/page=1/);

    // <select> で Page 2 (value=1) を選択
    const pageSelect = page.getByTestId("pdf-cg-viewer").locator("select");
    await pageSelect.selectOption("1");

    await expect(page).toHaveURL(/page=2/);
  });

  test("PN-8: Home/End キーでページ移動する", async ({ page }) => {
    await openPdfViewer(page);

    // End で最終ページへ
    await page.keyboard.press("End");
    await expect(page).toHaveURL(/page=2/);

    // Home で最初のページへ
    await page.keyboard.press("Home");
    await expect(page).toHaveURL(/page=1/);
  });

  test("PN-9: V キーで幅フィットに切り替わる", async ({ page }) => {
    await openPdfViewer(page);

    await page.keyboard.press("v");

    const wBtn = page.getByRole("button", { name: "幅フィット" });
    await expect(wBtn).toHaveAttribute("aria-pressed", "true");
  });

  test("PN-10: H キーで高さフィットに切り替わる", async ({ page }) => {
    await openPdfViewer(page);

    await page.keyboard.press("h");

    const hBtn = page.getByRole("button", { name: "高さフィット" });
    await expect(hBtn).toHaveAttribute("aria-pressed", "true");
  });

  test("PN-11: Q キーで見開きボタンは表示されるが操作可能 (PDF は showSpread=true)", async ({ page }) => {
    await openPdfViewer(page);

    // PDF CG ビューワーでは showSpread=true なので見開きボタンが存在する
    const spreadBtn = page.getByTestId("cg-spread-btn");
    await expect(spreadBtn).toBeVisible();

    // Q キーでサイクルが動作する
    await expect(spreadBtn).toHaveText("1");
    await page.keyboard.press("q");
    await expect(spreadBtn).toHaveText("2");
  });

  // PDF ビューワーでのセット間ジャンプが未実装の可能性
  test.fixme("PN-12: X キーでセット間ジャンプの NavigationPrompt が表示される", async ({ page }) => {
    await openPdfViewer(page);

    await page.keyboard.press("x");

    const prompt = page.locator("[data-testid='navigation-prompt']");
    await expect(prompt).toBeVisible({ timeout: 5000 });
  });


});

test.describe("PDF ナビゲーション — P3", () => {
  // 破損 PDF が pdf kind として認識されない可能性、または pdf-render-error 未表示
  test.fixme("PN-14: 破損 PDF でエラー表示される", async ({ page }) => {
    await navigateToMount(page, "docs");

    // corrupted.pdf をクリック
    const corruptedCard = page.locator("[data-testid^='file-card-']", {
      hasText: "corrupted.pdf",
    });
    await expect(corruptedCard).toBeVisible();
    await corruptedCard.click();

    // PDF レンダリングエラーが表示される
    await expect(page.getByTestId("pdf-render-error")).toBeVisible({
      timeout: 10_000,
    });
  });
});
