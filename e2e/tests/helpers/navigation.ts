// E2E テスト共通ヘルパー
// - マウントポイント遷移、ビューワー起動、検索インデックス待機

import { expect } from "@playwright/test";
import type { Page, APIRequestContext } from "@playwright/test";

// マウントポイントに遷移してブラウズ画面を開く
export async function navigateToMount(page: Page, mountName: string) {
  await page.goto("/");
  const card = page.locator("[data-testid^='mount-']", { hasText: mountName });
  await expect(card).toBeVisible();
  await card.click();
  await expect(page).toHaveURL(/\/browse\//);
}

// pictures ディレクトリで CG モードを開く
export async function openCgViewer(page: Page, mountName = "pictures") {
  await navigateToMount(page, mountName);

  // 画像タブに切り替え
  const imagesTab = page.locator("[data-testid='tab-images']");
  await expect(imagesTab).toBeVisible();
  await imagesTab.click();

  // 最初の画像カードをクリック
  const firstImage = page.locator("[data-testid^='file-card-']").first();
  await expect(firstImage).toBeVisible();
  // サムネイル読み込み完了を待機（レイアウトシフト回避）
  const thumb = firstImage.locator("img");
  await expect(thumb).toHaveJSProperty("complete", true);
  await firstImage.click();

  await expect(page.locator("[data-testid='cg-viewer']")).toBeVisible();
}

// ツールバーでマンガモードを選択してからビューワーを開く
export async function openMangaViewer(page: Page, mountName = "pictures") {
  await navigateToMount(page, mountName);

  // ツールバーでマンガモードを選択
  await page.getByTestId("mode-toggle-manga").click();
  await expect(page).toHaveURL(/mode=manga/);

  // 画像タブに切り替え
  const imagesTab = page.locator("[data-testid='tab-images']");
  await expect(imagesTab).toBeVisible();
  await imagesTab.click();

  // 最初の画像カードをクリック
  const firstImage = page.locator("[data-testid^='file-card-']").first();
  await expect(firstImage).toBeVisible();
  // サムネイル読み込み完了を待機（レイアウトシフト回避）
  const thumb = firstImage.locator("img");
  await expect(thumb).toHaveJSProperty("complete", true);
  await firstImage.click();

  await expect(page.locator("[data-testid='manga-viewer']")).toBeVisible();
}

// docs ディレクトリで PDF ビューワーを開く
export async function openPdfViewer(page: Page) {
  await navigateToMount(page, "docs");

  // sample.pdf をクリック
  const pdfCard = page.locator("[data-testid^='file-card-']", { hasText: "sample.pdf" });
  await expect(pdfCard).toBeVisible();
  await pdfCard.click();

  await expect(page.locator("[data-testid='pdf-cg-viewer']")).toBeVisible();
}

// ツールバーを表示する（マウスを上部に移動してポインターイベント発火）
export async function showToolbar(page: Page) {
  await page.mouse.move(600, 20);
  const wrapper = page.getByTestId("toolbar-wrapper");
  await expect(wrapper).toHaveCSS("opacity", "1");
}

// 検索インデックス構築完了を待機する
// バックエンド起動直後はインデックス未構築で 503 を返すか、200 でも結果が空の場合がある
// ※ 画像はインデックス対象外のため、動画ファイル名 (clip) で確認する
export async function waitForSearchIndex(request: APIRequestContext) {
  await expect.poll(
    async () => {
      const res = await request.get("/api/search?q=clip&limit=1");
      if (res.status() !== 200) return 0;
      const body = await res.json();
      return body.results?.length ?? 0;
    },
    { timeout: 30_000, message: "検索インデックス構築待ち" },
  ).toBeGreaterThanOrEqual(1);
}
