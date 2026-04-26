// 履歴モデル: 同一 nodeId への navigate を抑制し history 重複を防ぐ E2E テスト
// - BrowsePage の navigateBrowse callback は targetNodeId === 現在 nodeId を早期 return
// - これによりツリー/パンくずで自身を再クリックしても URL が増えず、
//   ブラウザバックで前のページに 1 ステップで戻れる

import { expect, test } from "@playwright/test";
import { navigateToMount } from "./helpers/navigation";

test.describe("ブラウズ間 navigate の自身重複抑制", () => {
  test("ツリーで自身ディレクトリを再クリックしても URL が変わらない", async ({ page }) => {
    await navigateToMount(page, "nested");
    const beforeUrl = page.url();

    // ツリーの active ノード（現在ディレクトリ = nested）を取得
    const activeNode = page.locator(`[data-testid='tree-node-']`).first();
    // tree-node-<nodeId> パターンで現在ノードを探す
    const currentNodeId = beforeUrl.split("/browse/")[1]?.split(/[?#]/)[0];
    expect(currentNodeId).toBeTruthy();

    const selfNode = page.locator(`[data-testid='tree-node-${currentNodeId}']`);
    await expect(selfNode).toBeVisible();

    // 自身ノードをクリック → navigateBrowse のガードで navigate されない
    await selfNode.click();

    // URL は変化しない
    expect(page.url()).toBe(beforeUrl);
  });

  test("自身を再クリックしてもブラウザバック 1 回で前のページに戻る", async ({ page }) => {
    // TopPage → nested に進入（page.goto + click で履歴確定）
    await page.goto("/");
    const topUrl = page.url();
    const card = page.locator("[data-testid^='mount-']", { hasText: "nested" });
    await card.click();
    await expect(page).toHaveURL(/\/browse\//);
    const browseUrl = page.url();
    const currentNodeId = browseUrl.split("/browse/")[1]?.split(/[?#]/)[0];

    // 自身ノードを 3 回クリック
    const selfNode = page.locator(`[data-testid='tree-node-${currentNodeId}']`);
    await selfNode.click();
    await selfNode.click();
    await selfNode.click();
    expect(page.url()).toBe(browseUrl);

    // ブラウザバック 1 回で TopPage に戻る（同一 nodeId は history に積まれていないため）
    await page.goBack();
    expect(page.url()).toBe(topUrl);
  });
});
