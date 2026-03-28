// サムネイル一覧サイドバー
// - 画像を /api/file/{node_id} で表示（固定サイズで帯域節約）
// - アクティブ画像をハイライト + scrollIntoView で自動スクロール
// - クリックでページジャンプ
// - scrollBehavior: CG モード = "smooth"、マンガモード = "instant"（jank 防止）

import { useEffect, useRef } from "react";
import type { BrowseEntry } from "../types/api";

interface ThumbnailSidebarProps {
  images: BrowseEntry[];
  currentIndex: number;
  onSelect: (index: number) => void;
  scrollBehavior?: ScrollBehavior;
}

export function ThumbnailSidebar({
  images,
  currentIndex,
  onSelect,
  scrollBehavior = "smooth",
}: ThumbnailSidebarProps) {
  const activeRef = useRef<HTMLButtonElement>(null);

  // アクティブサムネイルを自動スクロール
  useEffect(() => {
    activeRef.current?.scrollIntoView({ block: "nearest", behavior: scrollBehavior });
  }, [currentIndex, scrollBehavior]);

  return (
    <aside className="flex w-24 flex-shrink-0 flex-col gap-1 overflow-y-auto bg-gray-900/80 p-1">
      {images.map((entry, idx) => {
        const isActive = idx === currentIndex;
        return (
          <button
            key={entry.node_id}
            ref={isActive ? activeRef : undefined}
            type="button"
            onClick={() => onSelect(idx)}
            className={`overflow-hidden rounded ${isActive ? "ring-2 ring-blue-500" : "opacity-60 hover:opacity-100"}`}
            aria-current={isActive ? "true" : undefined}
          >
            <img
              src={`/api/file/${entry.node_id}`}
              alt={entry.name}
              className="h-16 w-full object-cover"
              loading="lazy"
              decoding="async"
            />
          </button>
        );
      })}
    </aside>
  );
}
