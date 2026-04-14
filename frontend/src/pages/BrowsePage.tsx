// ファイルブラウザーページ
// - isPdfViewerOpen: PdfCgViewer / PdfMangaViewer
// - isViewerOpen: CgViewer / MangaViewer (画像)
// - ブラウズモード: BrowseHeader + ViewerTabs + DirectoryTree + FileBrowser/VideoFeed
// - PDF クリック → openPdfViewer で PDF ビューワーを開く

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { browseInfiniteOptions } from "../hooks/api/browseQueries";
import { mountListOptions } from "../hooks/api/mountQueries";
import { useViewerParams, type ViewerTab } from "../hooks/useViewerParams";
import { useViewerStore } from "../stores/viewerStore";
import { BrowseHeader } from "../components/BrowseHeader";
import { CgViewer } from "../components/CgViewer";
import { DirectoryTree } from "../components/DirectoryTree";
import { FileBrowser } from "../components/FileBrowser";
import { MangaViewer } from "../components/MangaViewer";
import { PdfCgViewer } from "../components/PdfCgViewer";
import { PdfMangaViewer } from "../components/PdfMangaViewer";
import { VideoFeed } from "../components/VideoFeed";
import { ViewerTabs } from "../components/ViewerTabs";
import { resolveFirstViewable } from "../utils/resolveFirstViewable";

// ソート方向反転マップ
const SORT_FLIP = {
  "name-asc": "name-desc",
  "name-desc": "name-asc",
  "date-asc": "date-desc",
  "date-desc": "date-asc",
} as const;

