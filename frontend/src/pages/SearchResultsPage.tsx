// 検索結果ブラウズページ
// - URL: /search?q=...&scope=...&kind=...&sort=...&index=...&mode=...&pdf=...&page=...
// - searchInfiniteOptions の結果を BrowseEntry に変換して FileBrowser で表示
// - viewer は BrowsePageViewerSwitch を再利用、画像セットは常に名前昇順
// - viewerOrigin は { pathname: "/search", search } で保存（B キー閉じで /search に戻る）

import { useCallback, useMemo } from "react";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useNavigate, useSearchParams } from "react-router-dom";
import { browseNodeOptions, searchInfiniteOptions } from "../hooks/api/browseQueries";
import type { SearchSort } from "../hooks/api/browseQueries";
import { useViewerParams } from "../hooks/useViewerParams";
import { useViewerStore } from "../stores/viewerStore";
import { BrowsePageViewerSwitch } from "../components/BrowsePageViewerSwitch";
import { FileBrowser } from "../components/FileBrowser";
import { SearchBar } from "../components/SearchBar";
import type { BrowseEntry } from "../types/api";
import type { ViewerTab } from "../utils/viewerNavigation";
import { compareEntryName } from "../utils/sortEntries";
import { searchResultToBrowseEntry } from "../utils/searchResultToBrowseEntry";

const VALID_SEARCH_SORTS = new Set<string>([
  "relevance",
  "name-asc",
  "name-desc",
  "date-asc",
  "date-desc",
]);

const VALID_KINDS = new Set<string>(["directory", "image", "video", "pdf", "archive"]);

// kind フィルタは FileBrowser のタブと別管理（検索 API の kind パラメータに直結）
// "all" が ViewerTabs と紐づかないため、シンプルなセレクタとして実装する
const KIND_TABS: { label: string; value: string | null }[] = [
  { label: "すべて", value: null },
  { label: "画像", value: "image" },
  { label: "動画", value: "video" },
  { label: "PDF", value: "pdf" },
  { label: "アーカイブ", value: "archive" },
  { label: "ディレクトリ", value: "directory" },
];

export default function SearchResultsPage() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const viewerTransitionId = useViewerStore((s) => s.viewerTransitionId);
  const {
    params,
    isViewerOpen,
    isPdfViewerOpen,
    setIndex,
    setPdfPage,
    closeViewer,
    closePdfViewer,
  } = useViewerParams();

  // URL から検索条件を取得
  const q = (searchParams.get("q") ?? "").trim();
  const scope = searchParams.get("scope") ?? null;
  const rawKind = searchParams.get("kind");
  const kind = rawKind && VALID_KINDS.has(rawKind) ? rawKind : null;
  const rawSort = searchParams.get("sort");
  const sort: SearchSort = (
    rawSort && VALID_SEARCH_SORTS.has(rawSort) ? rawSort : "relevance"
  ) as SearchSort;

  // scope 配下の場合、ディレクトリ名を表示するために browseNodeOptions で取得
  const { data: scopeData } = useQuery(browseNodeOptions(scope ?? undefined));

  // 検索結果（無限スクロール）
  const {
    data: searchData,
    isLoading,
    hasNextPage,
    fetchNextPage,
    isFetchingNextPage,
    isError,
  } = useInfiniteQuery(searchInfiniteOptions({ q, scope, kind, sort }));

  // 検索結果を BrowseEntry に変換
  const allEntries: BrowseEntry[] = useMemo(() => {
    if (!searchData?.pages?.length) {
      return [];
    }
    return searchData.pages.flatMap((p) => p.results.map(searchResultToBrowseEntry));
  }, [searchData]);

  // ビューワー画像セット: 画像のみ + 名前昇順固定
  const viewerImages = useMemo(
    () => allEntries.filter((e) => e.kind === "image").sort(compareEntryName),
    [allEntries],
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

  // FileBrowser の onImageClick: ブラウズ順 index → 名前昇順 viewerImages の index に変換
  // BrowsePage と異なり tab=images では list が allEntries の image だけのフィルタ済み相当なので
  // FileBrowser の filterByTab の tab="images" 結果と一致させるため filteredImages を使う
  const filteredImages = useMemo(() => allEntries.filter((e) => e.kind === "image"), [allEntries]);

  const viewerIndexMap = useMemo(() => {
    const map = new Map<string, number>();
    viewerImages.forEach((img, idx) => map.set(img.node_id, idx));
    return map;
  }, [viewerImages]);

  const handleImageClick = useCallback(
    (browseIndex: number) => {
      const img = filteredImages[browseIndex];
      if (!img) {
        return;
      }
      const viewerIdx = viewerIndexMap.get(img.node_id) ?? 0;
      // useViewerParams.openViewer は computeOrigin で /search を検出
      // ここでは setIndex + tab=images を直接設定する代わりに openViewer 相当を呼ぶ
      // useViewerParams が export していないので自前で URL を組み立てる
      const next = new URLSearchParams(searchParams);
      next.set("tab", "images");
      next.set("index", String(viewerIdx));
      next.delete("pdf");
      next.delete("page");
      // origin 保存
      useViewerStore.getState().setViewerOrigin({
        pathname: "/search",
        search: searchParams.toString() ? `?${searchParams.toString()}` : "",
      });
      setSearchParams(next);
    },
    [filteredImages, viewerIndexMap, searchParams, setSearchParams],
  );

  const handlePdfClick = useCallback(
    (pdfNodeId: string) => {
      const next = new URLSearchParams(searchParams);
      next.set("pdf", pdfNodeId);
      next.set("page", "1");
      next.delete("index");
      next.delete("tab");
      useViewerStore.getState().setViewerOrigin({
        pathname: "/search",
        search: searchParams.toString() ? `?${searchParams.toString()}` : "",
      });
      setSearchParams(next);
    },
    [searchParams, setSearchParams],
  );

  // kind タブの切替: URL の kind を更新（既定値 null は削除）
  const handleKindChange = useCallback(
    (newKind: string | null) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          if (newKind) {
            next.set("kind", newKind);
          } else {
            next.delete("kind");
          }
          // viewer 関連はリセット
          next.delete("index");
          next.delete("pdf");
          next.delete("page");
          next.delete("tab");
          return next;
        },
        { replace: true },
      );
    },
    [setSearchParams],
  );

  // sort セレクタ
  const handleSortChange = useCallback(
    (newSort: SearchSort) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          if (newSort === "relevance") {
            next.delete("sort");
          } else {
            next.set("sort", newSort);
          }
          return next;
        },
        { replace: true },
      );
    },
    [setSearchParams],
  );

  // ナビゲーション（FileBrowser の onNavigate）: directory/archive クリックで /browse に遷移
  const handleNavigate = useCallback(
    (id: string, options?: { tab?: ViewerTab }) => {
      const tab = options?.tab;
      const browseSearch = tab && tab !== "filesets" ? `?tab=${tab}` : "";
      navigate(`/browse/${id}${browseSearch}`);
    },
    [navigate],
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

  // ヘッダー: 検索キーワード / scope 名 / 検索バー
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
          {scope && scopeData?.current_name && (
            <span className="text-xs text-gray-500" data-testid="search-scope-label">
              フォルダ内: {scopeData.current_name}
            </span>
          )}
        </div>
        <div className="max-w-2xl">
          <SearchBar scope={scope ?? undefined} />
        </div>
        {/* kind フィルタ + sort */}
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
