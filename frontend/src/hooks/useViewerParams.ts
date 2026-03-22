// URL 状態管理フック
// - useSearchParams をラップして型安全にアクセス
// - URL が Single Source of Truth

import { useSearchParams } from "react-router-dom";

export type ViewerTab = "images" | "videos";
export type ViewerMode = "cg" | "manga";

interface ViewerParams {
  tab: ViewerTab;
  index: number;
  mode: ViewerMode;
}

interface UseViewerParamsReturn {
  params: ViewerParams;
  setTab: (tab: ViewerTab) => void;
  setIndex: (index: number) => void;
  setMode: (mode: ViewerMode) => void;
}

export function useViewerParams(): UseViewerParamsReturn {
  const [searchParams, setSearchParams] = useSearchParams();

  const tab = (searchParams.get("tab") ?? "images") as ViewerTab;
  const index = parseInt(searchParams.get("index") ?? "0", 10);
  const mode = (searchParams.get("mode") ?? "cg") as ViewerMode;

  const setTab = (newTab: ViewerTab) => {
    setSearchParams((prev) => {
      const next = new URLSearchParams(prev);
      next.set("tab", newTab);
      return next;
    });
  };

  const setIndex = (newIndex: number) => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("index", String(newIndex));
        return next;
      },
      { replace: true },
    );
  };

  const setMode = (newMode: ViewerMode) => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("mode", newMode);
        return next;
      },
      { replace: true },
    );
  };

  return {
    params: { tab, index, mode },
    setTab,
    setIndex,
    setMode,
  };
}
