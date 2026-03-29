// E2E テスト用 mounts.json を生成する (v2 スキーマ)
// playwright globalSetup から呼び出される
// test-data 配下の各ディレクトリを個別マウントポイントとして登録

import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";

const testDataDir = path.resolve(import.meta.dirname, "test-data");

export function generateMountsJson(outputPath: string): void {
  const entries = fs.readdirSync(testDataDir, { withFileTypes: true });
  const mounts = entries
    .filter((e) => e.isDirectory())
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((e) => ({
      mount_id: crypto.randomBytes(8).toString("hex").slice(0, 16),
      name: e.name,
      slug: e.name,
      host_path: path.join(testDataDir, e.name),
    }));

  const config = { version: 2, mounts };
  const dir = path.dirname(outputPath);
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  fs.writeFileSync(outputPath, JSON.stringify(config, null, 2) + "\n");
  console.log(`E2E mounts.json 生成: ${mounts.length} マウント → ${outputPath}`);
}
