// ツールバー自動表示/非表示 E2E テスト
// - デスクトップ: ツールバー初期非表示、上部ホバーで表示
// - マウス中央移動で非表示に戻る
// - CG / マンガ / PDF 全モード共通

import { test, expect } from "@playwright/test";
import { openCgViewer, openMangaViewer, openPdfViewer } from "./helpers/navigation";

test.describe("ツールバー自動表示/非表示 — CG モード", () => {
  test("CG モードでツールバーが初期非表示", async ({ page }) => {
    await openCgViewer(page);

    // マウスを中央に移動して初期状態を確認
    await page.mouse.move(600, 400);

    const wrapper = page.getByTestId("toolbar-wrapper");
    await expect(wrapper).toHaveCSS("opacity", "0");
  });

  test("マウスを上部に移動するとツールバーが表示される", async ({ page }) => {
    await openCgViewer(page);

    // マウスを上部 (Y=20) に移動
    await page.mouse.move(600, 20);

    const wrapper = page.getByTestId("toolbar-wrapper");
    await expect(wrapper).toHaveCSS("opacity", "1");
  });

  test("マウスを中央に移動するとツールバーが非表示になる", async ({ page }) => {
    await openCgViewer(page);

    // 上部で表示
    await page.mouse.move(600, 20);
    const wrapper = page.getByTestId("toolbar-wrapper");
    await expect(wrapper).toHaveCSS("opacity", "1");

    // 中央に移動で非表示
    await page.mouse.move(600, 400);
    await expect(wrapper).toHaveCSS("opacity", "0");
  });
});

test.describe("ツールバー自動表示/非表示 — マンガモード", () => {
  test("マンガモードでも上部ホバーで表示される", async ({ page }) => {
    await openMangaViewer(page);

    // 初期状態は非表示
    await page.mouse.move(600, 400);
    const wrapper = page.getByTestId("toolbar-wrapper");
    await expect(wrapper).toHaveCSS("opacity", "0");

    // 上部ホバーで表示
    await page.mouse.move(600, 20);
    await expect(wrapper).toHaveCSS("opacity", "1");
  });
});

test.describe("ツールバー自動表示/非表示 — PDF CG モード", () => {
  test("PDF CG モードでも上部ホバーで表示される", async ({ page }) => {
    await openPdfViewer(page);

    // 初期状態は非表示
    await page.mouse.move(600, 400);
    const wrapper = page.getByTestId("toolbar-wrapper");
    await expect(wrapper).toHaveCSS("opacity", "0");

    // 上部ホバーで表示
    await page.mouse.move(600, 20);
    await expect(wrapper).toHaveCSS("opacity", "1");
  });
});
