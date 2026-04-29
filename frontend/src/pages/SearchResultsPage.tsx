// 検索結果ブラウズページ
// - URL: /search?q=...&scope=...&kind=...&sort=...&index=...&mode=...&pdf=...&page=...
// - データ層は useSearchResultsData、callback は useSearchResultsCallbacks に委譲
// - viewer は BrowsePageViewerSwitch を再利用、画像セットは常に名前昇順
// - viewerOrigin は { pathname: "/search", search } で保存（B キー閉じで /search に戻る）
// - directory/archive の ▶/Space は useOpenViewerFromEntry に委譲（buildOrigin で /search 起点を保存）
// - viewerJumpList は viewer 起動直前 snapshot で設定（cleanup や mount clear はしない）

import { useCallback, useMemo } from "react";
import { useSearchParams } from "react-router-dom";
import { useViewerParams } from "../hooks/useViewerParams";
import { useViewerStore } from "../stores/viewerStore";
import { useOpenViewerFromEntry } from "../hooks/useOpenViewerFromEntry";
import { useSearchResultsData } from "../hooks/useSearchResultsData";
import { useSearchResultsCallbacks } from "../hooks/useSearchResultsCallbacks";
import { BrowsePageViewerSwitch } from "../components/BrowsePageViewerSwitch";
import { FileBrowser } from "../components/FileBrowser";
import { SearchBar } from "../components/SearchBar";
import { buildJumpListFromSearch } from "../lib/jumpListNavigation";
import { compareEntryName } from "../utils/sortEntries";
import { buildSearchSearch } from "../utils/viewerNavigation";

import type { SearchSort } from "../hooks/api/browseQueries";

// kind フィルタは FileBrowser のタブと別管理（検索 API の kind パラメータに直結）
const KIND_TABS: { label: string; value: string | null }[] = [
  { label: "すべて", value: null },
  { label: "画像", value: "image" },
  { label: "動画", value: "video" },
  { label: "PDF", value: "pdf" },
  { label: "アーカイブ", value: "archive" },
  { label: "ディレクトリ", value: "directory" },
];

