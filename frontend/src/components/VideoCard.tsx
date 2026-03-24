// 1件の動画を埋め込みプレイヤーで表示するカード
// - ファイル名ラベルを動画の上に配置
// - HTML5 <video controls> でシーク・フルスクリーン対応
// - 再生不可時 (MKV等) はフォールバックメッセージを表示

import { useState } from "react";
import type { BrowseEntry } from "../types/api";
import { formatFileSize } from "../utils/format";

interface VideoCardProps {
  entry: BrowseEntry;
}

export function VideoCard({ entry }: VideoCardProps) {
  const [hasError, setHasError] = useState(false);

  return (
    <div className="overflow-hidden rounded-lg bg-gray-800">
      {/* ファイル名ラベルを動画の上に配置 */}
      <div className="px-3 pt-3">
        <p className="truncate text-sm font-medium">{entry.name}</p>
        {entry.size_bytes != null && (
          <span className="text-xs text-gray-400">{formatFileSize(entry.size_bytes)}</span>
        )}
      </div>
      <div className="p-3">
        {hasError ? (
          <div className="flex aspect-video items-center justify-center rounded bg-gray-700 text-sm text-gray-400">
            この形式はブラウザで再生できません
          </div>
        ) : (
          <video
            controls
            preload="none"
            className="w-full rounded"
            src={`/api/file/${entry.node_id}`}
            onError={() => setHasError(true)}
          >
            <track kind="captions" />
          </video>
        )}
      </div>
    </div>
  );
}
