// UI ローカル状態の管理 (zustand)
// - サーバー状態は TanStack Query に任せる
// - ここでは純粋な UI 状態のみ管理
// - fitMode, spreadMode は persist middleware で localStorage に永続化

import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { JumpListEntry } from "../lib/jumpListNavigation";

export type FitMode = "width" | "height" | "original";
export type SpreadMode = "single" | "spread" | "spread-offset";

// ビューワーを開いた時の起点情報（閉じる時に戻る先）
// Route-aware: pathname を含めることで /browse 以外（例: /search）から開いても復帰できる
export interface ViewerOrigin {
  pathname: string;
  search: string;
}

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

  // マンガモード: ズーム倍率 %（永続化）
  zoomLevel: number;
  setZoomLevel: (level: number) => void;
  zoomIn: () => void;
  zoomOut: () => void;

  // マンガモード: スクロール速度倍率（永続化）
  scrollSpeed: number;
  setScrollSpeed: (speed: number) => void;

  // ビューワー起点（永続化しない）: セットジャンプ後に閉じた時の復帰先
  viewerOrigin: ViewerOrigin | null;
  setViewerOrigin: (origin: ViewerOrigin | null) => void;

  // セットジャンプ中のトランジション ID（永続化しない）
  // 0: トランジションなし、>0: トランジション中
  viewerTransitionId: number;
  startViewerTransition: () => number;
  endViewerTransition: (id: number) => void;

  // セット間ジャンプの範囲リスト（永続化しない）
  // - null: 既存 FS sibling 経路（マウントルートまで再帰）
  // - 非 null: 与えられたリスト範囲内でのみ X/Z でジャンプ
  // - 検索結果からの viewer 起動時に snapshot され、close でクリアされる
  viewerJumpList: JumpListEntry[] | null;
  setViewerJumpList: (list: JumpListEntry[] | null) => void;

  // jumpList 内の現在位置（永続化しない）
  // - null: jumpList 未使用または範囲外（FS sibling 経路へフォールバック扱い）
  // - 非 null: jumpList[index] が現在開かれているセット起点
  // - viewer の currentNodeId が resolveFirstViewable で着地ずれしても index は起動 entry の位置で固定
  viewerJumpListIndex: number | null;
  // index のみ更新（navigate 後の前進/後退用）
  // - viewerJumpList が null または範囲外の場合は内部で null に正規化する
  setViewerJumpListIndex: (index: number | null) => void;
}

export const useViewerStore = create<ViewerState>()(
  persist(
    (set) => ({
      cycleSpreadMode: () =>
        set((state) => {
          const idx = SPREAD_CYCLE.indexOf(state.spreadMode);
          return { spreadMode: SPREAD_CYCLE[(idx + 1) % SPREAD_CYCLE.length] };
        }),

      endViewerTransition: (id) =>
        set((state) => {
          // stale な遷移完了は無視
          if (state.viewerTransitionId !== id) {
            return state;
          }
          return { viewerTransitionId: 0 };
        }),

      expandedNodeIds: new Set<string>(),

      fitMode: "height",

      isSidebarOpen: true,

      scrollSpeed: 1,

      setFitMode: (mode) => set({ fitMode: mode }),

      setScrollSpeed: (speed) => set({ scrollSpeed: Math.max(0.5, Math.min(3, speed)) }),

      setSidebarOpen: (isOpen) => set({ isSidebarOpen: isOpen }),

      setViewerJumpList: (list) => set({ viewerJumpList: list }),

      setViewerJumpListIndex: (index) =>
        set((state) => {
          // list 未設定なら index は常に null
          if (state.viewerJumpList === null) {
            return { viewerJumpListIndex: null };
          }
          // null は明示的なクリア
          if (index === null) {
            return { viewerJumpListIndex: null };
          }
          // 範囲外は null に正規化（不正な index による静かな破綻を防ぐ）
          if (index < 0 || index >= state.viewerJumpList.length) {
            return { viewerJumpListIndex: null };
          }
          return { viewerJumpListIndex: index };
        }),

      setViewerOrigin: (origin) => set({ viewerOrigin: origin }),

      setZoomLevel: (level) => set({ zoomLevel: Math.max(25, Math.min(300, level)) }),

      spreadMode: "single",

      startViewerTransition: () => {
        let newId = 0;
        set((state) => {
          newId = state.viewerTransitionId + 1;
          return { viewerTransitionId: newId };
        });
        return newId;
      },

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

      toggleSidebar: () => set((state) => ({ isSidebarOpen: !state.isSidebarOpen })),

      viewerJumpList: null,

      viewerJumpListIndex: null,

      viewerOrigin: null,

      viewerTransitionId: 0,

      zoomIn: () => set((state) => ({ zoomLevel: Math.min(300, state.zoomLevel + 25) })),

      zoomLevel: 100,

      zoomOut: () => set((state) => ({ zoomLevel: Math.max(25, state.zoomLevel - 25) })),
    }),
    {
      name: "viewer-store",
      partialize: (state) => ({
        expandedNodeIds: [...state.expandedNodeIds],
        fitMode: state.fitMode,
        scrollSpeed: state.scrollSpeed,
        spreadMode: state.spreadMode,
        zoomLevel: state.zoomLevel,
      }),
      // Set<string> ↔ Array<string> の変換
      merge: (persisted, current) => ({
        ...current,
        ...(persisted as Record<string, unknown>),
        expandedNodeIds: new Set((persisted as { expandedNodeIds?: string[] })?.expandedNodeIds),
      }),
    },
  ),
);
