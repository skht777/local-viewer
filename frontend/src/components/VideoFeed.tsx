// 動画をフィード形式で表示する
// - @tanstack/react-virtual で仮想スクロール
// - ビューポート外の動画はアンマウントされ、リソースを解放
// - 各動画は VideoCard で表示
// - 仮想スクロールの動的位置決めはインラインスタイル使用 (MangaViewer の前例に従う)

import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { BrowseEntry } from "../types/api";
import { VideoCard } from "./VideoCard";

interface VideoFeedProps {
  videos: BrowseEntry[];
}

export function VideoFeed({ videos }: VideoFeedProps) {
  if (videos.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <p className="text-gray-500">動画がありません</p>
      </div>
    );
  }

  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: videos.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 400,
    overscan: 2,
  });

  return (
    <div ref={parentRef} className="flex-1 overflow-y-auto">
      {/* 仮想スクロールの動的値は Tailwind で表現不可のためインラインスタイル使用 */}
      <div style={{ height: virtualizer.getTotalSize() }} className="relative mx-auto max-w-4xl">
        {virtualizer.getVirtualItems().map((item) => (
          <div
            key={item.key}
            ref={virtualizer.measureElement}
            data-index={item.index}
            style={{
              position: "absolute",
              top: 0,
              transform: `translateY(${item.start}px)`,
              width: "100%",
            }}
          >
            <div className="p-4">
              <VideoCard entry={videos[item.index]} />
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
