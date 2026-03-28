// ファイルブラウザーページ
// - isPdfViewerOpen: PdfCgViewer / PdfMangaViewer
// - isViewerOpen: CgViewer / MangaViewer (画像)
// - ブラウズモード: BrowseHeader + ViewerTabs + DirectoryTree + FileBrowser/VideoFeed
// - PDF クリック → openPdfViewer で PDF ビューワーを開く

import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate, useParams } from "react-router-dom";
import { browseNodeOptions, browseRootOptions } from "../hooks/api/browseQueries";
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

  // 現在のディレクトリのデータ
  const { data, isLoading } = useQuery(browseNodeOptions(nodeId));

  // ルート一覧 (ツリー用)
  const { data: rootData } = useQuery(browseRootOptions());

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
    <div className="flex min-h-screen flex-col">
      <BrowseHeader
        currentName={data?.current_name ?? ""}
        mode={params.mode}
        onModeChange={setMode}
      />
      <ViewerTabs activeTab={params.tab} onTabChange={setTab} />
      <div className="flex flex-1 overflow-hidden">
        {isSidebarOpen && rootData && (
          <DirectoryTree rootEntries={rootData.entries} activeNodeId={nodeId ?? ""} />
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
          />
        )}
      </div>
    </div>
  );
}
