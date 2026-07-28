// 現在タブが空ならコンテンツのある別タブへ自動切替
// - 優先順位: filesets > images > videos
// - 現在タブにコンテンツがあればそのまま
// - すべて空なら現在タブに留まる
// - ビューワー表示中は切替しない（tab 変更が isViewerOpen を false 化しビューワーを閉じるため）
// - 全ページ確定前 (hasNextPage) は切替しない（未取得ページのコンテンツを空と誤検知するため）

import { useEffect } from "react";
import type { ViewerTab } from "./useViewerParams";
import type { BrowseResponse } from "../types/api";

interface UseBrowseTabAutoSwitchParams {
  data: BrowseResponse | undefined;
  isLoading: boolean;
  hasNextPage: boolean;
  isViewerOpen: boolean;
  currentTab: ViewerTab;
  setTab: (tab: ViewerTab) => void;
}

export function useBrowseTabAutoSwitch({
  data,
  isLoading,
  hasNextPage,
  isViewerOpen,
  currentTab,
  setTab,
}: UseBrowseTabAutoSwitchParams): void {
  useEffect(() => {
    if (!data || isLoading || hasNextPage || isViewerOpen) {
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
  }, [data, isLoading, hasNextPage, isViewerOpen, currentTab, setTab]);
}
