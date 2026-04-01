// ディレクトリツリー (左サイドバー)
// - ルートノードから再帰的にツリーを表示
// - 展開時のみ子ノードを fetch (lazy loading)
// - ディレクトリ/アーカイブのみ表示
// - onNavigate コールバックで URL 組み立ては呼び出し元 (BrowsePage) に委譲
// - ancestorNodeIds で現在パスの祖先を自動展開
// - アクティブノードを scrollIntoView で表示範囲にスクロール

import type { Ref } from "react";
import { useEffect, useRef } from "react";
import { useQuery } from "@tanstack/react-query";
import { browseNodeOptions } from "../hooks/api/browseQueries";
import { useViewerStore } from "../stores/viewerStore";
import type { BrowseEntry } from "../types/api";

interface DirectoryTreeProps {
  rootEntries: BrowseEntry[];
  activeNodeId: string;
  ancestorNodeIds: string[];
  onNavigate: (nodeId: string) => void;
  onFocusBrowser?: () => void;
  ref?: Ref<HTMLElement>;
}

interface TreeNodeProps {
  entry: BrowseEntry;
  depth: number;
  activeNodeId: string;
  ancestorNodeIds: string[];
  onNavigate: (nodeId: string) => void;
}

function TreeNode({ entry, depth, activeNodeId, ancestorNodeIds, onNavigate }: TreeNodeProps) {
  const { expandedNodeIds, toggleExpanded } = useViewerStore();
  const buttonRef = useRef<HTMLButtonElement>(null);

  // 手動展開 ∪ 祖先自動展開
  const isExpanded = expandedNodeIds.has(entry.node_id) || ancestorNodeIds.includes(entry.node_id);
  const isActive = entry.node_id === activeNodeId;

  // アクティブノードを表示範囲にスクロール
  useEffect(() => {
    if (isActive && buttonRef.current) {
      buttonRef.current.scrollIntoView({ block: "nearest", behavior: "smooth" });
    }
  }, [isActive]);

  // 展開中のノードのみ子ノードを取得
  const { data: childData } = useQuery({
    ...browseNodeOptions(entry.node_id),
    enabled: isExpanded,
  });

  const handleClick = () => {
    toggleExpanded(entry.node_id);
    onNavigate(entry.node_id);
  };

  // ディレクトリ/アーカイブのみ子ノードを表示
  const childDirs =
    childData?.entries.filter(
      (e) => e.kind === "directory" || e.kind === "archive" || e.kind === "pdf",
    ) ?? [];

  return (
    <div>
      <button
        ref={buttonRef}
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
            ancestorNodeIds={ancestorNodeIds}
            onNavigate={onNavigate}
          />
        ))}
    </div>
  );
}

export function DirectoryTree({
  rootEntries,
  activeNodeId,
  ancestorNodeIds,
  onNavigate,
  onFocusBrowser,
  ref,
}: DirectoryTreeProps) {
  // ディレクトリ/アーカイブのみ表示
  const directories = rootEntries.filter(
    (e) => e.kind === "directory" || e.kind === "archive" || e.kind === "pdf",
  );

  return (
    <aside
      ref={ref}
      className="w-64 shrink-0 overflow-y-auto border-r border-white/5 bg-surface-card"
    >
      <div className="p-2 text-xs font-medium uppercase tracking-wider text-gray-500">
        ディレクトリ
      </div>
      {directories.map((entry) => (
        <TreeNode
          key={entry.node_id}
          entry={entry}
          depth={0}
          activeNodeId={activeNodeId}
          ancestorNodeIds={ancestorNodeIds}
          onNavigate={onNavigate}
        />
      ))}
    </aside>
  );
}
