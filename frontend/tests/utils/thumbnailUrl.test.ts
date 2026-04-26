import { thumbnailUrl } from "../../src/utils/thumbnailUrl";

describe("thumbnailUrl", () => {
  test("node_id からサムネイルURLを生成する", () => {
    expect(thumbnailUrl("abc123")).toBe("/api/thumbnail/abc123");
  });

  test("スラッシュやエンコードが不要な node_id をそのまま使用する", () => {
    expect(thumbnailUrl("node-xyz_456")).toBe("/api/thumbnail/node-xyz_456");
  });

  test("modifiedAt 指定時に ?v= パラメータが付与される", () => {
    expect(thumbnailUrl("abc123", 1_700_000_000)).toBe("/api/thumbnail/abc123?v=1700000000");
  });

  test("modifiedAt が小数の場合は整数に切り捨てる", () => {
    expect(thumbnailUrl("abc123", 1_700_000_000.789)).toBe("/api/thumbnail/abc123?v=1700000000");
  });

  test("modifiedAt が null の場合はバージョンなし", () => {
    expect(thumbnailUrl("abc123", null)).toBe("/api/thumbnail/abc123");
  });

  test("modifiedAt が undefined の場合はバージョンなし", () => {
    expect(thumbnailUrl("abc123", undefined)).toBe("/api/thumbnail/abc123");
  });
});
