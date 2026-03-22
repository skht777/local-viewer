// ファイルブラウザーページ
// - 上: BrowseHeader (ナビゲーション) + ViewerTabs (タブバー)
// - 左: DirectoryTree (サイドバー)
// - 右: FileBrowser (メインエリア、タブでフィルタ)

import { useQuery } from "@tanstack/react-query";
import { useNavigate, useParams } from "react-router-dom";
import { browseNodeOptions, browseRootOptions } from "../hooks/api/browseQueries";
import { useViewerParams } from "../hooks/useViewerParams";
import { useViewerStore } from "../stores/viewerStore";
import { BrowseHeader } from "../components/BrowseHeader";
import { DirectoryTree } from "../components/DirectoryTree";
import { FileBrowser } from "../components/FileBrowser";
import { ViewerTabs } from "../components/ViewerTabs";

export default function BrowsePage() {
  const { nodeId } = useParams<{ nodeId: string }>();
  const navigate = useNavigate();
  const isSidebarOpen = useViewerStore((s) => s.isSidebarOpen);
  const { params, setTab } = useViewerParams();

  // 現在のディレクトリのデータ
  const { data, isLoading } = useQuery(browseNodeOptions(nodeId));

  // ルート一覧 (ツリー用)
  const { data: rootData } = useQuery(browseRootOptions());

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
          tab={params.tab}
        />
      </div>
    </div>
  );
}
