// 無限スクロール（ページネーション）テスト
// 仕様出典: spec-ui-behavior.md §ファイル一覧
// IntersectionObserver + useInfiniteQuery でセンチネル到達時に次ページ自動ロード

import { test, expect } from "@playwright/test";
import { navigateToMount } from "./helpers/navigation";

test.describe("無限スクロール", () => {
  test("pictures マウントポイントで全カードが表示される", async ({ page }) => {
    // pictures は 4 件 — limit=100 以下なので 1 ページで全件
    await navigateToMount(page, "pictures");

    // 画像タブに切り替え
    const imagesTab = page.locator("[data-testid='tab-images']");
    await expect(imagesTab).toBeVisible();
    await imagesTab.click();

    // 全カードが表示される
    const cards = page.locator("[data-testid^='file-card-']");
    await expect(cards).toHaveCount(4, { timeout: 10_000 });
  });

  test("limit パラメータ付き API でページネーションが動作する", async ({
    page,
    request,
  }) => {
    // API レベルでページネーションの正常動作を検証
    // pictures マウントポイントの node_id を取得
    const mountsRes = await request.get("/api/mounts");
    expect(mountsRes.ok()).toBeTruthy();
    const mounts = await mountsRes.json();
    const pictures = mounts.mounts.find(
      (m: { name: string }) => m.name === "pictures",
    );
    expect(pictures).toBeTruthy();

    // limit=2 で最初のページを取得
    const page1Res = await request.get(
      `/api/browse/${pictures.node_id}?limit=2&sort=name-asc`,
    );
    expect(page1Res.ok()).toBeTruthy();
    const page1 = await page1Res.json();
    expect(page1.entries.length).toBe(2);
    expect(page1.next_cursor).not.toBeNull();
    expect(page1.total_count).toBeGreaterThanOrEqual(4);

    // カーソルで次のページを取得
    const page2Res = await request.get(
      `/api/browse/${pictures.node_id}?limit=2&sort=name-asc&cursor=${page1.next_cursor}`,
    );
    expect(page2Res.ok()).toBeTruthy();
    const page2 = await page2Res.json();
    expect(page2.entries.length).toBeGreaterThanOrEqual(1);

    // 重複なし
    const page1Ids = page1.entries.map((e: { node_id: string }) => e.node_id);
    const page2Ids = page2.entries.map((e: { node_id: string }) => e.node_id);
    const overlap = page1Ids.filter((id: string) => page2Ids.includes(id));
    expect(overlap).toHaveLength(0);
  });
});
