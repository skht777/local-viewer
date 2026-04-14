// URL 状態管理フック
// - useSearchParams をラップして型安全にアクセス
// - URL が Single Source of Truth
// - mode は browse スコープ: "manga" のみ URL に書き込み、"cg"(デフォルト) は省略
// - sort は browse スコープ: "name-asc"(デフォルト) は省略、他は URL に書き込み
// - index/pdf/page は viewer スコープ: ビューワー close で削除
// - pdf と index は排他: openPdfViewer で index/tab 削除、openViewer で pdf/page 削除

import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { useViewerStore } from "../stores/viewerStore";

export type ViewerTab = "filesets" | "images" | "videos";
export type ViewerMode = "cg" | "manga";
export type SortOrder = "name-asc" | "name-desc" | "date-asc" | "date-desc";

const VALID_SORT_ORDERS: Set<string> = new Set(["name-asc", "name-desc", "date-asc", "date-desc"]);

interface ViewerParams {
  tab: ViewerTab;
  index: number;
  mode: ViewerMode;
  sort: SortOrder;
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
  setSort: (sort: SortOrder) => void;
  openViewer: (index: number) => void;
  closeViewer: () => void;
  openPdfViewer: (nodeId: string) => void;
  closePdfViewer: () => void;
  setPdfPage: (page: number) => void;
  buildBrowseSearch: (overrides?: { tab?: string; index?: number }) => string;
}

export function useViewerParams(): UseViewerParamsReturn {
  const { nodeId } = useParams<{ nodeId: string }>();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const setViewerOrigin = useViewerStore((s) => s.setViewerOrigin);
  const viewerOrigin = useViewerStore((s) => s.viewerOrigin);

  const tab = (searchParams.get("tab") ?? "filesets") as ViewerTab;
  const hasIndex = searchParams.has("index");
  const index = hasIndex ? parseInt(searchParams.get("index")!, 10) : -1;
  // 不正値ガード: "manga" 以外はすべて "cg" に正規化
  const rawMode = searchParams.get("mode");
  const mode: ViewerMode = rawMode === "manga" ? "manga" : "cg";
  // 不正値ガード: 有効な SortOrder 以外は "name-asc" に正規化
  const rawSort = searchParams.get("sort");
  const sort: SortOrder =
    rawSort && VALID_SORT_ORDERS.has(rawSort) ? (rawSort as SortOrder) : "name-asc";
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

  // sort 正規化: "name-asc"(デフォルト) は省略、他は URL に書き込み
  const setSort = (newSort: SortOrder) => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (newSort === "name-asc") {
          next.delete("sort");
        } else {
          next.set("sort", newSort);
        }
        return next;
      },
      { replace: true },
    );
  };

  // 画像ビューワーを開く: tab=images + index を設定、pdf/page を削除
  // mode は browse スコープで管理済みなのでここでは操作しない
  // replace モード: ブラウザ Back で前のページに戻る
  const openViewer = (newIndex: number) => {
    // 起点情報を保存（閉じる時に戻る先）
    if (nodeId) {
      setViewerOrigin({ nodeId, search: buildBrowseSearch() });
    }
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("tab", "images");
        next.set("index", String(newIndex));
        next.delete("pdf");
        next.delete("page");
        return next;
      },
      { replace: true },
    );
  };

  // 画像ビューワーを閉じる: 起点に戻るか、履歴を1つ戻る
  const closeViewer = () => {
    if (viewerOrigin) {
      const origin = viewerOrigin;
      setViewerOrigin(null);
      navigate(`/browse/${origin.nodeId}${origin.search}`, { replace: true });
    } else {
      // deep link 等で origin がない場合: 現在ディレクトリに留まる
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev);
        next.delete("index");
        return next;
      });
    }
  };

  // PDF ビューワーを開く: pdf/page を設定、index/tab を削除
  // mode は browse スコープで管理済みなのでここでは操作しない
  // replace モード: ブラウザ Back で前のページに戻る
  const openPdfViewer = (pdfNodeId: string) => {
    if (nodeId) {
      setViewerOrigin({ nodeId, search: buildBrowseSearch() });
    }
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("pdf", pdfNodeId);
        next.set("page", "1");
        next.delete("index");
        next.delete("tab");
        return next;
      },
      { replace: true },
    );
  };

  // PDF ビューワーを閉じる: 起点に戻るか、現在ディレクトリに留まる
  const closePdfViewer = () => {
    if (viewerOrigin) {
      const origin = viewerOrigin;
      setViewerOrigin(null);
      navigate(`/browse/${origin.nodeId}${origin.search}`, { replace: true });
    } else {
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev);
        next.delete("pdf");
        next.delete("page");
        return next;
      });
    }
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
  const buildBrowseSearch = (overrides?: { tab?: string; index?: number }): string => {
    const next = new URLSearchParams();
    const currentMode = searchParams.get("mode");
    if (currentMode === "manga") next.set("mode", "manga");
    const nextTab = overrides?.tab ?? searchParams.get("tab");
    if (nextTab && nextTab !== "filesets") next.set("tab", nextTab);
    const currentSort = searchParams.get("sort");
    if (currentSort && VALID_SORT_ORDERS.has(currentSort) && currentSort !== "name-asc") {
      next.set("sort", currentSort);
    }
    if (overrides?.index != null) next.set("index", String(overrides.index));
    return next.toString() ? `?${next}` : "";
  };

  return {
    params: { tab, index, mode, sort, pdfNodeId, pdfPage },
    isViewerOpen,
    isPdfViewerOpen,
    setTab,
    setIndex,
    setMode,
    setSort,
    openViewer,
    closeViewer,
    openPdfViewer,
    closePdfViewer,
    setPdfPage,
    buildBrowseSearch,
  };
}
