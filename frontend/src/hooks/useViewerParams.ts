// URL 状態管理フック
// - useSearchParams をラップして型安全にアクセス
// - URL が Single Source of Truth
// - pdf と index は排他: openPdfViewer で index/tab 削除、openViewer で pdf/page 削除

import { useSearchParams } from "react-router-dom";

export type ViewerTab = "filesets" | "images" | "videos";
export type ViewerMode = "cg" | "manga";

interface ViewerParams {
  tab: ViewerTab;
  index: number;
  mode: ViewerMode;
  pdfNodeId: string | null;
  pdfPage: number;
}

interface UseViewerParamsReturn {
  params: ViewerParams;
  isViewerOpen: boolean;
  isPdfViewerOpen: boolean;
  setTab: (tab: ViewerTab) => void;
  setIndex: (index: number) => void;
  setMode: (mode: ViewerMode) => void;
  openViewer: (index: number) => void;
  closeViewer: () => void;
  openPdfViewer: (nodeId: string) => void;
  closePdfViewer: () => void;
  setPdfPage: (page: number) => void;
}

export function useViewerParams(): UseViewerParamsReturn {
  const [searchParams, setSearchParams] = useSearchParams();

  const tab = (searchParams.get("tab") ?? "filesets") as ViewerTab;
  const hasIndex = searchParams.has("index");
  const index = hasIndex ? parseInt(searchParams.get("index")!, 10) : -1;
  const mode = (searchParams.get("mode") ?? "cg") as ViewerMode;
  const pdfNodeId = searchParams.get("pdf") ?? null;
  const pdfPage = parseInt(searchParams.get("page") ?? "1", 10) || 1;

  // PDF が開いていたら画像ビューワーは開かない (排他)
  const isPdfViewerOpen = pdfNodeId !== null && (mode === "cg" || mode === "manga");
  const isViewerOpen =
    !isPdfViewerOpen && hasIndex && tab === "images" && (mode === "cg" || mode === "manga");

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

  // 画像ビューワーを開く: tab=images + index + mode を設定、pdf/page を削除
  const openViewer = (newIndex: number) => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("tab", "images");
        next.set("index", String(newIndex));
        next.set("mode", next.get("mode") ?? "cg");
        // PDF パラメータを排他的に削除
        next.delete("pdf");
        next.delete("page");
        return next;
      },
      { replace: true },
    );
  };

  // 画像ビューワーを閉じる: index と mode を URL から削除
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

  // PDF ビューワーを開く: pdf/page/mode を設定、index/tab を削除
  const openPdfViewer = (nodeId: string) => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("pdf", nodeId);
        next.set("page", "1");
        next.set("mode", next.get("mode") ?? "cg");
        // 画像ビューワーパラメータを排他的に削除
        next.delete("index");
        next.delete("tab");
        return next;
      },
      { replace: true },
    );
  };

  // PDF ビューワーを閉じる: pdf/page/mode を URL から削除
  const closePdfViewer = () => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.delete("pdf");
        next.delete("page");
        next.delete("mode");
        return next;
      },
      { replace: true },
    );
  };

  // PDF ページ番号を更新
  const setPdfPage = (page: number) => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("page", String(page));
        return next;
      },
      { replace: true },
    );
  };

  return {
    params: { tab, index, mode, pdfNodeId, pdfPage },
    isViewerOpen,
    isPdfViewerOpen,
    setTab,
    setIndex,
    setMode,
    openViewer,
    closeViewer,
    openPdfViewer,
    closePdfViewer,
    setPdfPage,
  };
}
