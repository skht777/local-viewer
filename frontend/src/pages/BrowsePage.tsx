// ファイルブラウザーページ
// - isPdfViewerOpen: PdfCgViewer / PdfMangaViewer
// - isViewerOpen: CgViewer / MangaViewer (画像)
// - ブラウズモード: BrowseHeader + ViewerTabs + DirectoryTree + FileBrowser/VideoFeed
// - PDF クリック → openPdfViewer で PDF ビューワーを開く
// - データ取得・タブ自動切替・ソート操作・フォーカスエリアは個別 hooks へ委譲

import { useCallback, useMemo, useRef } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { mountListOptions } from "../hooks/api/mountQueries";
import { useBrowseInfiniteData } from "../hooks/useBrowseInfiniteData";
import { useBrowseSortHandlers } from "../hooks/useBrowseSortHandlers";
import { useBrowseTabAutoSwitch } from "../hooks/useBrowseTabAutoSwitch";
import { useBrowseTabAvailability } from "../hooks/useBrowseTabAvailability";
import { useFocusAreaSwitcher } from "../hooks/useFocusAreaSwitcher";
import { useOpenViewerFromEntry } from "../hooks/useOpenViewerFromEntry";
import { useViewerImages } from "../hooks/useViewerImages";
import { useViewerParams } from "../hooks/useViewerParams";
import { useViewerStore } from "../stores/viewerStore";
import { BrowseHeader } from "../components/BrowseHeader";
import { BrowsePageViewerSwitch } from "../components/BrowsePageViewerSwitch";
import { DirectoryTree } from "../components/DirectoryTree";
import { FileBrowser } from "../components/FileBrowser";
import { VideoFeed } from "../components/VideoFeed";
import { ViewerTabs } from "../components/ViewerTabs";

