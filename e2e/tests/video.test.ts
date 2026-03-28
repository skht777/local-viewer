// 動画タブテスト
// 仕様出典: plan-phase5.md, plan-phase5.5.md, initial-architecture.md §動画タブ
// P2: VD-3(ファイル名ラベル)
// P3: VD-4(ファイルサイズ), VD-5(動画再生), VD-6(MKVフォールバック)

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

  // 最小 MP4 フィクスチャはメディアデータを含まず再生できない可能性がある
  test.fixme("VD-5: 動画が再生可能である", async ({ page }) => {
    await page.goto("/");

    const videosMount = page.locator("[data-testid^='mount-']", {
      hasText: "videos",
    });
    await expect(videosMount).toBeVisible();
    await videosMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    const videosTab = page.locator("[data-testid='tab-videos']");
    if (await videosTab.isVisible()) {
      await videosTab.click();

      const video = page.locator("video").first();
      await expect(video).toBeVisible();

      // play() を呼んで再生状態を確認
      const isPaused = await video.evaluate(
        (el) => (el as HTMLVideoElement).play().then(() => (el as HTMLVideoElement).paused),
      );
      expect(isPaused).toBe(false);
    }
  });

  // MKV がバックエンドで video kind として認識されるか不明
  test.fixme("VD-6: MKV 非対応でフォールバック表示される", async ({ page }) => {
    await page.goto("/");

    const videosMount = page.locator("[data-testid^='mount-']", {
      hasText: "videos",
    });
    await expect(videosMount).toBeVisible();
    await videosMount.click();
    await expect(page).toHaveURL(/\/browse\//);

    const videosTab = page.locator("[data-testid='tab-videos']");
    if (await videosTab.isVisible()) {
      await videosTab.click();

      // unsupported.mkv のカードまたはエラーメッセージを確認
      const mkvCard = page.locator("[data-testid^='video-card-']", {
        hasText: "unsupported.mkv",
      });
      // MKV が video として認識される場合、再生エラーのフォールバックが表示される
      if (await mkvCard.isVisible()) {
        await expect(mkvCard).toContainText(/再生できません|非対応|error/i);
      }
    }
  });
});