export default function BrowsePage() {
  const { nodeId } = useParams<{ nodeId: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const isSidebarOpen = useViewerStore((s) => s.isSidebarOpen);
  const viewerTransitionId = useViewerStore((s) => s.viewerTransitionId);
  const endViewerTransition = useViewerStore((s) => s.endViewerTransition);
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

  // 検索結果からの遷移で select パラメータが指定されている場合のハイライト用
  const [searchParams] = useSearchParams();
  const selectedNodeId = searchParams.get("select") ?? undefined;

  // ファイルブラウザー ↔ ツリーのフォーカスエリア管理
  const [focusArea, setFocusArea] = useState<"browser" | "tree">("browser");
  const treeRef = useRef<HTMLElement>(null);

  // 現在のディレクトリのデータ (ページネーション対応)
  const {
    data: infiniteData,
    isLoading,
    hasNextPage,
    fetchNextPage,
    isFetchingNextPage,
    isError,
  } = useInfiniteQuery(browseInfiniteOptions(nodeId, params.sort));

  // 全ページの entries を結合し、メタデータは先頭ページから取得
  const data = useMemo(() => {
    if (!infiniteData?.pages?.length) return undefined;
    const first = infiniteData.pages[0];
    const allEntries = infiniteData.pages.flatMap((p) => p.entries);
    return {
      ...first,
      entries: allEntries,
    };
  }, [infiniteData]);

  // セットジャンプのトランジション完了: データ到着でクリア
  useEffect(() => {
    if (viewerTransitionId > 0 && data && !isLoading) {
      endViewerTransition(viewerTransitionId);
    }
  }, [viewerTransitionId, data, isLoading, endViewerTransition]);

  // マウントポイント一覧 (ツリー用)
  const { data: mountData } = useQuery(mountListOptions());

  // タブ自動切替: 現在のタブが空なら最適なタブに切替
  // 優先順位: filesets > images > videos
  // 現在タブにコンテンツがあればそのまま維持
  useEffect(() => {
    if (!data || isLoading) return;

    const hasFilesets = data.entries.some(
      (e) => e.kind === "directory" || e.kind === "archive" || e.kind === "pdf",
    );
    const hasImages = data.entries.some((e) => e.kind === "image");
    const hasVideos = data.entries.some((e) => e.kind === "video");

    // 現在のタブにコンテンツがあればそのまま
    if (params.tab === "filesets" && hasFilesets) return;
    if (params.tab === "images" && hasImages) return;
    if (params.tab === "videos" && hasVideos) return;

    // 現在のタブが空 → 最適なタブに自動切替
    // すべて空の場合は現在タブに留まる
    if (hasFilesets) {
      setTab("filesets");
    } else if (hasImages) {
      setTab("images");
    } else if (hasVideos) {
      setTab("videos");
    }
  }, [data, isLoading, params.tab, setTab]);

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

  // 現在のディレクトリ内の画像エントリのみ（CgViewer の表示範囲）
  const images = useMemo(
    () => (data?.entries ?? []).filter((e) => e.kind === "image"),
    [data?.entries],
  );

  // 動画エントリ（VideoFeed の表示対象）
  const videos = useMemo(
    () => (data?.entries ?? []).filter((e) => e.kind === "video"),
    [data?.entries],
  );

  // コンテンツのないタブを disabled にする
  // 全て空の場合は filesets のみ有効（デフォルトタブ）
  const disabledTabs = useMemo(() => {
    if (!data) return new Set<ViewerTab>();
    const disabled = new Set<ViewerTab>();
    const hasFilesets = data.entries.some(
      (e) => e.kind === "directory" || e.kind === "archive" || e.kind === "pdf",
    );
    if (!hasFilesets) disabled.add("filesets");
    if (images.length === 0) disabled.add("images");
    if (videos.length === 0) disabled.add("videos");
    if (disabled.size === 3) disabled.delete("filesets");
    return disabled;
  }, [data, images.length, videos.length]);

  // ソートトグルロジック（名前/更新日）
  const handleSortName = useCallback(() => {
    setSort(params.sort.startsWith("name") ? SORT_FLIP[params.sort] : "name-asc");
  }, [params.sort, setSort]);

  const handleSortDate = useCallback(() => {
    setSort(params.sort.startsWith("date") ? SORT_FLIP[params.sort] : "date-desc");
  }, [params.sort, setSort]);

  // モード切替（CG ↔ マンガ）
  const handleToggleMode = useCallback(() => {
    setMode(params.mode === "cg" ? "manga" : "cg");
  }, [params.mode, setMode]);

  // 親ディレクトリに戻る
  const handleGoParent = useCallback(() => {
    if (data?.parent_node_id) {
      navigate(`/browse/${data.parent_node_id}${buildBrowseSearch()}`);
    }
  }, [data?.parent_node_id, navigate, buildBrowseSearch]);

  // ツリーにフォーカス移動（現在のディレクトリのノードにフォーカス）
  const handleFocusTree = useCallback(() => {
    setFocusArea("tree");
    const activeNode = treeRef.current?.querySelector<HTMLElement>(`[data-node-id="${nodeId}"]`);
    const fallback = treeRef.current?.querySelector<HTMLElement>("[data-testid^='tree-node-']");
    (activeNode ?? fallback)?.focus();
  }, [nodeId]);

  // ブラウザーにフォーカス移動（ツリーから呼ばれる）
  const handleFocusBrowser = useCallback(() => {
    setFocusArea("browser");
    const selectedCard = document.querySelector<HTMLElement>("[aria-current='true']");
    selectedCard?.focus();
  }, []);

  // PDF ファイル名を entries から取得
  const pdfEntry = useMemo(
    () => (data?.entries ?? []).find((e) => e.node_id === params.pdfNodeId),
    [data?.entries, params.pdfNodeId],
  );

  // PDF ビューワー表示中 (画像ビューワーより先に判定)
  if (isPdfViewerOpen && params.pdfNodeId) {
    const pdfName = pdfEntry?.name ?? "";
    const commonProps = {
      pdfNodeId: params.pdfNodeId,
      pdfName,
      parentNodeId: data?.current_node_id ?? nodeId ?? null,
      ancestors: data?.ancestors,
      initialPage: params.pdfPage,
      mode: params.mode,
      sort: params.sort,
      onPageChange: setPdfPage,
      onClose: closePdfViewer,
    };
    if (params.mode === "manga") {
      return <PdfMangaViewer {...commonProps} />;
    }
    return <PdfCgViewer {...commonProps} />;
  }

  // セットジャンプのトランジション中: ローディングオーバーレイを表示
  if (viewerTransitionId > 0) {
    return (
      <div
        data-testid="viewer-transition"
        className="fixed inset-0 z-50 flex items-center justify-center bg-black"
      >
        <div className="text-gray-400">読み込み中...</div>
      </div>
    );
  }

  // 画像ビューワー表示中
  if (isViewerOpen && images.length > 0) {
    const safeIndex = Math.max(0, Math.min(params.index, images.length - 1));

    if (params.mode === "manga") {
      return (
        <MangaViewer
          images={images}
          currentIndex={safeIndex}
          setName={data?.current_name ?? ""}
          parentNodeId={data?.parent_node_id ?? null}
          currentNodeId={data?.current_node_id ?? null}
          ancestors={data?.ancestors}
          mode={params.mode}
          sort={params.sort}
          onIndexChange={setIndex}
          onClose={closeViewer}
        />
      );
    }

    return (
      <CgViewer
        images={images}
        currentIndex={safeIndex}
        setName={data?.current_name ?? ""}
        parentNodeId={data?.parent_node_id ?? null}
        currentNodeId={data?.current_node_id ?? null}
        ancestors={data?.ancestors}
        mode={params.mode}
        sort={params.sort}
        onIndexChange={setIndex}
        onClose={closeViewer}
      />
    );
  }

  return (
    <div className="flex h-screen flex-col">
      <BrowseHeader
        currentName={data?.current_name ?? ""}
        ancestors={data?.ancestors ?? []}
        onBreadcrumbSelect={(id) => navigate(`/browse/${id}${buildBrowseSearch()}`)}
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
            onNavigate={(id) => navigate(`/browse/${id}${buildBrowseSearch()}`)}
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
              navigate(`/browse/${id}${buildBrowseSearch({ tab: options?.tab })}`);
            }}
            onImageClick={openViewer}
            onPdfClick={openPdfViewer}
            onOpenViewer={async (id) => {
              // ディレクトリ内を再帰探索し、最初の閲覧対象を見つけてビューワーを開く
              try {
                const target = await resolveFirstViewable(id, queryClient, params.sort);
                if (!target) {
                  navigate(`/browse/${id}${buildBrowseSearch()}`);
                  return;
                }

                if (target.entry.kind === "pdf") {
                  // PDF: 親ディレクトリでPDFビューワーを開く (browse スコープを保持)
                  const sp = new URLSearchParams();
                  sp.set("pdf", target.entry.node_id);
                  sp.set("page", "1");
                  if (params.mode === "manga") sp.set("mode", "manga");
                  if (params.sort !== "name-asc") sp.set("sort", params.sort);
                  navigate(`/browse/${target.parentNodeId}?${sp}`);
                } else if (target.entry.kind === "image") {
                  // 画像: 親ディレクトリでビューワーを開く
                  navigate(
                    `/browse/${target.parentNodeId}${buildBrowseSearch({ tab: "images", index: 0 })}`,
                  );
                } else {
                  // アーカイブ: BrowsePage の infinite キャッシュにプリフェッチしてから進入
                  await queryClient.prefetchInfiniteQuery(
                    browseInfiniteOptions(target.entry.node_id, params.sort),
                  );
                  navigate(
                    `/browse/${target.entry.node_id}${buildBrowseSearch({ tab: "images", index: 0 })}`,
                  );
                }
              } catch {
                // エラー時は進入にフォールバック
                navigate(`/browse/${id}${buildBrowseSearch()}`);
              }
            }}
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
