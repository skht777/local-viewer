// ファイルブラウザーページ
// - isViewerOpen=false: BrowseHeader + ViewerTabs + DirectoryTree + FileBrowser
// - isViewerOpen=true && mode=cg: CgViewer
// - isViewerOpen=true && mode=manga: MangaViewer
// - ディレクトリ内の画像のみがビューワーの表示範囲

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
import { ViewerTabs } from "../components/ViewerTabs";

export default function BrowsePage() {
  const { nodeId } = useParams<{ nodeId: string }>();
  const navigate = useNavigate();
  const isSidebarOpen = useViewerStore((s) => s.isSidebarOpen);
  const { params, setTab, setMode, isViewerOpen, openViewer, closeViewer, setIndex } =
    useViewerParams();

  // 現在のディレクトリのデータ
  const { data, isLoading } = useQuery(browseNodeOptions(nodeId));

  // ルート一覧 (ツリー用)
  const { data: rootData } = useQuery(browseRootOptions());

  // 現在のディレクトリ内の画像エントリのみ（CgViewer の表示範囲）
  const images = useMemo(
    () => (data?.entries ?? []).filter((e) => e.kind === "image"),
    [data?.entries],
  );

  // ビューワー表示中
  if (isViewerOpen && images.length > 0) {
    const safeIndex = Math.max(0, Math.min(params.index, images.length - 1));

    // マンガモード: 縦スクロールビューワー
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
          onModeChange={setMode}
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
        onModeChange={setMode}
        onClose={closeViewer}
      />
    );
  }

  return (
    <div className="flex min-h-screen flex-col">
      <BrowseHeader currentName={data?.current_name ?? ""} />
      <ViewerTabs activeTab={params.tab} onTabChange={setTab} />
      <div className="flex flex-1 overflow-hidden">
        {isSidebarOpen && rootData && (
          <DirectoryTree rootEntries={rootData.entries} activeNodeId={nodeId ?? ""} />
        )}
        <FileBrowser
          entries={data?.entries ?? []}
          isLoading={isLoading}
          onNavigate={(id) => navigate(`/browse/${id}`)}
          onImageClick={openViewer}
          onTabChange={setTab}
          tab={params.tab}
        />
      </div>
    </div>
  );
}
