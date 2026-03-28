// 動画タブテスト
// 仕様出典: plan-phase5.md, plan-phase5.5.md, initial-architecture.md §動画タブ
// P2: VD-3(ファイル名ラベル)
// P3: VD-4(ファイルサイズ), VD-5(再生エラーフォールバック), VD-6(MKVフォールバック)

import { test, expect } from "@playwright/test";

test.describe("動画タブ", () => {
  test("動画タブで動画カードが表示される", async ({ page }) => {
    await page.goto("/");

    // videos マウントポイントカードをクリック
    const videosMount = page.locator("[data-testid^='mount-']", {
      hasText: "videos",
    });
    await expect(videosMount).toBeVisible();
    await videosMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    // 動画タブに切り替え
    const videosTab = page.locator("[data-testid='tab-videos']");
    if (await videosTab.isVisible()) {
      await videosTab.click();
      await expect(page).toHaveURL(/tab=videos/);

      // 動画カードが表示される
      const videoCards = page.locator("[data-testid^='video-card-']");
      await expect(videoCards.first()).toBeVisible();
    }
  });

  test("video 要素が存在し src が設定されている", async ({ page }) => {
    await page.goto("/");

    // videos マウントポイントカードをクリック
    const videosMount = page.locator("[data-testid^='mount-']", {
      hasText: "videos",
    });
    await expect(videosMount).toBeVisible();
    await videosMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    const videosTab = page.locator("[data-testid='tab-videos']");
    if (await videosTab.isVisible()) {
      await videosTab.click();

      // <video> 要素を確認
      const video = page.locator("video").first();
      await expect(video).toBeVisible();
      const src = await video.getAttribute("src");
      expect(src).toBeTruthy();
    }
  });

  test("VD-3: 動画カード内にファイル名ラベルが表示される", async ({ page }) => {
    await page.goto("/");

    // videos マウントポイントへ遷移
    const videosMount = page.locator("[data-testid^='mount-']", {
      hasText: "videos",
    });
    await expect(videosMount).toBeVisible();
    await videosMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    const videosTab = page.locator("[data-testid='tab-videos']");
    if (await videosTab.isVisible()) {
      await videosTab.click();

      // video-card 内に "clip1.mp4" テキストが表示される
      const videoCard = page.locator("[data-testid^='video-card-']").first();
      await expect(videoCard).toBeVisible();
      await expect(videoCard).toContainText("clip1.mp4");
    }
  });

  test("VD-4: 動画カード内にファイルサイズが表示される", async ({ page }) => {
    await page.goto("/");

    // videos マウントポイントへ遷移
    const videosMount = page.locator("[data-testid^='mount-']", {
      hasText: "videos",
    });
    await expect(videosMount).toBeVisible();
    await videosMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    const videosTab = page.locator("[data-testid='tab-videos']");
    if (await videosTab.isVisible()) {
      await videosTab.click();

      // video-card 内にサイズ表示 (B, KB, MB 等)
      const videoCard = page.locator("[data-testid^='video-card-']").first();
      await expect(videoCard).toBeVisible();
      await expect(videoCard).toContainText(/\d+(\.\d+)?\s*(B|KB|MB|GB)/);
    }
  });

  // 最小 MP4 フィクスチャはメディアデータを含まないため再生不可
  test("VD-5: 再生不可の動画でエラーフォールバックが表示される", async ({ page }) => {
    await page.goto("/");

    const videosMount = page.locator("[data-testid^='mount-']", {
      hasText: "videos",
    });
    await expect(videosMount).toBeVisible();
    await videosMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    // videosTab は確定アサーション（サイレントパス防止）
    const videosTab = page.locator("[data-testid='tab-videos']");
    await expect(videosTab).toBeVisible();
    await videosTab.click();

    // clip1.mp4 を明示的に指定（ソート順に依存しない）
    const videoCard = page.locator("[data-testid^='video-card-']", {
      hasText: "clip1.mp4",
    });
    await expect(videoCard).toBeVisible();

    // load() でリソース読み込みを開始 → 無効 MP4 で error イベント発火
    // preload="none" のためブラウザは load() まで何もしない
    const video = videoCard.locator("video");
    await expect(video).toBeVisible();
    await video.evaluate((el) => (el as HTMLVideoElement).load());

    // VideoCard の onError → hasError=true → フォールバック表示
    await expect(
      videoCard.getByTestId("video-error-fallback"),
    ).toBeVisible({ timeout: 10_000 });
  });

  // MKV はバックエンドで kind="video" として認識済み、ブラウザは非対応
  test("VD-6: MKV 非対応でフォールバック表示される", async ({ page }) => {
    await page.goto("/");

    const videosMount = page.locator("[data-testid^='mount-']", {
      hasText: "videos",
    });
    await expect(videosMount).toBeVisible();
    await videosMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    const videosTab = page.locator("[data-testid='tab-videos']");
    await expect(videosTab).toBeVisible();
    await videosTab.click();

    const mkvCard = page.locator("[data-testid^='video-card-']", {
      hasText: "unsupported.mkv",
    });
    await expect(mkvCard).toBeVisible();

    // load() で MKV ロード開始 → ブラウザ非対応で error イベント発火
    const video = mkvCard.locator("video");
    await expect(video).toBeVisible();
    await video.evaluate((el) => (el as HTMLVideoElement).load());

    await expect(
      mkvCard.getByTestId("video-error-fallback"),
    ).toBeVisible({ timeout: 10_000 });
  });
});
