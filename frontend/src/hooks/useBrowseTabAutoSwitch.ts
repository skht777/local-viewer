// 現在タブが空ならコンテンツのある別タブへ自動切替
// - 優先順位: filesets > images > videos
// - 現在タブにコンテンツがあればそのまま
// - すべて空なら現在タブに留まる

import { useEffect } from "react";
import type { ViewerTab } from "./useViewerParams";
import type { BrowseResponse } from "../types/api";

interface UseBrowseTabAutoSwitchParams {
  data: BrowseResponse | undefined;
  isLoading: boolean;
  currentTab: ViewerTab;
  setTab: (tab: ViewerTab) => void;
}

export function useBrowseTabAutoSwitch({
  data,
  isLoading,
  currentTab,
  setTab,
}: UseBrowseTabAutoSwitchParams): void {
  useEffect(() => {
    if (!data || isLoading) {
      return;
    }

    const hasFilesets = data.entries.some(
      (e) => e.kind === "directory" || e.kind === "archive" || e.kind === "pdf",
    );
    const hasImages = data.entries.some((e) => e.kind === "image");
    const hasVideos = data.entries.some((e) => e.kind === "video");

    // 現在のタブにコンテンツがあればそのまま
    if (currentTab === "filesets" && hasFilesets) {
      return;
    }
    if (currentTab === "images" && hasImages) {
      return;
    }
    if (currentTab === "videos" && hasVideos) {
      return;
    }

    // 現在のタブが空 → 最適なタブに自動切替（すべて空なら現在タブに留まる）
    if (hasFilesets) {
      setTab("filesets");
    } else if (hasImages) {
      setTab("images");
    } else if (hasVideos) {
      setTab("videos");
    }
  }, [data, isLoading, currentTab, setTab]);
}
