// URL 状態管理フック
// - useSearchParams をラップして型安全にアクセス
// - URL が Single Source of Truth

import { useSearchParams } from "react-router-dom";

export type ViewerTab = "filesets" | "images" | "videos";
export type ViewerMode = "cg" | "manga";

interface ViewerParams {
  tab: ViewerTab;
  index: number;
  mode: ViewerMode;
}

interface UseViewerParamsReturn {
  params: ViewerParams;
  isViewerOpen: boolean;
  setTab: (tab: ViewerTab) => void;
  setIndex: (index: number) => void;
  setMode: (mode: ViewerMode) => void;
  openViewer: (index: number) => void;
  closeViewer: () => void;
}

export function useViewerParams(): UseViewerParamsReturn {
  const [searchParams, setSearchParams] = useSearchParams();

  const tab = (searchParams.get("tab") ?? "filesets") as ViewerTab;
  const hasIndex = searchParams.has("index");
  const index = hasIndex ? parseInt(searchParams.get("index")!, 10) : -1;
  const mode = (searchParams.get("mode") ?? "cg") as ViewerMode;

  // ビューワーは images タブ + 有効な mode + index 指定時のみ開く
  const isViewerOpen = hasIndex && tab === "images" && (mode === "cg" || mode === "manga");

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

  // ビューワーを開く: tab=images に切り替え、index と mode を設定
  const openViewer = (newIndex: number) => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("tab", "images");
        next.set("index", String(newIndex));
        next.set("mode", next.get("mode") ?? "cg");
        return next;
      },
      { replace: true },
    );
  };

  // ビューワーを閉じる: index と mode を URL から削除
  const closeViewer = () => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.delete("index");
        next.delete("mode");
        return next;
      },
      { replace: true },
    );
  };

  return {
    params: { tab, index, mode },
    isViewerOpen,
    setTab,
    setIndex,
    setMode,
    openViewer,
    closeViewer,
  };
}
