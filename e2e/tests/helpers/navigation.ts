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
  // サムネイル読み込みによる DOM 再構築を待つ
  await firstImage.click({ force: true });

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
  await firstImage.click({ force: true });

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

// 検索インデックス構築完了を待機する
// バックエンド起動直後はインデックス未構築で 503 を返す場合がある
export async function waitForSearchIndex(request: APIRequestContext) {
  await expect.poll(
    async () => {
      const res = await request.get("/api/search?q=photo&limit=1");
      return res.status();
    },
    { timeout: 15_000, message: "検索インデックス構築待ち" },
  ).not.toBe(503);
}
