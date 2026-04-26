// ソート/モードトグルの handler 群（同じ軸内で asc/desc を反転、別軸なら既定方向に切替）

import { useCallback } from "react";
import type { SortOrder, ViewerMode } from "./useViewerParams";

const SORT_FLIP = {
  "name-asc": "name-desc",
  "name-desc": "name-asc",
  "date-asc": "date-desc",
  "date-desc": "date-asc",
} as const;

interface UseBrowseSortHandlersParams {
  sort: SortOrder;
  mode: ViewerMode;
  setSort: (sort: SortOrder) => void;
  setMode: (mode: ViewerMode) => void;
}

interface UseBrowseSortHandlersResult {
  handleSortName: () => void;
  handleSortDate: () => void;
  handleToggleMode: () => void;
}

export function useBrowseSortHandlers({
  sort,
  mode,
  setSort,
  setMode,
}: UseBrowseSortHandlersParams): UseBrowseSortHandlersResult {
  const handleSortName = useCallback(() => {
    setSort(sort.startsWith("name") ? SORT_FLIP[sort] : "name-asc");
  }, [sort, setSort]);

  const handleSortDate = useCallback(() => {
    setSort(sort.startsWith("date") ? SORT_FLIP[sort] : "date-desc");
  }, [sort, setSort]);

  const handleToggleMode = useCallback(() => {
    setMode(mode === "cg" ? "manga" : "cg");
  }, [mode, setMode]);

  return { handleSortName, handleSortDate, handleToggleMode };
}
