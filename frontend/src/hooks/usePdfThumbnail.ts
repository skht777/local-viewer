// PDF の先頭ページを pdfjs-dist でレンダリングし、blob URL として返すフック
// - getDocument() で PDF をロード → page.render() → canvas.toBlob() → URL.createObjectURL()
// - 生成結果は node_id + modified_at をキーにモジュールスコープの LRU にキャッシュする。
//   仮想スクロールでカードが再マウントされるたびに PDF 全体を取り直さないため
// - blob URL の所有者はキャッシュ。アンマウントでは revoke せず、LRU から溢れた時のみ revoke する
// - enabled=false の場合はロードしない

import { useEffect, useState } from "react";
import type { PDFDocumentProxy, PDFPageProxy } from "../lib/pdfjs";
import { getDocument } from "../lib/pdfjs";
import { PDF_LOAD_OPTIONS } from "../lib/pdfLoadOptions";
import { fileUrl } from "../utils/fileUrl";

const THUMB_WIDTH = 300;

// キャッシュ上限 (JPEG サムネイル ~20KB × 200 ≒ 4MB 程度に収まる想定)
const CACHE_LIMIT = 200;

// Map は挿入順を保持するため、削除 → 再挿入で LRU として扱える
const thumbnailCache = new Map<string, string>();

function cacheKey(nodeId: string, modifiedAt: number | null): string {
  return `${nodeId}:${modifiedAt ?? "none"}`;
}

// 参照した要素を末尾へ移動しつつ取り出す (LRU)
function readCache(key: string): string | null {
  const url = thumbnailCache.get(key);
  if (url === undefined) {
    return null;
  }
  thumbnailCache.delete(key);
  thumbnailCache.set(key, url);
  return url;
}

// 上限を超えた分は最も古いものから revoke して捨てる
function writeCache(key: string, url: string): void {
  thumbnailCache.set(key, url);
  while (thumbnailCache.size > CACHE_LIMIT) {
    const [oldestKey, oldestUrl] = thumbnailCache.entries().next().value as [string, string];
    thumbnailCache.delete(oldestKey);
    URL.revokeObjectURL(oldestUrl);
  }
}

// キャッシュを全解放する (テストおよび明示的なメモリ解放用)
export function clearPdfThumbnailCache(): void {
  for (const url of thumbnailCache.values()) {
    URL.revokeObjectURL(url);
  }
  thumbnailCache.clear();
}

interface PdfThumbnailResult {
  url: string | null;
  isLoading: boolean;
  hasError: boolean;
}

// PDF の先頭ページを取得する。pdfDoc は呼び出し側が destroy する責務を持つ
async function loadPdfThumbnailPage(
  nodeId: string,
  modifiedAt: number | null,
): Promise<{ pdfDoc: PDFDocumentProxy; page: PDFPageProxy }> {
  const pdfDoc = await getDocument({ url: fileUrl(nodeId, modifiedAt), ...PDF_LOAD_OPTIONS })
    .promise;
  const page = await pdfDoc.getPage(1);
  return { pdfDoc, page };
}

// 指定幅に収まるスケールでオフスクリーン canvas にレンダリングし JPEG blob を返す
async function renderThumbnailToBlob(page: PDFPageProxy, maxWidth: number): Promise<Blob | null> {
  const unscaledViewport = page.getViewport({ scale: 1 });
  const scale = maxWidth / unscaledViewport.width;
  const viewport = page.getViewport({ scale });
  const canvas = document.createElement("canvas");
  canvas.width = viewport.width;
  canvas.height = viewport.height;
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    throw new Error("Canvas 2D context not available");
  }
  await page.render({ canvasContext: ctx, viewport, canvas }).promise;
  return await new Promise<Blob | null>((resolve) => {
    canvas.toBlob(resolve, "image/jpeg", 0.8);
  });
}

export function usePdfThumbnail(
  nodeId: string,
  enabled: boolean,
  modifiedAt: number | null = null,
): PdfThumbnailResult {
  const key = cacheKey(nodeId, modifiedAt);
  // 初期値をキャッシュから引くことで、再マウント時に 1 フレームも空表示にならない
  // (LRU の参照順更新は副作用なのでレンダー中には行わず effect 側に任せる)
  const [url, setUrl] = useState<string | null>(
    () => (enabled ? thumbnailCache.get(key) : undefined) ?? null,
  );
  const [isLoading, setIsLoading] = useState(false);
  const [hasError, setHasError] = useState(false);

  useEffect(() => {
    if (!enabled) {
      return;
    }

    const cached = readCache(key);
    if (cached) {
      setUrl(cached);
      setIsLoading(false);
      setHasError(false);
      return;
    }

    let cancelled = false;
    let pdfDoc: PDFDocumentProxy | null = null;

    const generate = async () => {
      // 別 PDF に切り替わった直後に前の画像を見せ続けないよう明示的にリセットする
      setUrl(null);
      setIsLoading(true);
      setHasError(false);
      try {
        const loaded = await loadPdfThumbnailPage(nodeId, modifiedAt);
        if (cancelled) {
          return;
        }
        ({ pdfDoc } = loaded);
        const blob = await renderThumbnailToBlob(loaded.page, THUMB_WIDTH);
        if (cancelled || !blob) {
          return;
        }
        const blobUrl = URL.createObjectURL(blob);
        writeCache(key, blobUrl);
        setUrl(blobUrl);
      } catch {
        if (!cancelled) {
          setHasError(true);
        }
      } finally {
        if (!cancelled) {
          setIsLoading(false);
        }
        pdfDoc?.destroy();
      }
    };

    generate();

    // blob URL はキャッシュが所有するため、アンマウントでは revoke しない
    return () => {
      cancelled = true;
    };
  }, [key, nodeId, enabled, modifiedAt]);

  return { url, isLoading, hasError };
}
