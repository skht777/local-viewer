// ディレクトリツリー (左サイドバー)
// - ルートノードから再帰的にツリーを表示
// - 展開時のみ子ノードを fetch (lazy loading)
// - ディレクトリ/アーカイブのみ表示

import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { browseNodeOptions } from "../hooks/api/browseQueries";
import { useViewerStore } from "../stores/viewerStore";
import type { BrowseEntry } from "../types/api";

interface DirectoryTreeProps {
  rootEntries: BrowseEntry[];
  activeNodeId: string;
}

interface TreeNodeProps {
  entry: BrowseEntry;
  depth: number;
  activeNodeId: string;
}

function TreeNode({ entry, depth, activeNodeId }: TreeNodeProps) {
  const navigate = useNavigate();
  const { expandedNodeIds, toggleExpanded } = useViewerStore();
  const isExpanded = expandedNodeIds.has(entry.node_id);
  const isActive = entry.node_id === activeNodeId;

  // 展開中のノードのみ子ノードを取得
  const { data: childData } = useQuery({
    ...browseNodeOptions(entry.node_id),
    enabled: isExpanded,
  });

  const handleClick = () => {
    toggleExpanded(entry.node_id);
    navigate(`/browse/${entry.node_id}`);
  };

  // ディレクトリ/アーカイブのみ子ノードを表示
  const childDirs =
    childData?.entries.filter(
      (e) => e.kind === "directory" || e.kind === "archive" || e.kind === "pdf",
    ) ?? [];

  return (
    <div>
      <button
        type="button"
        data-testid={`tree-node-${entry.node_id}`}
        onClick={handleClick}
        className={`flex w-full items-center gap-1 px-2 py-1 text-left text-sm transition-colors hover:bg-surface-raised ${
          isActive ? "bg-surface-raised text-white" : "text-gray-300"
        }`}
        style={{ paddingLeft: `${depth * 16 + 8}px` }}
      >
        <span className="w-4 text-xs text-gray-500">{isExpanded ? "▾" : "▸"}</span>
        <span className="truncate">{entry.name}</span>
      </button>
      {isExpanded &&
        childDirs.map((child) => (
          <TreeNode
            key={child.node_id}
            entry={child}
            depth={depth + 1}
            activeNodeId={activeNodeId}
          />
        ))}
    </div>
  );
}

export function DirectoryTree({ rootEntries, activeNodeId }: DirectoryTreeProps) {
  // ディレクトリ/アーカイブのみ表示
  const directories = rootEntries.filter(
    (e) => e.kind === "directory" || e.kind === "archive" || e.kind === "pdf",
  );

  return (
    <aside className="w-64 shrink-0 overflow-y-auto border-r border-white/5 bg-surface-card">
      <div className="p-2 text-xs font-medium uppercase tracking-wider text-gray-500">
        ディレクトリ
      </div>
      {directories.map((entry) => (
        <TreeNode key={entry.node_id} entry={entry} depth={0} activeNodeId={activeNodeId} />
      ))}
    </aside>
  );
}
