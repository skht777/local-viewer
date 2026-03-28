// URL 状態管理フック
// - useSearchParams をラップして型安全にアクセス
// - URL が Single Source of Truth
// - mode は browse スコープ: "manga" のみ URL に書き込み、"cg"(デフォルト) は省略
// - index/pdf/page は viewer スコープ: ビューワー close で削除
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
  buildBrowseSearch: (overrides?: { tab?: string }) => string;
}

export function useViewerParams(): UseViewerParamsReturn {
  const [searchParams, setSearchParams] = useSearchParams();

  const tab = (searchParams.get("tab") ?? "filesets") as ViewerTab;
  const hasIndex = searchParams.has("index");
  const index = hasIndex ? parseInt(searchParams.get("index")!, 10) : -1;
  // 不正値ガード: "manga" 以外はすべて "cg" に正規化
  const rawMode = searchParams.get("mode");
  const mode: ViewerMode = rawMode === "manga" ? "manga" : "cg";
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

  // mode 正規化: "manga" のみ URL に書き込み、"cg" は省略（デフォルト）
  const setMode = (newMode: ViewerMode) => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (newMode === "manga") {
          next.set("mode", "manga");
        } else {
          next.delete("mode");
        }
        return next;
      },
      { replace: true },
    );
  };

  // 画像ビューワーを開く: tab=images + index を設定、pdf/page を削除
  // mode は browse スコープで管理済みなのでここでは操作しない
  // push モード: ブラウザ Back でビューワー状態に復帰可能
  const openViewer = (newIndex: number) => {
    setSearchParams((prev) => {
      const next = new URLSearchParams(prev);
      next.set("tab", "images");
      next.set("index", String(newIndex));
      // PDF パラメータを排他的に削除
      next.delete("pdf");
      next.delete("page");
      return next;
    });
  };

  // 画像ビューワーを閉じる: viewer スコープ (index) のみ削除
  // push モード: ブラウザ Back でビューワー状態に復帰可能
  const closeViewer = () => {
    setSearchParams((prev) => {
      const next = new URLSearchParams(prev);
      next.delete("index");
      return next;
    });
  };

  // PDF ビューワーを開く: pdf/page を設定、index/tab を削除
  // mode は browse スコープで管理済みなのでここでは操作しない
  // push モード: ブラウザ Back でビューワー状態に復帰可能
  const openPdfViewer = (nodeId: string) => {
    setSearchParams((prev) => {
      const next = new URLSearchParams(prev);
      next.set("pdf", nodeId);
      next.set("page", "1");
      // 画像ビューワーパラメータを排他的に削除
      next.delete("index");
      next.delete("tab");
      return next;
    });
  };

  // PDF ビューワーを閉じる: viewer スコープ (pdf/page) のみ削除
  // push モード: ブラウザ Back でビューワー状態に復帰可能
  const closePdfViewer = () => {
    setSearchParams((prev) => {
      const next = new URLSearchParams(prev);
      next.delete("pdf");
      next.delete("page");
      return next;
    });
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

  // browse スコープのパラメータを維持し、viewer スコープだけ除外した search 文字列を返す
  // ディレクトリ遷移時に mode/tab を引き継ぐために使用
  const buildBrowseSearch = (overrides?: { tab?: string }): string => {
    const next = new URLSearchParams();
    const currentMode = searchParams.get("mode");
    if (currentMode === "manga") next.set("mode", "manga");
    const nextTab = overrides?.tab ?? searchParams.get("tab");
    if (nextTab && nextTab !== "filesets") next.set("tab", nextTab);
    return next.toString() ? `?${next}` : "";
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
    buildBrowseSearch,
  };
}
