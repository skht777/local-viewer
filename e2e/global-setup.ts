// Playwright グローバルセットアップ
// テスト実行前に mounts.json を生成する

import path from "node:path";
import { generateMountsJson } from "./fixtures/generate-mounts";

const mountsPath = path.resolve(import.meta.dirname, "fixtures", "e2e-mounts.json");

export default function globalSetup(): void {
  generateMountsJson(mountsPath);
}
