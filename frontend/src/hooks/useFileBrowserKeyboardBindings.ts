// FileBrowser のキーボード操作バインディング
// - 矢印 / WASD: handleMove で選択を delta 分移動 + 仮想スクロール + focus 遷移
// - Enter / g: 主アクション
// - Space: 開く（オーバーレイ Open）
// - g/Shift+G: 親方向操作 (goParent), 他

import { useCallback } from "react";
import { useBrowseKeyboard } from "./useBrowseKeyboard";
import type { ViewerTab } from "./useViewerParams";
import type { BrowseEntry } from "../types/api";

interface UseFileBrowserKeyboardBindingsParams {
  filtered: BrowseEntry[];
  indexMap: Map<string, number>;
  effectiveSelectedId: string | null;
  scrollToItem: (index: number) => void;
  setLocalSelectedId: (id: string | null) => void;
  handleAction: (entry: BrowseEntry) => void;
  handleOpen: (entry: BrowseEntry) => void;
  getColumnCount: () => number;
  keyboardEnabled: boolean;
  onGoParent?: () => void;
  onFocusTree?: () => void;
  onToggleMode?: () => void;
  onSortName?: () => void;
  onSortDate?: () => void;
  onTabChange?: (tab: ViewerTab) => void;
}

export function useFileBrowserKeyboardBindings({
  filtered,
  indexMap,
  effectiveSelectedId,
  scrollToItem,
  setLocalSelectedId,
  handleAction,
  handleOpen,
  getColumnCount,
  keyboardEnabled,
  onGoParent,
  onFocusTree,
  onToggleMode,
  onSortName,
  onSortDate,
  onTabChange,
}: UseFileBrowserKeyboardBindingsParams): void {
  // delta 分だけ選択を移動し、仮想スクロールで可視化 + focus 遷移
  const handleMove = useCallback(
    (delta: number) => {
      const currentIndex = indexMap.get(effectiveSelectedId ?? "") ?? -1;
      const newIndex = currentIndex + delta;
      if (newIndex < 0 || newIndex >= filtered.length) {
        return;
      }
      const target = filtered[newIndex];
      setLocalSelectedId(target.node_id);
      scrollToItem(newIndex);
      // DOM 更新後にフォーカスを移動
      requestAnimationFrame(() => {
        const el = document.querySelector<HTMLElement>(
          `[data-testid="file-card-${target.node_id}"]`,
        );
        el?.focus({ preventScroll: true });
      });
    },
    [filtered, indexMap, effectiveSelectedId, scrollToItem, setLocalSelectedId],
  );

  useBrowseKeyboard(
    {
      move: handleMove,
      action: () => {
        const entry = filtered.find((e) => e.node_id === effectiveSelectedId);
        if (entry) {
          handleAction(entry);
        }
      },
      open: () => {
        const entry = filtered.find((e) => e.node_id === effectiveSelectedId);
        if (entry) {
          handleOpen(entry);
        }
      },
      goParent: onGoParent ?? (() => {}),
      focusTree: onFocusTree ?? (() => {}),
      toggleMode: onToggleMode ?? (() => {}),
      sortName: onSortName ?? (() => {}),
      sortDate: onSortDate ?? (() => {}),
      tabChange: onTabChange ?? (() => {}),
      getColumnCount,
    },
    keyboardEnabled,
  );
}
