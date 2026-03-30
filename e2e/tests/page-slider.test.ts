// ページスライダーテスト
// - CG モード: 画面下部の水平スライダー（フェードイン/アウト）
// - マンガモード: 画面右端の縦スライダー（フェードイン/アウト）

import { test, expect } from "@playwright/test";
import { openCgViewer, openMangaViewer, openPdfViewer } from "./helpers/navigation";

test.describe("CG モード — 水平ページスライダー", () => {
  test("PS-1: マウスが画面下部に近づくとスライダーがフェードインする", async ({ page }) => {
    await openCgViewer(page);
    const slider = page.getByTestId("page-slider");

    // 初期状態: 非表示（opacity: 0）
    await expect(slider).toHaveCSS("opacity", "0");

    // マウスを画面下部に移動（steps で pointermove の確実な配信を保証）
    const viewer = page.getByTestId("cg-viewer");
    const box = await viewer.boundingBox();
    if (!box) throw new Error("viewer not found");
    await page.mouse.move(box.x + box.width / 2, box.y + box.height - 20, { steps: 5 });

    // フェードイン
    await expect(slider).toHaveCSS("opacity", "1", { timeout: 3000 });
  });

  test("PS-2: マウスが離れるとスライダーがフェードアウトする", async ({ page }) => {
    await openCgViewer(page);
    const slider = page.getByTestId("page-slider");

    // まずフェードインさせる
    const viewer = page.getByTestId("cg-viewer");
    const box = await viewer.boundingBox();
    if (!box) throw new Error("viewer not found");
    await page.mouse.move(box.x + box.width / 2, box.y + box.height - 20, { steps: 5 });
    await expect(slider).toHaveCSS("opacity", "1", { timeout: 3000 });

    // マウスを画面中央（上部寄り）に移動
    await page.mouse.move(box.x + box.width / 2, box.y + 50, { steps: 5 });

    // フェードアウト
    await expect(slider).toHaveCSS("opacity", "0", { timeout: 3000 });
  });

  test("PS-3: スライダー操作で URL の index が変化する", async ({ page }) => {
    await openCgViewer(page);

    // フェードインさせる
    const viewer = page.getByTestId("cg-viewer");
    const box = await viewer.boundingBox();
    if (!box) throw new Error("viewer not found");
    await page.mouse.move(box.x + box.width / 2, box.y + box.height - 20, { steps: 5 });

    const rangeInput = page.getByTestId("page-slider").locator("input[type='range']");
    await expect(rangeInput).toBeVisible({ timeout: 3000 });

    // スライダーを操作
    await rangeInput.fill("2");
    await expect(page).toHaveURL(/index=2/);
  });

  test("PS-4: スライダーに data-testid='page-slider' が付与されている", async ({ page }) => {
    await openCgViewer(page);
    const slider = page.getByTestId("page-slider");
    await expect(slider).toBeAttached();
  });

  test("PS-5: スライダーに aria-label='ページスライダー' が付与されている", async ({ page }) => {
    await openCgViewer(page);
    const rangeInput = page.getByRole("slider", { name: "ページスライダー" });
    await expect(rangeInput).toBeAttached();
  });

  test("PS-6: 画像1枚のみの場合スライダーが表示されない", async ({ page }) => {
    // nested/dirs/sub1 は画像1枚のみ
    await test.step("nested マウントポイントの dirs/sub1 に移動", async () => {
      await page.goto("/");
      const nestedMount = page.locator("[data-testid^='mount-']", { hasText: "nested" });
      await expect(nestedMount).toBeVisible();
      await nestedMount.click();
      await expect(page).toHaveURL(/\/browse\//);

      // dirs サブディレクトリに入る
      const dirsDir = page.locator("[data-testid^='file-card-']", { hasText: "dirs" });
      await expect(dirsDir).toBeVisible();
      await dirsDir.click();
      await expect(page).toHaveURL(/\/browse\//);

      const sub1 = page.locator("[data-testid^='file-card-']", { hasText: "sub1" });
      await expect(sub1).toBeVisible();
      await sub1.click();
    });

    await test.step("画像タブで1枚の画像を開く", async () => {
      const imagesTab = page.locator("[data-testid='tab-images']");
      await expect(imagesTab).toBeVisible();
      await imagesTab.click();

      const firstImage = page.locator("[data-testid^='file-card-']").first();
      await expect(firstImage).toBeVisible();
      await firstImage.click({ force: true });
      await expect(page.getByTestId("cg-viewer")).toBeVisible();
    });

    await test.step("スライダーが DOM に存在しない", async () => {
      const slider = page.getByTestId("page-slider");
      await expect(slider).not.toBeAttached();
    });
  });
});

test.describe("CG モード — PDF ページスライダー", () => {
  test("PS-7: PDF CG モードでもスライダーが存在する", async ({ page }) => {
    await openPdfViewer(page);
    const slider = page.getByTestId("page-slider");
    await expect(slider).toBeAttached();
  });

  test("PS-8: PDF CG モードでスライダー操作でページが変わる", async ({ page }) => {
    await openPdfViewer(page);

    // フェードインさせる
    const viewer = page.getByTestId("pdf-cg-viewer");
    const box = await viewer.boundingBox();
    if (!box) throw new Error("viewer not found");
    await page.mouse.move(box.x + box.width / 2, box.y + box.height - 20, { steps: 5 });

    const rangeInput = page.getByTestId("page-slider").locator("input[type='range']");
    await expect(rangeInput).toBeVisible({ timeout: 3000 });

    await rangeInput.fill("1");
    await expect(page).toHaveURL(/page=2/);
  });
});

test.describe("マンガモード — 縦ページスライダー", () => {
  test("PS-9: マウスが画面右端に近づくとスライダーがフェードインする", async ({ page }) => {
    await openMangaViewer(page);
    const slider = page.getByTestId("page-slider");

    // 初期状態: 非表示
    await expect(slider).toHaveCSS("opacity", "0");

    // マウスを画面右端に移動
    const viewer = page.getByTestId("manga-viewer");
    const box = await viewer.boundingBox();
    if (!box) throw new Error("viewer not found");
    await page.mouse.move(box.x + box.width - 20, box.y + box.height / 2, { steps: 5 });

    // フェードイン
    await expect(slider).toHaveCSS("opacity", "1", { timeout: 3000 });
  });

  test("PS-10: マウスが離れるとスライダーがフェードアウトする", async ({ page }) => {
    await openMangaViewer(page);
    const slider = page.getByTestId("page-slider");

    // フェードインさせる
    const viewer = page.getByTestId("manga-viewer");
    const box = await viewer.boundingBox();
    if (!box) throw new Error("viewer not found");
    await page.mouse.move(box.x + box.width - 20, box.y + box.height / 2, { steps: 5 });
    await expect(slider).toHaveCSS("opacity", "1", { timeout: 3000 });

    // マウスを左側に移動
    await page.mouse.move(box.x + 100, box.y + box.height / 2, { steps: 5 });

    // フェードアウト
    await expect(slider).toHaveCSS("opacity", "0", { timeout: 3000 });
  });

  test("PS-11: 縦スライダー操作でスクロール位置が変化する", async ({ page }) => {
    await openMangaViewer(page);

    // フェードインさせる
    const viewer = page.getByTestId("manga-viewer");
    const box = await viewer.boundingBox();
    if (!box) throw new Error("viewer not found");
    await page.mouse.move(box.x + box.width - 20, box.y + box.height / 2, { steps: 5 });

    const rangeInput = page.getByTestId("page-slider").locator("input[type='range']");
    await expect(rangeInput).toBeVisible({ timeout: 3000 });

    // スクロールエリアの初期位置を記録
    const scrollArea = page.getByTestId("manga-scroll-area");
    const scrollBefore = await scrollArea.evaluate((el) => el.scrollTop);

    // スライダーを末尾付近に操作
    await rangeInput.fill("3");

    // スクロール位置が変化
    await expect.poll(
      () => scrollArea.evaluate((el) => el.scrollTop),
      { message: "スライダー操作でスクロール位置が変化するはず", timeout: 5000 },
    ).toBeGreaterThan(scrollBefore);
  });
});

test.describe("ツールバー — ページカウンター", () => {
  test("PS-12: CG モードのツールバーにページカウンターが表示される", async ({ page }) => {
    await openCgViewer(page);
    const counter = page.getByTestId("page-counter");
    await expect(counter).toBeVisible();
    await expect(counter).toHaveText(/1\s*\/\s*\d+/);
  });

  test("PS-13: マンガモードのツールバーにページカウンターが表示される", async ({ page }) => {
    await openMangaViewer(page);
    const counter = page.getByTestId("page-counter");
    await expect(counter).toBeVisible();
    await expect(counter).toHaveText(/1\s*\/\s*\d+/);
  });

  test("PS-14: PDF CG モードのツールバーにページカウンターが表示される", async ({ page }) => {
    await openPdfViewer(page);
    const counter = page.getByTestId("page-counter");
    await expect(counter).toBeVisible();
    await expect(counter).toHaveText(/1\s*\/\s*\d+/);
  });

  test("PS-15: サムネイルサイドバーが表示されない", async ({ page }) => {
    await openCgViewer(page);
    const sidebar = page.getByTestId("cg-viewer").locator("aside");
    await expect(sidebar).not.toBeVisible();
  });
});
