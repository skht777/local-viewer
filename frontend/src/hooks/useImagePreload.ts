// 隣接画像のプリフェッチ
// - 現在 index の前後 range 枚を new Image().src で先読み
// - ブラウザのイメージキャッシュを活用

import { useEffect } from "react";
import type { BrowseEntry } from "../types/api";

export function useImagePreload(images: BrowseEntry[], currentIndex: number, range = 2): void {
  useEffect(() => {
    for (let offset = -range; offset <= range; offset++) {
      const idx = currentIndex + offset;
      if (offset !== 0 && idx >= 0 && idx < images.length) {
        const img = new Image();
        img.src = `/api/file/${images[idx].node_id}`;
      }
    }
  }, [images, currentIndex, range]);
}
