// 動画をフィード形式で表示する
// - @tanstack/react-virtual で仮想スクロール
// - ビューポート外の動画はアンマウントされ、リソースを解放
// - 各動画は VideoCard で表示
// - 仮想スクロールの動的位置決めはインラインスタイル使用 (MangaViewer の前例に従う)
// - 再生位置マップ (useRef) でアンマウント後も再生位置を保持

import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { BrowseEntry } from "../types/api";
import { VideoCard } from "./VideoCard";

interface VideoFeedProps {
  videos: BrowseEntry[];
}

export function VideoFeed({ videos }: VideoFeedProps) {
  // 再生位置マップ (node_id → currentTime)
  // useRef でリレンダーを回避
  const playbackTimeRef = useRef(new Map<string, number>());
  const parentRef = useRef<HTMLDivElement>(null);

  const handleTimeUpdate = (nodeId: string, time: number) => {
    playbackTimeRef.current.set(nodeId, time);
  };

  const virtualizer = useVirtualizer({
    count: videos.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 400,
    overscan: 2,
  });

  if (videos.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <p className="text-gray-500">動画がありません</p>
      </div>
    );
  }

  return (
    <div ref={parentRef} className="flex-1 overflow-y-auto">
      {/* 仮想スクロールの動的値は Tailwind で表現不可のためインラインスタイル使用 */}
      <div style={{ height: virtualizer.getTotalSize() }} className="relative mx-auto max-w-4xl">
        {virtualizer.getVirtualItems().map((item) => {
          const video = videos[item.index];
          return (
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
                <VideoCard
                  entry={video}
                  initialTime={playbackTimeRef.current.get(video.node_id)}
                  onTimeUpdate={handleTimeUpdate}
                />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
