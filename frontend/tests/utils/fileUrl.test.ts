// fileUrl の振る舞い検証
// - modifiedAt 指定時: ?v={整数} を付与（ファイル差し替え時の HTTP キャッシュ無効化）
// - modifiedAt が null/undefined: バージョンなし（ETag フォールバック）

import { fileUrl } from "../../src/utils/fileUrl";

describe("fileUrl", () => {
  test("modifiedAt 指定時に ?v= 付き URL を返す", () => {
    expect(fileUrl("abc123", 1234.567)).toBe("/api/file/abc123?v=1234");
  });

  test("modifiedAt が整数のときそのまま v に使う", () => {
    expect(fileUrl("abc123", 1700000000)).toBe("/api/file/abc123?v=1700000000");
  });

  test("modifiedAt が null のときバージョンなし", () => {
    expect(fileUrl("abc123", null)).toBe("/api/file/abc123");
  });

  test("modifiedAt 省略時もバージョンなし", () => {
    expect(fileUrl("abc123")).toBe("/api/file/abc123");
  });
});
