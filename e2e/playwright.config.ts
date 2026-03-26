// Playwright E2E テスト設定
// - E2E 専用ポート (8001/5174) で開発サーバーと共存可能
// - テストデータの ROOT_DIR でバックエンドを起動
// - 現在 chromium のみ

import { defineConfig } from "@playwright/test";
import path from "node:path";

const projectRoot = path.resolve(import.meta.dirname, "..");
const testDataDir = path.resolve(import.meta.dirname, "fixtures/test-data");

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
      command: `bash -c 'source ${projectRoot}/backend/.venv/bin/activate && ROOT_DIR=${testDataDir} NODE_SECRET=e2e-test-secret uvicorn backend.main:app --port ${BACKEND_PORT}'`,
      cwd: projectRoot,
      port: BACKEND_PORT,
      reuseExistingServer: false,
      timeout: 15_000,
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
