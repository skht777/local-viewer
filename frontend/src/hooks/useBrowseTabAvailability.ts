// コンテンツのないタブを disabled として返す
// - 全て空のときは filesets のみ有効化（デフォルトタブを残す）

import { useMemo } from "react";
import type { ViewerTab } from "./useViewerParams";
import type { BrowseEntry, BrowseResponse } from "../types/api";

interface UseBrowseTabAvailabilityParams {
  data: BrowseResponse | undefined;
  images: BrowseEntry[];
  videos: BrowseEntry[];
}

export function useBrowseTabAvailability({
  data,
  images,
  videos,
}: UseBrowseTabAvailabilityParams): Set<ViewerTab> {
  return useMemo(() => {
    if (!data) {
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
  }, [data, images.length, videos.length]);
}
