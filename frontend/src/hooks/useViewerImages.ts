// BrowsePage の画像配列とビューワー起動関連の派生値
// - images: ブラウズソート順（FileBrowser/VideoFeed と同じ表示順）
// - viewerImages: ビューワーは常に名前昇順
// - viewerIndexMap: ブラウズ順 index → ビューワー順 index への高速変換
// - openViewerNameSorted: FileBrowser から渡されるブラウズ順 index を変換して openViewer 呼び出し

import { useCallback, useMemo } from "react";
import { compareEntryName } from "../utils/sortEntries";
import type { BrowseEntry } from "../types/api";

interface UseViewerImagesResult {
  images: BrowseEntry[];
  viewerImages: BrowseEntry[];
  viewerIndexMap: Map<string, number>;
  openViewerNameSorted: (browseIndex: number) => void;
}

export function useViewerImages(
  entries: BrowseEntry[] | undefined,
  openViewer: (index: number) => void,
): UseViewerImagesResult {
  // 現在のディレクトリ内の画像エントリ（ブラウズソート順）
  const images = useMemo(() => (entries ?? []).filter((e) => e.kind === "image"), [entries]);

  // ビューワー用: ブラウズのソート順に関わらず名前昇順で表示
  const viewerImages = useMemo(() => images.toSorted(compareEntryName), [images]);

  // ブラウズ順インデックス → ビューワー順インデックスの変換マップ
  const viewerIndexMap = useMemo(() => {
    const map = new Map<string, number>();
    viewerImages.forEach((img, idx) => map.set(img.node_id, idx));
    return map;
  }, [viewerImages]);

  // FileBrowser からのブラウズ順インデックスをビューワー順に変換して開く
  const openViewerNameSorted = useCallback(
    (browseIndex: number) => {
      const img = images[browseIndex];
      if (!img) {
        return;
      }
      const viewerIdx = viewerIndexMap.get(img.node_id) ?? 0;
      openViewer(viewerIdx);
    },
    [images, viewerIndexMap, openViewer],
  );

  return { images, viewerImages, viewerIndexMap, openViewerNameSorted };
}
