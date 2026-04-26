// ディレクトリツリー (左サイドバー)
// - WAI-ARIA TreeView パターン準拠（role="tree", role="treeitem", aria-expanded）
// - ルートノードから再帰的にツリーを表示
// - 展開時のみ子ノードを fetch (lazy loading)
// - ディレクトリ/アーカイブ/PDFのみ表示
// - onNavigate コールバックで URL 組み立ては呼び出し元 (BrowsePage) に委譲
// - ancestorNodeIds で現在パスの祖先を自動展開
// - アクティブノードを scrollIntoView で表示範囲にスクロール
// - キーボード: ↑↓移動、→展開、←折りたたみ、Enter選択、Home/End、tでブラウザー切替

import type { Ref } from "react";
import { useCallback, useEffect, useRef } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { browseNodeOptions } from "../hooks/api/browseQueries";
import { useTreeKeyboard } from "../hooks/useTreeKeyboard";
import { useViewerStore } from "../stores/viewerStore";
import type { BrowseEntry } from "../types/api";

interface DirectoryTreeProps {
  rootEntries: BrowseEntry[];
  activeNodeId: string;
  ancestorNodeIds: string[];
  onNavigate: (nodeId: string) => void;
  onFocusBrowser?: () => void;
  keyboardEnabled?: boolean;
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
  const queryClient = useQueryClient();
  const hoverTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // 手動展開 ∪ 祖先自動展開
  const isExpanded = expandedNodeIds.has(entry.node_id) || ancestorNodeIds.includes(entry.node_id);
  const isActive = entry.node_id === activeNodeId;

  // アクティブノードを表示範囲にスクロール
  useEffect(() => {
    if (isActive && buttonRef.current) {
      buttonRef.current.scrollIntoView({ block: "nearest", behavior: "auto" });
    }
  }, [isActive]);

  // 展開中のノードのみ子ノードを取得
  const { data: childData } = useQuery({
    ...browseNodeOptions(entry.node_id),
    enabled: isExpanded,
  });