export default function SearchResultsPage() {
  const viewerTransitionId = useViewerStore((s) => s.viewerTransitionId);
  const [searchParams] = useSearchParams();
  const {
    params,
    isViewerOpen,
    isPdfViewerOpen,
    setIndex,
    setPdfPage,
    closeViewer,
    closePdfViewer,
    buildBrowseSearch,
  } = useViewerParams();

  const {
    q,
    scope,
    kind,
    sort,
    isLoading,
    hasNextPage,
    fetchNextPage,
    isFetchingNextPage,
    isError,
    allEntries,
    allRawResults,
    scopeName,
  } = useSearchResultsData();

  // ビューワー画像セット: 画像のみ + 名前昇順固定
  const viewerImages = useMemo(
    () => allEntries.filter((e) => e.kind === "image").toSorted(compareEntryName),
    [allEntries],
  );

  // FileBrowser の onImageClick は filteredImages (image kind だけのフィルタ) 基準 index を渡す
  const filteredImages = useMemo(() => allEntries.filter((e) => e.kind === "image"), [allEntries]);

  // viewerIndexMap: filteredImages 基準 → viewerImages 昇順 index への変換
  const viewerIndexMap = useMemo(() => {
    const map = new Map<string, number>();
    viewerImages.forEach((img, idx) => map.set(img.node_id, idx));
    return map;
  }, [viewerImages]);

  const { handleImageClick, handlePdfClick, handleKindChange, handleSortChange, handleNavigate } =
    useSearchResultsCallbacks({ filteredImages, viewerIndexMap, allRawResults });

  const setViewerJumpList = useViewerStore((s) => s.setViewerJumpList);

  // ▶/Space で directory/archive/PDF/image をビューワー起動する callback
  // - nodeId=undefined: フック内部の /browse/${nodeId} 形 origin 保存を抑止
  // - buildOrigin: first-viewable 解決成功後に /search origin を保存（fallback 経路で残らない）
  // - buildBrowseSearch: 画像 viewer の URL に ?index=0&tab=images を付ける
  const baseOpenViewer = useOpenViewerFromEntry({
    nodeId: undefined,
    mode: params.mode,
    sort: "name-asc",
    buildBrowseSearch,
    buildOrigin: () => ({
      pathname: "/search",
      search: buildSearchSearch(searchParams),
    }),
  });

  // viewer 起動直前に検索結果の DAP を jumpList として snapshot
  // - 画像 entry の起動経路は jumpList=null にして既存 FS 経路で動作させる
  const openViewerFromEntry = useCallback(
    async (entryNodeId: string) => {
      const entry = allEntries.find((e) => e.node_id === entryNodeId);
      if (
        entry &&
        (entry.kind === "directory" || entry.kind === "archive" || entry.kind === "pdf")
      ) {
        setViewerJumpList(buildJumpListFromSearch(allRawResults));
      } else {
        setViewerJumpList(null);
      }
      await baseOpenViewer(entryNodeId);
    },
    [baseOpenViewer, allEntries, allRawResults, setViewerJumpList],
  );

  // ビューワー用 data 形状（BrowsePageViewerSwitch が要求）
  const viewerData = useMemo(
    () => ({
      current_name: q,
      current_node_id: null,
      parent_node_id: null,
      ancestors: [],
      entries: allEntries,
    }),
    [q, allEntries],
  );

  // ビューワー（PDF / 画像 / トランジション）を先に判定
  if (isPdfViewerOpen || viewerTransitionId > 0 || (isViewerOpen && viewerImages.length > 0)) {
    return (
      <BrowsePageViewerSwitch
        nodeId={undefined}
        data={viewerData}
        mode={params.mode}
        sort="name-asc"
        isPdfViewerOpen={isPdfViewerOpen}
        isViewerOpen={isViewerOpen}
        pdfNodeId={params.pdfNodeId}
        pdfPage={params.pdfPage}
        index={params.index}
        viewerTransitionId={viewerTransitionId}
        viewerImages={viewerImages}
        setIndex={setIndex}
        setPdfPage={setPdfPage}
        closeViewer={closeViewer}
        closePdfViewer={closePdfViewer}
      />
    );
  }

  return (
    <div className="flex h-screen flex-col">
      <header className="flex flex-col gap-3 border-b border-surface-overlay bg-surface-base px-6 py-4">
        <div className="flex items-baseline gap-3">
          <h1 className="text-lg font-semibold text-white">検索結果</h1>
          {q && (
            <span className="text-sm text-gray-400">
              「<span className="text-blue-300">{q}</span>」
            </span>
          )}
          {scope && scopeName && (
            <span className="text-xs text-gray-500" data-testid="search-scope-label">
              フォルダ内: {scopeName}
            </span>
          )}
        </div>
        <div className="max-w-2xl">
          <SearchBar scope={scope ?? undefined} />
        </div>
        <div className="flex items-center gap-3">
          <div className="flex gap-1" data-testid="search-kind-tabs">
            {KIND_TABS.map((tab) => (
              <button
                key={tab.value ?? "all"}
                type="button"
                onClick={() => handleKindChange(tab.value)}
                data-testid={`search-kind-${tab.value ?? "all"}`}
                className={`rounded px-3 py-1 text-sm ${
                  kind === tab.value
                    ? "bg-blue-600 text-white"
                    : "bg-surface-raised text-gray-400 hover:bg-surface-overlay"
                }`}
              >
                {tab.label}
              </button>
            ))}
          </div>
          <select
            value={sort}
            onChange={(e) => handleSortChange(e.target.value as SearchSort)}
            data-testid="search-sort-select"
            className="ml-auto rounded bg-surface-raised px-3 py-1 text-sm text-white"
          >
            <option value="relevance">関連度</option>
            <option value="name-asc">名前昇順</option>
            <option value="name-desc">名前降順</option>
            <option value="date-asc">日付昇順</option>
            <option value="date-desc">日付降順</option>
          </select>
        </div>
      </header>
      <div className="flex flex-1 overflow-hidden">
        <FileBrowser
          entries={allEntries}
          isLoading={isLoading}
          onNavigate={handleNavigate}
          onImageClick={handleImageClick}
          onPdfClick={handlePdfClick}
          onOpenViewer={openViewerFromEntry}
          tab={kind === "image" ? "images" : kind === "video" ? "videos" : "filesets"}
          sort="name-asc"
          hasMore={hasNextPage}
          isLoadingMore={isFetchingNextPage}
          isError={isError}
          onLoadMore={fetchNextPage}
        />
      </div>
    </div>
  );
}
