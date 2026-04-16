// URL 直接遷移・永続化テスト
// P1: UD-1(CG直接), UD-2(マンガ直接), UD-3(PDF直接)
// P2: UD-4(ブラウザ戻る), UD-5(pdf/index排他), UD-6(fitMode永続化),
//     UD-7(zoomLevel永続化), UD-8(タブURL保持), UD-9(scrollSpeed永続化)

import { test, expect } from "@playwright/test";
import { openCgViewer, openMangaViewer, navigateToMount, showToolbar } from "./helpers/navigation";

// API から node_id を動的取得するヘルパー
async function getNodeIds(request: import("@playwright/test").APIRequestContext) {
  // /api/mounts でマウントポイント一覧を取得
  const mountsRes = await request.get("/api/mounts");
  const mountsData = await mountsRes.json();

  // pictures マウントポイントの node_id
  const picturesMount = mountsData.mounts.find((m: { name: string }) => m.name === "pictures");
  if (!picturesMount) throw new Error("pictures マウントポイントが見つかりません");

  // docs マウントポイントの node_id
  const docsMount = mountsData.mounts.find((m: { name: string }) => m.name === "docs");
  if (!docsMount) throw new Error("docs マウントポイントが見つかりません");

  // docs 内の sample.pdf の node_id
  const docsRes = await request.get(`/api/browse/${docsMount.node_id}`);
  const docsData = await docsRes.json();
  const samplePdf = docsData.entries.find((e: { name: string }) => e.name === "sample.pdf");
  if (!samplePdf) throw new Error("sample.pdf が見つかりません");

  return {
    picturesNodeId: picturesMount.node_id as string,
    docsNodeId: docsMount.node_id as string,
    pdfNodeId: samplePdf.node_id as string,
  };
}

test.describe("URL 直接遷移", () => {
  test("UD-1: 直接 URL で CG モード 3ページ目が開く", async ({ page, request }) => {
    const { picturesNodeId } = await getNodeIds(request);

    await page.goto(`/browse/${picturesNodeId}?tab=images&index=2&mode=cg`);

    await expect(page.getByTestId("cg-viewer")).toBeVisible();
    const counter = page.getByTestId("page-counter");
    await expect(counter).toContainText(/3\s*\/\s*\d+/);
  });

  test("UD-2: 直接 URL でマンガモードが開く", async ({ page, request }) => {
    const { picturesNodeId } = await getNodeIds(request);

    await page.goto(`/browse/${picturesNodeId}?tab=images&index=0&mode=manga`);

    await expect(page.getByTestId("manga-viewer")).toBeVisible();
  });

  test("UD-3: PDF 直接 URL で指定ページが開く", async ({ page, request }) => {
    const { docsNodeId, pdfNodeId } = await getNodeIds(request);

    await page.goto(`/browse/${docsNodeId}?pdf=${pdfNodeId}&page=2&mode=cg`);

    await expect(page.getByTestId("pdf-cg-viewer")).toBeVisible();
    const counter = page.getByTestId("page-counter");
    await expect(counter).toContainText(/2\s*\/\s*2/);
  });
});

test.describe("URL 履歴・排他制御", () => {
  test("UD-4: ブラウザ戻るボタンで前の URL 状態に復帰する", async ({ page }) => {
    await openCgViewer(page);
    await expect(page).toHaveURL(/index=/);
    const cgUrl = page.url();

    // Escape でビューワーを閉じる
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("cg-viewer")).not.toBeVisible();

    // ブラウザ戻る → CG ビューワーの URL に復帰
    await page.goBack();
    await expect(page).toHaveURL(cgUrl);
  });

  test("UD-5: pdf と index が同時指定された場合 PDF が優先される", async ({ page, request }) => {
    const { docsNodeId, pdfNodeId } = await getNodeIds(request);

    // pdf と index を両方指定
    await page.goto(`/browse/${docsNodeId}?pdf=${pdfNodeId}&index=1&mode=cg`);

    // PDF ビューワーが表示される（画像ビューワーではない）
    await expect(page.getByTestId("pdf-cg-viewer")).toBeVisible();
  });
});

test.describe("localStorage 永続化", () => {
  test("UD-6: fitMode が localStorage に永続化される", async ({ page }) => {
    await openCgViewer(page);

    // V キーで幅フィットに切り替え
    await page.keyboard.press("v");
    const wBtn = page.getByRole("button", { name: "幅フィット" });
    await expect(wBtn).toHaveAttribute("aria-pressed", "true");

    // ビューワーを閉じる
    await page.keyboard.press("b");
    await expect(page.getByTestId("cg-viewer")).not.toBeVisible();

    // CG ビューワーを再度開く
    await openCgViewer(page);

    // 幅フィットが維持されている
    const wBtnAfter = page.getByRole("button", { name: "幅フィット" });
    await expect(wBtnAfter).toHaveAttribute("aria-pressed", "true");
  });

  test("UD-7: zoomLevel が localStorage に永続化される", async ({ page }) => {
    await openMangaViewer(page);

    // + キーで 125% にズーム
    await page.keyboard.press("Equal");
    await expect(page.getByTestId("manga-zoom-level")).toHaveText("125%");

    // ビューワーを閉じる
    await page.keyboard.press("b");
    await expect(page.getByTestId("manga-viewer")).not.toBeVisible();

    // マンガビューワーを再度開く
    await openMangaViewer(page);

    // 125% が維持されている
    await expect(page.getByTestId("manga-zoom-level")).toHaveText("125%");
  });

  test("UD-8: タブ切替の URL がリロード後も保持される", async ({ page }) => {
    await navigateToMount(page, "videos");

    // 動画タブに切り替え
    const videosTab = page.locator("[data-testid='tab-videos']");
    if (await videosTab.isVisible()) {
      await videosTab.click();
      await expect(page).toHaveURL(/tab=videos/);

      // リロード
      await page.reload();

      // tab=videos が維持される
      await expect(page).toHaveURL(/tab=videos/);
    }
  });

  test("UD-9: scrollSpeed が localStorage に永続化される", async ({ page }) => {
    await openMangaViewer(page);
    await showToolbar(page);

    // 速度を 2.0x に変更
    const speedSlider = page.getByRole("slider", { name: "スクロール速度" });
    await speedSlider.fill("2");
    await expect(page.getByTestId("manga-scroll-speed-label")).toHaveText("2x");

    // ビューワーを閉じてリロード
    await page.keyboard.press("b");
    await page.reload();

    // マンガビューワーを再度開く
    await openMangaViewer(page);

    // 2.0x が維持されている
    await expect(page.getByTestId("manga-scroll-speed-label")).toHaveText("2x");
  });
});