  // アイコンクリック: 展開/折りたたみのみ（ナビゲートしない）
  const handleChevronClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    toggleExpanded(entry.node_id);
  };

  // ラベルクリック: ナビゲート + 未展開なら展開（折りたたみはしない）
  const handleLabelClick = () => {
    if (!isExpanded) {
      toggleExpanded(entry.node_id);
    }
    onNavigate(entry.node_id);
  };

  // 200ms デバウンス付き hover プリフェッチ (マウスカーソル通過で無駄リクエストを抑制)
  const handlePointerEnter = () => {
    hoverTimerRef.current = setTimeout(() => {
      queryClient.prefetchQuery(browseNodeOptions(entry.node_id));
    }, 200);
  };
  const handlePointerLeave = () => {
    if (hoverTimerRef.current) {
      clearTimeout(hoverTimerRef.current);
      hoverTimerRef.current = null;
    }
  };
  // アンマウント時にタイマーをクリア (pointerLeave 未発火のまま消える場合のリーク防止)
  // oxlint-disable-next-line arrow-body-style
  useEffect(() => {
    return () => {
      if (hoverTimerRef.current) {
        clearTimeout(hoverTimerRef.current);
      }
    };
  }, []);

  // ディレクトリ/アーカイブ/PDFのみ子ノードを表示
  const childDirs =
    childData?.entries.filter(
      (e) => e.kind === "directory" || e.kind === "archive" || e.kind === "pdf",
    ) ?? [];

  return (
    <div role="treeitem" aria-expanded={isExpanded}>
      <button
        ref={buttonRef}
        type="button"
        data-testid={`tree-node-${entry.node_id}`}
        data-node-id={entry.node_id}
        onClick={handleLabelClick}
        onPointerEnter={handlePointerEnter}
        onPointerLeave={handlePointerLeave}
        className={`flex w-full items-center gap-1 py-1 pr-2 pl-[calc(var(--depth,0)*16px+8px)] text-left text-sm transition-colors hover:bg-surface-raised ${
          isActive ? "bg-surface-raised text-white" : "text-gray-300"
        }`}
        // CSS 変数注入のみ inline style (frontend-react.md Styling 例外条項準拠)
        style={{ "--depth": depth } as React.CSSProperties}
        tabIndex={-1}
      >
        {/* アイコン: クリックで展開/折りたたみのみ（非 tabbable） */}
        <span
          role="button"
          tabIndex={-1}
          onClick={handleChevronClick}
          className="w-4 text-xs text-gray-500"
        >
          {isExpanded ? "▾" : "▸"}
        </span>
        <span className="truncate">{entry.name}</span>
      </button>
      {isExpanded && childDirs.length > 0 && (
        <div role="group">
          {childDirs.map((child) => (
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
      )}
    </div>
  );
}

// ツリー内の可視ボタンを DOM 順序で取得
function getVisibleButtons(container: HTMLElement | null): HTMLButtonElement[] {
  if (!container) {
    return [];
  }
  return [...container.querySelectorAll<HTMLButtonElement>("[data-node-id]")];
}

// 現在フォーカスされているボタンのインデックスを取得
function getFocusedIndex(buttons: HTMLButtonElement[]): number {
  return buttons.indexOf(document.activeElement as HTMLButtonElement);
}

export function DirectoryTree({
  rootEntries,
  activeNodeId,
  ancestorNodeIds,
  onNavigate,
  onFocusBrowser,
  keyboardEnabled = false,
  ref,
}: DirectoryTreeProps) {
  const internalRef = useRef<HTMLElement>(null);
  const treeRef = ref ?? internalRef;
  const { toggleExpanded } = useViewerStore();

  // ディレクトリ/アーカイブ/PDFのみ表示
  const directories = rootEntries.filter(
    (e) => e.kind === "directory" || e.kind === "archive" || e.kind === "pdf",
  );

  // treeRef からコンテナ要素を取得するヘルパー
  const getContainer = useCallback((): HTMLElement | null => {
    if (typeof treeRef === "function") {
      return null;
    }
    return treeRef?.current ?? null;
  }, [treeRef]);

  const moveDown = useCallback(() => {
    const buttons = getVisibleButtons(getContainer());
    const idx = getFocusedIndex(buttons);
    if (idx < buttons.length - 1) {
      buttons[idx + 1].focus();
    }
  }, [getContainer]);

  const moveUp = useCallback(() => {
    const buttons = getVisibleButtons(getContainer());
    const idx = getFocusedIndex(buttons);
    if (idx > 0) {
      buttons[idx - 1].focus();
    }
  }, [getContainer]);

  const expand = useCallback(() => {
    const focused = document.activeElement as HTMLButtonElement;
    const nodeId = focused?.dataset?.nodeId;
    if (!nodeId) {
      return;
    }
    const treeItem = focused.closest("[role='treeitem']");
    const isExpanded = treeItem?.getAttribute("aria-expanded") === "true";
    if (isExpanded) {
      // 展開済み → 最初の子に移動
      const group = treeItem?.querySelector("[role='group']");
      const firstChild = group?.querySelector<HTMLButtonElement>("[data-node-id]");
      firstChild?.focus();
    } else {
      // 折りたたまれている → 展開
      toggleExpanded(nodeId);
    }
  }, [toggleExpanded]);

  const collapse = useCallback(() => {
    const focused = document.activeElement as HTMLButtonElement;
    const nodeId = focused?.dataset?.nodeId;
    if (!nodeId) {
      return;
    }
    const treeItem = focused.closest("[role='treeitem']");
    const isExpanded = treeItem?.getAttribute("aria-expanded") === "true";
    if (isExpanded) {
      // 展開中 → 折りたたむ
      toggleExpanded(nodeId);
    } else {
      // 折りたたみ済み → 親の treeitem に移動
      const parentGroup = treeItem?.parentElement;
      if (parentGroup?.getAttribute("role") === "group") {
        const parentItem = parentGroup.closest("[role='treeitem']");
        const parentButton = parentItem?.querySelector<HTMLButtonElement>("[data-node-id]");
        parentButton?.focus();
      }
    }
  }, [toggleExpanded]);

  // Enter: navigate のみ（展開/折りたたみは ←/→ で操作）
  const select = useCallback(() => {
    const focused = document.activeElement as HTMLButtonElement;
    const nodeId = focused?.dataset?.nodeId;
    if (nodeId) {
      onNavigate(nodeId);
    }
  }, [onNavigate]);

  const goFirst = useCallback(() => {
    const buttons = getVisibleButtons(getContainer());
    buttons[0]?.focus();
  }, [getContainer]);

  const goLast = useCallback(() => {
    const buttons = getVisibleButtons(getContainer());
    buttons.at(-1)?.focus();
  }, [getContainer]);

  useTreeKeyboard(
    {
      moveDown,
      moveUp,
      expand,
      collapse,
      select,
      goFirst,
      goLast,
      focusBrowser: onFocusBrowser ?? (() => {}),
    },
    keyboardEnabled,
  );

  return (
    <aside
      ref={treeRef as Ref<HTMLElement>}
      className="w-64 shrink-0 overflow-y-auto border-r border-white/5 bg-surface-card"
    >
      <div className="p-2 text-xs font-medium uppercase tracking-wider text-gray-500">
        ディレクトリ
      </div>
      <div role="tree" aria-label="ディレクトリツリー">
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
      </div>
    </aside>
  );
}
