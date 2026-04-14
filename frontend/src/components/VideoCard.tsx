// 1件の動画を埋め込みプレイヤーで表示するカード
// - ファイル名ラベルを動画の上に配置
// - HTML5 <video controls> でシーク・フルスクリーン対応
// - 再生不可時 (MKV等) はフォールバックメッセージを表示
// - initialTime / onTimeUpdate で仮想スクロール時の再生位置を保存・復元

import { useRef, useState } from "react";
import type { BrowseEntry } from "../types/api";
import { formatFileSize } from "../utils/format";

interface VideoCardProps {
  entry: BrowseEntry;
  initialTime?: number;
  onTimeUpdate?: (nodeId: string, time: number) => void;
}

export function VideoCard({ entry, initialTime, onTimeUpdate }: VideoCardProps) {
  const [hasError, setHasError] = useState(false);
  const videoRef = useRef<HTMLVideoElement>(null);
  const lastReportRef = useRef(0);

  // マウント時に initialTime を設定 (metadata 読み込み完了後)
  const handleLoadedMetadata = () => {
    if (videoRef.current && initialTime && initialTime > 0) {
      videoRef.current.currentTime = initialTime;
    }
  };

  // timeupdate を約 1 秒間隔でスロットリングして親に通知
  const handleTimeUpdate = () => {
    if (!videoRef.current || !onTimeUpdate) return;
    const now = Date.now();
    if (now - lastReportRef.current < 1000) return;
    lastReportRef.current = now;
    onTimeUpdate(entry.node_id, videoRef.current.currentTime);
  };

  return (
    <div
      data-testid={`video-card-${entry.node_id}`}
      className="overflow-hidden rounded-lg bg-surface-card ring-1 ring-white/5"
    >
      {/* ファイル名ラベルを動画の上に配置 */}
      <div className="px-3 pt-3">
        <p className="truncate text-sm font-medium">{entry.name}</p>
        {entry.size_bytes != null && (
          <span className="text-xs font-mono tabular-nums text-gray-400">
            {formatFileSize(entry.size_bytes)}
          </span>
        )}
      </div>
      <div className="p-3">
        {hasError ? (
          <div
            data-testid="video-error-fallback"
            className="flex aspect-video items-center justify-center rounded bg-surface-raised text-sm text-gray-400"
          >
            この形式はブラウザで再生できません
          </div>
        ) : (
          <video
            ref={videoRef}
            controls
            preload="none"
            className="max-h-[85vh] w-full rounded object-contain"
            src={`/api/file/${entry.node_id}`}
            onError={() => setHasError(true)}
            onLoadedMetadata={handleLoadedMetadata}
            onTimeUpdate={handleTimeUpdate}
          >
            <track kind="captions" />
          </video>
        )}
      </div>
    </div>
  );
}
