// UI ローカル状態の管理 (zustand)
// - サーバー状態は TanStack Query に任せる
// - ここでは純粋な UI 状態のみ管理

import { create } from "zustand";

interface ViewerState {
  // サイドバー開閉
  isSidebarOpen: boolean;
  toggleSidebar: () => void;
  setSidebarOpen: (isOpen: boolean) => void;

  // ディレクトリツリーの展開状態
  expandedNodeIds: Set<string>;
  toggleExpanded: (nodeId: string) => void;
}

export const useViewerStore = create<ViewerState>((set) => ({
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
}));
