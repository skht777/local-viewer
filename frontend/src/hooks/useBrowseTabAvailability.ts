// コンテンツのないタブを disabled として返す
// - 全て空のときは filesets のみ有効化（デフォルトタブを残す）
// - 全ページ確定前 (hasNextPage) は disabled 判定しない
//   （1 ページ目にディレクトリだけ並ぶと未取得ページの画像/動画を空と誤検知するため）

import { useMemo } from "react";
import type { ViewerTab } from "./useViewerParams";
import type { BrowseEntry, BrowseResponse } from "../types/api";

interface UseBrowseTabAvailabilityParams {
  data: BrowseResponse | undefined;
  images: BrowseEntry[];
  videos: BrowseEntry[];
  hasNextPage: boolean;
}

export function useBrowseTabAvailability({
  data,
  images,
  videos,
  hasNextPage,
}: UseBrowseTabAvailabilityParams): Set<ViewerTab> {
  return useMemo(() => {
    if (!data || hasNextPage) {
      return new Set<ViewerTab>();
    }
    const disabled = new Set<ViewerTab>();
    const hasFilesets = data.entries.some(
      (e) => e.kind === "directory" || e.kind === "archive" || e.kind === "pdf",
    );
    if (!hasFilesets) {
      disabled.add("filesets");
    }
    if (images.length === 0) {
      disabled.add("images");
    }
    if (videos.length === 0) {
      disabled.add("videos");
    }
    if (disabled.size === 3) {
      disabled.delete("filesets");
    }
    return disabled;
  }, [data, images.length, videos.length, hasNextPage]);
}
