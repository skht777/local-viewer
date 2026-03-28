import { formatPageLabel } from "../../src/utils/formatPageLabel";

describe("formatPageLabel", () => {
  test("セット名ありの単ページ表示", () => {
    expect(formatPageLabel("photos", 3, 12)).toBe("photos 3 / 12");
  });

  test("セット名なしの単ページ表示", () => {
    expect(formatPageLabel("", 1, 5)).toBe("1 / 5");
  });

  test("見開き時の範囲表示", () => {
    expect(formatPageLabel("album", 3, 12, 4)).toBe("album 3-4 / 12");
  });

  test("currentEnd が current と同じ場合は範囲表示しない", () => {
    expect(formatPageLabel("set", 3, 10, 3)).toBe("set 3 / 10");
  });
});
