import { describe, expect, test } from "vitest";
import {
  buildBrowseSearch,
  buildCloseImageSearch,
  buildClosePdfSearch,
  buildOpenImageSearch,
  buildOpenPdfSearch,
} from "../../src/utils/viewerNavigation";

describe("buildOpenImageSearch", () => {
  test("tab=images と index を設定し pdf/page を削除する", () => {
    const cur = new URLSearchParams("pdf=abc&page=3&mode=manga");
    const next = buildOpenImageSearch(cur, { index: 5 });
    expect(next.get("tab")).toBe("images");
    expect(next.get("index")).toBe("5");
    expect(next.get("pdf")).toBeNull();
    expect(next.get("page")).toBeNull();
    expect(next.get("mode")).toBe("manga");
  });

  test("tab override が渡された場合はその値を使う", () => {
    const next = buildOpenImageSearch(new URLSearchParams(), { index: 0, tab: "filesets" });
    expect(next.get("tab")).toBe("filesets");
  });
});

describe("buildOpenPdfSearch", () => {
  test("pdf/page を設定し index/tab を削除する", () => {
    const cur = new URLSearchParams("tab=images&index=10");
    const next = buildOpenPdfSearch(cur, { pdfNodeId: "pdf-123" });
    expect(next.get("pdf")).toBe("pdf-123");
    expect(next.get("page")).toBe("1");
    expect(next.get("index")).toBeNull();
    expect(next.get("tab")).toBeNull();
  });

  test("page が指定された場合はその値を使う", () => {
    const next = buildOpenPdfSearch(new URLSearchParams(), { pdfNodeId: "p", page: 7 });
    expect(next.get("page")).toBe("7");
  });
});

describe("buildCloseImageSearch", () => {
  test("index のみを削除する", () => {
    const cur = new URLSearchParams("index=5&mode=manga&tab=images");
    const next = buildCloseImageSearch(cur);
    expect(next.get("index")).toBeNull();
    expect(next.get("mode")).toBe("manga");
    expect(next.get("tab")).toBe("images");
  });
});

describe("buildClosePdfSearch", () => {
  test("pdf/page を削除する", () => {
    const cur = new URLSearchParams("pdf=x&page=2&mode=manga");
    const next = buildClosePdfSearch(cur);
    expect(next.get("pdf")).toBeNull();
    expect(next.get("page")).toBeNull();
    expect(next.get("mode")).toBe("manga");
  });
});

describe("buildBrowseSearch", () => {
  test("viewer スコープを除外し mode/tab/sort を保持する", () => {
    const cur = new URLSearchParams("mode=manga&tab=images&sort=date-desc&pdf=x&page=3&index=5");
    expect(buildBrowseSearch(cur)).toBe("?mode=manga&tab=images&sort=date-desc");
  });

  test("overrides の tab/index を優先する", () => {
    const cur = new URLSearchParams("mode=manga");
    expect(buildBrowseSearch(cur, { tab: "videos", index: 2 })).toBe(
      "?mode=manga&tab=videos&index=2",
    );
  });

  test("デフォルト値は URL に残さない", () => {
    const cur = new URLSearchParams("sort=name-asc&tab=filesets");
    expect(buildBrowseSearch(cur)).toBe("");
  });
});
