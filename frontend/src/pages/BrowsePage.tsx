// ファイルブラウザーページ
// - 左: DirectoryTree (サイドバー)
// - 右: FileBrowser (メインエリア)
// - 上: BrowseHeader (ナビゲーション)

import { useQuery } from "@tanstack/react-query";
import { useNavigate, useParams } from "react-router-dom";
import { browseNodeOptions, browseRootOptions } from "../hooks/api/browseQueries";
import { useViewerStore } from "../stores/viewerStore";
import { BrowseHeader } from "../components/BrowseHeader";
import { DirectoryTree } from "../components/DirectoryTree";
import { FileBrowser } from "../components/FileBrowser";

export default function BrowsePage() {
  const { nodeId } = useParams<{ nodeId: string }>();
  const navigate = useNavigate();
  const isSidebarOpen = useViewerStore((s) => s.isSidebarOpen);

  // 現在のディレクトリのデータ
  const { data, isLoading } = useQuery(browseNodeOptions(nodeId));

  // ルート一覧 (ツリー用)
  const { data: rootData } = useQuery(browseRootOptions());

  return (
    <div className="flex min-h-screen flex-col">
      <BrowseHeader currentName={data?.current_name ?? ""} />
      <div className="flex flex-1 overflow-hidden">
        {isSidebarOpen && rootData && (
          <DirectoryTree rootEntries={rootData.entries} activeNodeId={nodeId ?? ""} />
        )}
        <FileBrowser
          entries={data?.entries ?? []}
          isLoading={isLoading}
          onNavigate={(id) => navigate(`/browse/${id}`)}
        />
      </div>
    </div>
  );
}
