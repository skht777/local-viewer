// UI ローカル状態の管理 (zustand)
// - サーバー状態は TanStack Query に任せる
// - ここでは純粋な UI 状態のみ管理
// - fitMode, spreadMode は persist middleware で localStorage に永続化

import { create } from "zustand";
import { persist } from "zustand/middleware";

export type FitMode = "width" | "height" | "original";
export type SpreadMode = "single" | "spread" | "spread-offset";

const SPREAD_CYCLE: SpreadMode[] = ["single", "spread", "spread-offset"];

interface ViewerState {
  // サイドバー開閉（永続化しない）
  isSidebarOpen: boolean;
  toggleSidebar: () => void;
  setSidebarOpen: (isOpen: boolean) => void;

  // ディレクトリツリーの展開状態（永続化しない）
  expandedNodeIds: Set<string>;
  toggleExpanded: (nodeId: string) => void;

  // 画像表示モード（永続化）
  fitMode: FitMode;
  setFitMode: (mode: FitMode) => void;

  // 見開きモード（永続化）
  spreadMode: SpreadMode;
  cycleSpreadMode: () => void;
}

export const useViewerStore = create<ViewerState>()(
  persist(
    (set) => ({
      isSidebarOpen: true,

      toggleSidebar: () => set((state) => ({ isSidebarOpen: !state.isSidebarOpen })),

      setSidebarOpen: (isOpen) => set({ isSidebarOpen: isOpen }),

      expandedNodeIds: new Set<string>(),

      toggleExpanded: (nodeId) =>
        set((state) => {
          const next = new Set(state.expandedNodeIds);
          if (next.has(nodeId)) {
            next.delete(nodeId);
          } else {
            next.add(nodeId);
          }
          return { expandedNodeIds: next };
        }),

      fitMode: "width",

      setFitMode: (mode) => set({ fitMode: mode }),

      spreadMode: "single",

      cycleSpreadMode: () =>
        set((state) => {
          const idx = SPREAD_CYCLE.indexOf(state.spreadMode);
          return { spreadMode: SPREAD_CYCLE[(idx + 1) % SPREAD_CYCLE.length] };
        }),
    }),
    {
      name: "viewer-store",
      partialize: (state) => ({ fitMode: state.fitMode, spreadMode: state.spreadMode }),
    },
  ),
);
