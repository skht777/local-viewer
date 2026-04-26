// FileBrowser のアクション handler 群
// - handleAction: ダブルクリック / Enter / g 用（kind 別の主アクション）
// - handleOpen: オーバーレイ「▶ 開く」/ Space 用
// - handleEnter: オーバーレイ「→ 進入」用（directory/archive のみ）
// - getOpenHandler / getEnterHandler: kind に応じた handler を返す（none なら undefined）

import { useCallback } from "react";
import type { ViewerTab } from "./useViewerParams";
import type { BrowseEntry } from "../types/api";

interface UseFileBrowserActionsParams {
  indexMap: Map<string, number>;
  onNavigate: (nodeId: string, options?: { tab?: ViewerTab }) => void;
  onImageClick?: (imageIndex: number) => void;
  onPdfClick?: (nodeId: string) => void;
  onOpenViewer?: (nodeId: string) => void;
}

type EntryHandler = (entry: BrowseEntry) => void;

interface UseFileBrowserActionsResult {
  handleAction: EntryHandler;
  handleOpen: EntryHandler;
  handleEnter: EntryHandler;
  getOpenHandler: (entry: BrowseEntry) => EntryHandler | undefined;
  getEnterHandler: (entry: BrowseEntry) => EntryHandler | undefined;
}

export function useFileBrowserActions({
  indexMap,
  onNavigate,
  onImageClick,
  onPdfClick,
  onOpenViewer,
}: UseFileBrowserActionsParams): UseFileBrowserActionsResult {
  // ダブルクリック / Enter / g: アクション実行（進入/ビューワー起動）
  const handleAction = useCallback<EntryHandler>(
    (entry) => {
      if (entry.kind === "archive") {
        onNavigate(entry.node_id, { tab: "images" });
      } else if (entry.kind === "directory") {
        onNavigate(entry.node_id);
      } else if (entry.kind === "pdf") {
        onPdfClick?.(entry.node_id);
      } else if (entry.kind === "image" && onImageClick) {
        const imageIndex = indexMap.get(entry.node_id) ?? -1;
        if (imageIndex >= 0) {
          onImageClick(imageIndex);
        }
      }
    },
    [indexMap, onNavigate, onPdfClick, onImageClick],
  );

  // オーバーレイ「▶ 開く」/ Space
  const handleOpen = useCallback<EntryHandler>(
    (entry) => {
      if (entry.kind === "directory" || entry.kind === "archive") {
        onOpenViewer?.(entry.node_id);
      } else if (entry.kind === "image" && onImageClick) {
        const imageIndex = indexMap.get(entry.node_id) ?? -1;
        if (imageIndex >= 0) {
          onImageClick(imageIndex);
        }
      } else if (entry.kind === "pdf") {
        onPdfClick?.(entry.node_id);
      }
    },
    [indexMap, onOpenViewer, onImageClick, onPdfClick],
  );

  // オーバーレイ「→ 進入」: directory/archive のナビゲーション
  const handleEnter = useCallback<EntryHandler>(
    (entry) => {
      if (entry.kind === "archive") {
        onNavigate(entry.node_id, { tab: "images" });
      } else if (entry.kind === "directory") {
        onNavigate(entry.node_id);
      }
    },
    [onNavigate],
  );

  const getOpenHandler = useCallback(
    (entry: BrowseEntry): EntryHandler | undefined => {
      if (
        entry.kind === "directory" ||
        entry.kind === "archive" ||
        entry.kind === "image" ||
        entry.kind === "pdf"
      ) {
        return handleOpen;
      }
      return undefined;
    },
    [handleOpen],
  );

  const getEnterHandler = useCallback(
    (entry: BrowseEntry): EntryHandler | undefined => {
      if (entry.kind === "directory" || entry.kind === "archive") {
        return handleEnter;
      }
      return undefined;
    },
    [handleEnter],
  );

  return {
    handleAction,
    handleOpen,
    handleEnter,
    getOpenHandler,
    getEnterHandler,
  };
}
