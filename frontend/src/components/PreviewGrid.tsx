// ディレクトリ/アーカイブのプレビューサムネイルを1-3枚表示するグリッド
// - 1枚: フル表示 (object-cover)
// - 2枚: 左右2分割
// - 3枚: 左1枚大 + 右2枚小
// - 全画像エラー時に onAllError コールバックで通知

import { useState } from "react";
import { thumbnailUrl } from "../utils/thumbnailUrl";

interface PreviewGridProps {
  previewNodeIds: string[];
  modifiedAt?: number | null;
  onAllError?: () => void;
  batchThumbnails?: Map<string, string>;
}

export function PreviewGrid({
  previewNodeIds,
  modifiedAt,
  onAllError,
  batchThumbnails,
}: PreviewGridProps) {
  const [errorSet, setErrorSet] = useState<Set<string>>(new Set());

  const handleError = (nodeId: string) => {
    setErrorSet((prev) => {
      const next = new Set(prev);
      next.add(nodeId);
      // 全画像がエラーなら親に通知
      if (next.size >= previewNodeIds.length) {
        onAllError?.();
      }
      return next;
    });
  };

  // バッチ URL 優先、なければ個別 URL にフォールバック
  const thumbSrc = (id: string) => batchThumbnails?.get(id) ?? thumbnailUrl(id, modifiedAt);

  // エラーでない画像のみ表示
  const validIds = previewNodeIds.filter((id) => !errorSet.has(id));

  if (validIds.length === 0) {
    return null;
  }

  if (validIds.length === 1) {
    return (
      <img
        src={thumbSrc(validIds[0])}
        alt=""
        className="h-full w-full object-cover"
        loading="lazy"
        decoding="async"
        onError={() => handleError(validIds[0])}
      />
    );
  }

  if (validIds.length === 2) {
    return (
      <div className="grid h-full w-full grid-cols-2 gap-0.5">
        {validIds.map((id) => (
          <img
            key={id}
            src={thumbSrc(id)}
            alt=""
            className="h-full w-full object-cover"
            loading="lazy"
            decoding="async"
            onError={() => handleError(id)}
          />
        ))}
      </div>
    );
  }

  // 3枚: 左1枚大 (row-span-2) + 右2枚小
  return (
    <div className="grid h-full w-full grid-cols-2 grid-rows-2 gap-0.5">
      <img
        src={thumbSrc(validIds[0])}
        alt=""
        className="row-span-2 h-full w-full object-cover"
        loading="lazy"
        decoding="async"
        onError={() => handleError(validIds[0])}
      />
      <img
        src={thumbSrc(validIds[1])}
        alt=""
        className="h-full w-full object-cover"
        loading="lazy"
        decoding="async"
        onError={() => handleError(validIds[1])}
      />
      <img
        src={thumbSrc(validIds[2])}
        alt=""
        className="h-full w-full object-cover"
        loading="lazy"
        decoding="async"
        onError={() => handleError(validIds[2])}
      />
    </div>
  );
}
