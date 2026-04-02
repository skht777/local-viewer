import { thumbnailUrl } from "../../src/utils/thumbnailUrl";

describe("thumbnailUrl", () => {
  test("node_id からサムネイルURLを生成する", () => {
    expect(thumbnailUrl("abc123")).toBe("/api/thumbnail/abc123");
  });

  test("スラッシュやエンコードが不要な node_id をそのまま使用する", () => {
    expect(thumbnailUrl("node-xyz_456")).toBe("/api/thumbnail/node-xyz_456");
  });
});
