// Playwright E2E テスト設定
// - E2E 専用ポート (8001/5174) で開発サーバーと共存可能
// - テストデータの各ディレクトリをマウントポイントとして起動
// - プロジェクト: chromium (デスクトップ) + mobile-chromium (Pixel 5)
//   + mobile-webkit (PLAYWRIGHT_WEBKIT=1 のときのみ、ローカル任意)
// - mobile プロジェクトは tests/mobile/** のみ実行、その他は chromium のみ

import { defineConfig, devices } from "@playwright/test";
import path from "node:path";
import { generateMountsJson } from "./fixtures/generate-mounts";

const projectRoot = path.resolve(import.meta.dirname, "..");
const testDataDir = path.resolve(import.meta.dirname, "fixtures/test-data");
const mountsPath = path.resolve(import.meta.dirname, "fixtures/e2e-mounts.json");

// webServer 起動前に mounts.json を生成 (globalSetup では webServer 起動後に実行されるため)
generateMountsJson(mountsPath);

// E2E 専用ポート（開発サーバーの 8000/5173 と競合しない）
const BACKEND_PORT = 8001;
const FRONTEND_PORT = 5174;

const enableWebkit = process.env.PLAYWRIGHT_WEBKIT === "1";

export default defineConfig({
  testDir: "./tests",
  timeout: 30_000,
  retries: 1,
  reporter: [["html", { open: "never" }], ["list"]],

  webServer: [
    {
      command: `cargo run -- --port ${BACKEND_PORT}`,
      cwd: path.join(projectRoot, "backend"),
      port: BACKEND_PORT,
      reuseExistingServer: false,
      timeout: 30_000,
      env: {
        MOUNT_BASE_DIR: testDataDir,
        MOUNT_CONFIG_PATH: mountsPath,
        NODE_SECRET: "e2e-test-secret",
      },
    },
    {
      command: `VITE_API_PORT=${BACKEND_PORT} npx vite --port ${FRONTEND_PORT}`,
      cwd: path.join(projectRoot, "frontend"),
      port: FRONTEND_PORT,
      reuseExistingServer: false,
      timeout: 15_000,
    },
  ],

  use: {
    baseURL: `http://localhost:${FRONTEND_PORT}`,
    trace: "on-first-retry",
    screenshot: "only-on-failure",
  },

  projects: [
    {
      name: "chromium",
      // モバイル専用 spec は除外 (mobile-chromium プロジェクトで実行する)
      testIgnore: /\/mobile\//,
      use: {
        browserName: "chromium",
      },
    },
    {
      name: "mobile-chromium",
      // tests/mobile/** のみ実行 (Pixel 5: 393×851, hasTouch, isMobile)
      testMatch: /\/mobile\//,
      use: { ...devices["Pixel 5"] },
    },
    ...(enableWebkit
      ? [
          {
            name: "mobile-webkit",
            testMatch: /\/mobile\//,
            // PLAYWRIGHT_WEBKIT=1 のときのみ有効 (Ubuntu CI で webkit 未インストールの場合あり)
            use: { ...devices["iPhone 13"] },
          },
        ]
      : []),
  ],
});
