// Playwright E2E テスト設定
// - E2E 専用ポート (8001/5174) で開発サーバーと共存可能
// - テストデータの各ディレクトリをマウントポイントとして起動
// - 現在 chromium のみ

import { defineConfig } from "@playwright/test";
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
      use: {
        browserName: "chromium",
      },
    },
  ],
});
