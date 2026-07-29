// ビューワー表示中の画像リスト再構築で表示画像がすり替わるのを防ぐ
// - URL の index は「viewerImages 配列の位置」であり、全ページの後追い到着
//   (useEnsureAllBrowsePages / useEnsureAllPages) で名前昇順の並びが変わると
//   同じ index が別の画像を指してしまう
// - 表示中の node_id を追跡し、リスト再構築で位置がずれたら新しい index へ
//   URL を補正する (setIndex は replace のため履歴は汚れない)
// - ユーザーのページ送り (index のみ変化) では補正しない

import { useEffect, useRef } from "react";
import type { BrowseEntry } from "../types/api";

interface UseViewerIndexRealignParams {
  isViewerOpen: boolean;
  viewerImages: BrowseEntry[];
  index: number;
  setIndex: (index: number) => void;
}

export function useViewerIndexRealign({
  isViewerOpen,
  viewerImages,
  index,
  setIndex,
}: UseViewerIndexRealignParams): void {
  const trackedNodeIdRef = useRef<string | null>(null);
  const prevImagesRef = useRef<BrowseEntry[]>(viewerImages);

  useEffect(() => {
    if (!isViewerOpen || viewerImages.length === 0) {
      trackedNodeIdRef.current = null;
      prevImagesRef.current = viewerImages;
      return;
    }

    const safeIndex = Math.max(0, Math.min(index, viewerImages.length - 1));
    const listChanged = prevImagesRef.current !== viewerImages;
    prevImagesRef.current = viewerImages;

    if (listChanged && trackedNodeIdRef.current) {
      const newIndex = viewerImages.findIndex((e) => e.node_id === trackedNodeIdRef.current);
      if (newIndex >= 0 && newIndex !== safeIndex) {
        // 表示中だった画像の新しい位置へ補正 (tracked は維持し、次回の発火で同期)
        setIndex(newIndex);
        return;
      }
      // 消えた場合は clamp 後の画像を新たに追跡する (下へフォールスルー)
    }

    trackedNodeIdRef.current = viewerImages[safeIndex]?.node_id ?? null;
  }, [isViewerOpen, viewerImages, index, setIndex]);
}
