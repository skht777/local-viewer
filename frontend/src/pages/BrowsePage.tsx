// ファイルブラウザーページ
// - isViewerOpen=false: BrowseHeader + ViewerTabs + DirectoryTree + FileBrowser
// - isViewerOpen=true: CgViewer（フルスクリーンオーバーレイ）
// - ディレクトリ内の画像のみが CgViewer の表示範囲

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
import { ViewerTabs } from "../components/ViewerTabs";

export default function BrowsePage() {
  const { nodeId } = useParams<{ nodeId: string }>();
  const navigate = useNavigate();
  const isSidebarOpen = useViewerStore((s) => s.isSidebarOpen);
  const { params, setTab, isViewerOpen, openViewer, closeViewer, setIndex } = useViewerParams();

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

    // マンガモードは Phase 3 で実装
    if (params.mode === "manga") {
      return (
        <div className="flex min-h-screen items-center justify-center bg-black text-gray-400">
          <div className="text-center">
            <p className="mb-4 text-lg">マンガモード（Phase 3 で実装予定）</p>
            <button
              type="button"
              onClick={closeViewer}
              className="rounded bg-gray-700 px-4 py-2 text-white hover:bg-gray-600"
            >
              戻る
            </button>
          </div>
        </div>
      );
    }

    return (
      <CgViewer
        images={images}
        currentIndex={safeIndex}
        setName={data?.current_name ?? ""}
        parentNodeId={data?.parent_node_id ?? null}
        onIndexChange={setIndex}
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
          tab={params.tab}
        />
      </div>
    </div>
  );
}
