// ファイルブラウザーページ
// - isPdfViewerOpen: PdfCgViewer / PdfMangaViewer
// - isViewerOpen: CgViewer / MangaViewer (画像)
// - ブラウズモード: BrowseHeader + ViewerTabs + DirectoryTree + FileBrowser/VideoFeed
// - PDF クリック → openPdfViewer で PDF ビューワーを開く

import { useEffect, useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { browseNodeOptions } from "../hooks/api/browseQueries";
import { mountListOptions } from "../hooks/api/mountQueries";
import { useViewerParams } from "../hooks/useViewerParams";
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

export default function BrowsePage() {
  const { nodeId } = useParams<{ nodeId: string }>();
  const navigate = useNavigate();
  const isSidebarOpen = useViewerStore((s) => s.isSidebarOpen);
  const {
    params,
    setTab,
    setMode,
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

  // 現在のディレクトリのデータ
  const { data, isLoading } = useQuery(browseNodeOptions(nodeId));

  // マウントポイント一覧 (ツリー用)
  const { data: mountData } = useQuery(mountListOptions());

  // タブ自動切替: filesets が空なら images > videos の優先順位で切替
  // すべて空の場合は filesets にフォールバック（何もしない）
  useEffect(() => {
    if (!data || isLoading) return;
    if (params.tab !== "filesets") return;

    const hasFilesets = data.entries.some(
      (e) => e.kind === "directory" || e.kind === "archive" || e.kind === "pdf",
    );
    if (hasFilesets) return;

    const hasImages = data.entries.some((e) => e.kind === "image");
    if (hasImages) {
      setTab("images");
      return;
    }

    const hasVideos = data.entries.some((e) => e.kind === "video");
    if (hasVideos) {
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
      })),
    [mountData?.mounts],
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
      initialPage: params.pdfPage,
      mode: params.mode,
      onPageChange: setPdfPage,
      onClose: closePdfViewer,
    };
    if (params.mode === "manga") {
      return <PdfMangaViewer {...commonProps} />;
    }
    return <PdfCgViewer {...commonProps} />;
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
          mode={params.mode}
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
        mode={params.mode}
        onIndexChange={setIndex}
        onClose={closeViewer}
      />
    );
  }

  return (
    <div className="flex h-screen flex-col">
      <BrowseHeader
        currentName={data?.current_name ?? ""}
        mode={params.mode}
        onModeChange={setMode}
      />
      <ViewerTabs activeTab={params.tab} onTabChange={setTab} />
      <div className="flex flex-1 overflow-hidden">
        {isSidebarOpen && rootEntries.length > 0 && (
          <DirectoryTree rootEntries={rootEntries} activeNodeId={nodeId ?? ""} />
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
            tab={params.tab}
            selectedNodeId={selectedNodeId}
            onTabChange={setTab}
          />
        )}
      </div>
    </div>
  );
}