export default function BrowsePage() {
  const { nodeId } = useParams<{ nodeId: string }>();
  const navigate = useNavigate();
  const isSidebarOpen = useViewerStore((s) => s.isSidebarOpen);
  const viewerTransitionId = useViewerStore((s) => s.viewerTransitionId);
  const {
    params,
    setTab,
    setMode,
    setSort,
    isViewerOpen,
    isPdfViewerOpen,
    openViewer,
    closeViewer,
    openPdfViewer,
    closePdfViewer,
    setIndex,
    setPdfPage,
    buildBrowseSearch,
  } = useViewerParams();

  const openViewerFromEntry = useOpenViewerFromEntry({
    nodeId,
    mode: params.mode,
    sort: params.sort,
    buildBrowseSearch,
  });

  // 検索結果からの遷移で select パラメータが指定されている場合のハイライト用
  const [searchParams] = useSearchParams();
  const selectedNodeId = searchParams.get("select") ?? undefined;

  // ファイルブラウザー ↔ ツリーのフォーカスエリア管理
  const treeRef = useRef<HTMLElement>(null);
  const { focusArea, handleFocusTree, handleFocusBrowser } = useFocusAreaSwitcher({
    treeRef,
    nodeId,
  });

  // 現在のディレクトリのデータ (ページネーション対応 + viewerTransitionId 終了)
  const { data, isLoading, hasNextPage, fetchNextPage, isFetchingNextPage, isError } =
    useBrowseInfiniteData(nodeId, params.sort);

  // マウントポイント一覧 (ツリー用)
  const { data: mountData } = useQuery(mountListOptions());

  // タブ自動切替
  useBrowseTabAutoSwitch({ data, isLoading, currentTab: params.tab, setTab });

  // MountEntry → BrowseEntry に変換してツリーに渡す
  const rootEntries = useMemo(
    () =>
      (mountData?.mounts ?? []).map((m) => ({
        node_id: m.node_id,
        name: m.name,
        kind: "directory" as const,
        size_bytes: null,
        mime_type: null,
        child_count: m.child_count,
        modified_at: null,
        preview_node_ids: null,
      })),
    [mountData?.mounts],
  );

  // 祖先ノード ID 配列（ツリー自動展開用）
  const ancestorNodeIds = useMemo(
    () => (data?.ancestors ?? []).map((a) => a.node_id),
    [data?.ancestors],
  );

  // 画像配列とビューワー起動関連の派生値
  const { images, viewerImages, openViewerNameSorted } = useViewerImages(data?.entries, openViewer);

  // 動画エントリ（VideoFeed の表示対象）
  const videos = useMemo(
    () => (data?.entries ?? []).filter((e) => e.kind === "video"),
    [data?.entries],
  );

  // タブ可用性（コンテンツのないタブを disabled）
  const disabledTabs = useBrowseTabAvailability({ data, images, videos });

  // ソート/モードトグル
  const { handleSortName, handleSortDate, handleToggleMode } = useBrowseSortHandlers({
    sort: params.sort,
    mode: params.mode,
    setSort,
    setMode,
  });

  // ブラウズ間遷移の共通 callback
  // - 自分自身（現在 nodeId）への navigate は抑制し history 重複を防ぐ
  // - search は呼び出し側で構築する（mode/sort/tab を保持するため）
  const navigateBrowse = useCallback(
    (targetNodeId: string, search: string) => {
      if (targetNodeId === nodeId) {
        return;
      }
      navigate(`/browse/${targetNodeId}${search}`);
    },
    [navigate, nodeId],
  );

  // 親ディレクトリに戻る
  const handleGoParent = useCallback(() => {
    if (data?.parent_node_id) {
      navigateBrowse(data.parent_node_id, buildBrowseSearch());
    }
  }, [data?.parent_node_id, navigateBrowse, buildBrowseSearch]);

  // ビューワー (PDF / 画像 / トランジション) を先に判定し、該当する場合はそれを返す
  const viewerSwitch = (
    <BrowsePageViewerSwitch
      nodeId={nodeId}
      data={data}
      mode={params.mode}
      sort={params.sort}
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
  if (isPdfViewerOpen || viewerTransitionId > 0 || (isViewerOpen && viewerImages.length > 0)) {
    return viewerSwitch;
  }

  return (
    <div className="flex h-screen flex-col">
      <BrowseHeader
        currentName={data?.current_name ?? ""}
        ancestors={data?.ancestors ?? []}
        onBreadcrumbSelect={(id) => navigateBrowse(id, buildBrowseSearch())}
        mode={params.mode}
        onModeChange={setMode}
        nodeId={nodeId}
      />
      <ViewerTabs
        activeTab={params.tab}
        onTabChange={setTab}
        disabledTabs={disabledTabs}
        sort={params.sort}
        onSortChange={setSort}
      />
      <div className="flex flex-1 overflow-hidden">
        {isSidebarOpen && rootEntries.length > 0 && (
          <DirectoryTree
            ref={treeRef}
            rootEntries={rootEntries}
            activeNodeId={nodeId ?? ""}
            ancestorNodeIds={ancestorNodeIds}
            onNavigate={(id) => navigateBrowse(id, buildBrowseSearch())}
            onFocusBrowser={handleFocusBrowser}
            keyboardEnabled={focusArea === "tree"}
          />
        )}
        {params.tab === "videos" ? (
          <VideoFeed videos={videos} />
        ) : (
          <FileBrowser
            entries={data?.entries ?? []}
            isLoading={isLoading}
            onNavigate={(id, options) => {
              navigateBrowse(id, buildBrowseSearch({ tab: options?.tab }));
            }}
            onImageClick={openViewerNameSorted}
            onPdfClick={openPdfViewer}
            onOpenViewer={openViewerFromEntry}
            onGoParent={handleGoParent}
            onTabChange={setTab}
            onFocusTree={handleFocusTree}
            onToggleMode={handleToggleMode}
            onSortName={handleSortName}
            onSortDate={handleSortDate}
            tab={params.tab}
            sort={params.sort}
            selectedNodeId={selectedNodeId}
            keyboardEnabled={focusArea === "browser"}
            hasMore={hasNextPage}
            isLoadingMore={isFetchingNextPage}
            isError={isError}
            onLoadMore={fetchNextPage}
          />
        )}
      </div>
    </div>
  );
}
