// useBrowseSortHandlers の振る舞い検証
// - handleSortName: 同じ name 軸なら asc/desc を反転、別軸なら "name-asc"
// - handleSortDate: 同じ date 軸なら反転、別軸なら "date-desc"（新着順がデフォルト）
// - handleToggleMode: cg ↔ manga の切替

import { renderHook } from "@testing-library/react";
import { useBrowseSortHandlers } from "../../src/hooks/useBrowseSortHandlers";
import type { SortOrder, ViewerMode } from "../../src/hooks/useViewerParams";

interface Setup {
  sort: SortOrder;
  mode: ViewerMode;
}

function setup({ sort, mode }: Setup) {
  const setSort = vi.fn();
  const setMode = vi.fn();
  const { result } = renderHook(() => useBrowseSortHandlers({ sort, mode, setSort, setMode }));
  return { result, setSort, setMode };
}

describe("useBrowseSortHandlers", () => {
  describe("handleSortName", () => {
    test("name-asc → name-desc に反転する", () => {
      const { result, setSort } = setup({ sort: "name-asc", mode: "cg" });
      result.current.handleSortName();
      expect(setSort).toHaveBeenCalledWith("name-desc");
    });

    test("name-desc → name-asc に反転する", () => {
      const { result, setSort } = setup({ sort: "name-desc", mode: "cg" });
      result.current.handleSortName();
      expect(setSort).toHaveBeenCalledWith("name-asc");
    });

    test("date 軸からは name-asc に切り替わる（既定方向）", () => {
      const { result, setSort } = setup({ sort: "date-desc", mode: "cg" });
      result.current.handleSortName();
      expect(setSort).toHaveBeenCalledWith("name-asc");
    });
  });

  describe("handleSortDate", () => {
    test("date-asc → date-desc に反転する", () => {
      const { result, setSort } = setup({ sort: "date-asc", mode: "cg" });
      result.current.handleSortDate();
      expect(setSort).toHaveBeenCalledWith("date-desc");
    });

    test("date-desc → date-asc に反転する", () => {
      const { result, setSort } = setup({ sort: "date-desc", mode: "cg" });
      result.current.handleSortDate();
      expect(setSort).toHaveBeenCalledWith("date-asc");
    });

    test("name 軸からは date-desc に切り替わる（新着順がデフォルト）", () => {
      const { result, setSort } = setup({ sort: "name-asc", mode: "cg" });
      result.current.handleSortDate();
      expect(setSort).toHaveBeenCalledWith("date-desc");
    });
  });

  describe("handleToggleMode", () => {
    test("cg → manga に切り替わる", () => {
      const { result, setMode } = setup({ sort: "name-asc", mode: "cg" });
      result.current.handleToggleMode();
      expect(setMode).toHaveBeenCalledWith("manga");
    });

    test("manga → cg に切り替わる", () => {
      const { result, setMode } = setup({ sort: "name-asc", mode: "manga" });
      result.current.handleToggleMode();
      expect(setMode).toHaveBeenCalledWith("cg");
    });
  });
});
