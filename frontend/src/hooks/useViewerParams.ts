// URL 状態管理フック
// - useSearchParams をラップして型安全にアクセス
// - URL が Single Source of Truth
// - mode は browse スコープ: "manga" のみ URL に書き込み、"cg"(デフォルト) は省略
// - sort は browse スコープ: "name-asc"(デフォルト) は省略、他は URL に書き込み
// - index/pdf/page は viewer スコープ: ビューワー close で削除
// - pdf と index は排他: openPdfViewer で index/tab 削除、openViewer で pdf/page 削除
//
// URL 構築の pure ロジックは utils/viewerNavigation.ts に分離し、
// 本 hook は副作用（setSearchParams / navigate / viewerOrigin の更新）のみを担当する。

import { useLocation, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { useViewerStore } from "../stores/viewerStore";
import { updateSearchParams } from "../utils/searchParamUpdater";
import {
  buildBrowseSearch as buildBrowseSearchPure,
  buildCloseImageSearch,
  buildClosePdfSearch,
  buildOpenImageSearch,
  buildOpenPdfSearch,
  buildSearchSearch,
  VALID_SORT_ORDERS,
} from "../utils/viewerNavigation";
import type { SortOrder, ViewerMode, ViewerTab } from "../utils/viewerNavigation";

export type { SortOrder, ViewerMode, ViewerTab } from "../utils/viewerNavigation";

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
  const location = useLocation();
  const [searchParams, setSearchParams] = useSearchParams();
  const setViewerOrigin = useViewerStore((s) => s.setViewerOrigin);
  const viewerOrigin = useViewerStore((s) => s.viewerOrigin);

  // viewer 起動時の起点情報を組み立てる
  // - /browse/:nodeId なら pathname=/browse/{nodeId}、search は browse スコープ（mode/tab/sort）
  // - /search なら pathname=location.pathname、search は検索スコープ（q/scope/kind/sort/mode）
  // - その他のルートは何もしない
  const computeOrigin = (): { pathname: string; search: string } | null => {
    if (nodeId) {
      return {
        pathname: `/browse/${nodeId}`,
        search: buildBrowseSearchPure(searchParams),
      };
    }
    if (location.pathname === "/search") {
      return {
        pathname: "/search",
        search: buildSearchSearch(searchParams),
      };
    }
    return null;
  };

  const tab = (searchParams.get("tab") ?? "filesets") as ViewerTab;
  const indexParam = searchParams.get("index");
  const index = indexParam === null ? -1 : parseInt(indexParam, 10);
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
    !isPdfViewerOpen &&
    indexParam !== null &&
    tab === "images" &&
    (mode === "cg" || mode === "manga");

  const buildBrowseSearch = (overrides?: { tab?: string; index?: number }): string =>
    buildBrowseSearchPure(searchParams, overrides);

  const setTab = (newTab: ViewerTab) => {
    setSearchParams((prev) =>
      updateSearchParams(prev, (next) => {
        next.set("tab", newTab);
      }),
    );
  };

  const setIndex = (newIndex: number) => {
    setSearchParams(
      (prev) =>
        updateSearchParams(prev, (next) => {
          next.set("index", String(newIndex));
        }),
      { replace: true },
    );
  };

  // mode 正規化: "manga" のみ URL に書き込み、"cg" は省略（デフォルト）
  const setMode = (newMode: ViewerMode) => {
    setSearchParams(
      (prev) =>
        updateSearchParams(prev, (next) => {
          if (newMode === "manga") {
            next.set("mode", "manga");
          } else {
            next.delete("mode");
          }
        }),
      { replace: true },
    );
  };

  // sort 正規化: "name-asc"(デフォルト) は省略、他は URL に書き込み
  const setSort = (newSort: SortOrder) => {
    setSearchParams(
      (prev) =>
        updateSearchParams(prev, (next) => {
          if (newSort === "name-asc") {
            next.delete("sort");
          } else {
            next.set("sort", newSort);
          }
        }),
      { replace: true },
    );
  };

  // 画像ビューワーを開く: tab=images + index を設定、pdf/page を削除
  // mode は browse スコープで管理済みなのでここでは操作しない
  // push モード: ブラウザバックで開く前の URL に戻れるようにする（B キー閉じと一致）
  const openViewer = (newIndex: number) => {
    // 起点情報を保存（B キー閉じる時に戻る先）
    const origin = computeOrigin();
    if (origin) {
      setViewerOrigin(origin);
    }
    setSearchParams((prev) => buildOpenImageSearch(prev, { index: newIndex }));
  };

  // 画像ビューワーを閉じる: 起点に戻るか、履歴を1つ戻る
  const closeViewer = () => {
    if (viewerOrigin) {
      const origin = viewerOrigin;
      setViewerOrigin(null);
      navigate(`${origin.pathname}${origin.search}`, { replace: true });
    } else {
      // deep link 等で origin がない場合: 現在ディレクトリに留まる
      setSearchParams(buildCloseImageSearch);
    }
  };

  // PDF ビューワーを開く: pdf/page を設定、index/tab を削除
  // mode は browse スコープで管理済みなのでここでは操作しない
  // push モード: ブラウザバックで開く前の URL に戻れるようにする（B キー閉じと一致）
  const openPdfViewer = (nextPdfNodeId: string) => {
    const origin = computeOrigin();
    if (origin) {
      setViewerOrigin(origin);
    }
    setSearchParams((prev) => buildOpenPdfSearch(prev, { pdfNodeId: nextPdfNodeId }));
  };

  // PDF ビューワーを閉じる: 起点に戻るか、現在ディレクトリに留まる
  const closePdfViewer = () => {
    if (viewerOrigin) {
      const origin = viewerOrigin;
      setViewerOrigin(null);
      navigate(`${origin.pathname}${origin.search}`, { replace: true });
    } else {
      setSearchParams(buildClosePdfSearch);
    }
  };

  // PDF ページ番号を更新
  const setPdfPage = (page: number) => {
    setSearchParams(
      (prev) =>
        updateSearchParams(prev, (next) => {
          next.set("page", String(page));
        }),
      { replace: true },
    );
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
