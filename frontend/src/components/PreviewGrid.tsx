// ディレクトリ/アーカイブのプレビューサムネイルを1-3枚表示するグリッド
// - 1枚: フル表示 (object-cover)
// - 2枚: 左右2分割
// - 3枚: 左1枚大 + 右2枚小
// - 全画像エラー時に onAllError コールバックで通知

import { useState } from "react";

interface PreviewGridProps {
  previewNodeIds: string[];
  onAllError?: () => void;
  batchThumbnails?: Map<string, string>;
}

export function PreviewGrid({ previewNodeIds, onAllError, batchThumbnails }: PreviewGridProps) {
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

  // バッチ Blob URL があれば使用、なければスケルトン表示（個別 API フォールバックなし）
  const thumbSrc = (id: string) => batchThumbnails?.get(id);

  // エラーでない画像のみ表示
  const validIds = previewNodeIds.filter((id) => !errorSet.has(id));

  if (validIds.length === 0) {
    return null;
  }

  // src が未定義（バッチロード中）ならスケルトン、それ以外は img を表示
  const renderThumb = (id: string, extraClass?: string) => {
    const src = thumbSrc(id);
    if (!src) {
      return <div className={`animate-pulse bg-surface-raised ${extraClass ?? "h-full w-full"}`} />;
    }
    return (
      <img
        src={src}
        alt=""
        className={`object-cover ${extraClass ?? "h-full w-full"}`}
        loading="lazy"
        decoding="async"
        onError={() => handleError(id)}
      />
    );
  };

  if (validIds.length === 1) {
    return renderThumb(validIds[0], "h-full w-full");
  }

  if (validIds.length === 2) {
    return (
      <div className="grid h-full w-full grid-cols-2 gap-0.5">
        {validIds.map((id) => (
          <div key={id}>{renderThumb(id, "h-full w-full")}</div>
        ))}
      </div>
    );
  }

  // 3枚: 左1枚大 (row-span-2) + 右2枚小
  return (
    <div className="grid h-full w-full grid-cols-2 grid-rows-2 gap-0.5">
      <div className="row-span-2">{renderThumb(validIds[0], "h-full w-full")}</div>
      {renderThumb(validIds[1], "h-full w-full")}
      {renderThumb(validIds[2], "h-full w-full")}
    </div>
  );
}
