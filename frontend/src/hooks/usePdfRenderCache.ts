// PDF 描画キャッシュ — ImageBitmap のバイト数ベース LRU
// - キー: "${pageNumber}:${scale}" (fitMode/containerSize から算出)
// - 値: ImageBitmap
// - 概算バイト数 (width * height * 4) で上限管理 (256MB)
// - evict 時に bitmap.close() でメモリ解放
// - invalidate() で全破棄 (fitMode 変更時等)

import { useCallback, useRef } from "react";

const DEFAULT_MAX_BYTES = 256 * 1024 * 1024; // 256MB

interface CacheEntry {
  key: string;
  bitmap: ImageBitmap;
  bytes: number;
  lastAccess: number;
}

export interface PdfRenderCache {
  get: (key: string) => ImageBitmap | undefined;
  put: (key: string, bitmap: ImageBitmap) => void;
  invalidate: () => void;
}

export function usePdfRenderCache(maxBytes = DEFAULT_MAX_BYTES): PdfRenderCache {
  const entriesRef = useRef<Map<string, CacheEntry>>(new Map());
  const totalBytesRef = useRef(0);
  const accessCounterRef = useRef(0);

  const get = useCallback((key: string): ImageBitmap | undefined => {
    const entry = entriesRef.current.get(key);
    if (!entry) return undefined;
    // LRU 更新
    entry.lastAccess = ++accessCounterRef.current;
    return entry.bitmap;
  }, []);

  const evictLRU = useCallback(
    (targetBytes: number) => {
      const entries = entriesRef.current;
      // lastAccess が小さい順にソート
      const sorted = [...entries.values()].toSorted((a, b) => a.lastAccess - b.lastAccess);
      for (const entry of sorted) {
        if (totalBytesRef.current + targetBytes <= maxBytes) break;
        entry.bitmap.close();
        entries.delete(entry.key);
        totalBytesRef.current -= entry.bytes;
      }
    },
    [maxBytes],
  );

  const put = useCallback(
    (key: string, bitmap: ImageBitmap) => {
      const entries = entriesRef.current;

      // 既存エントリがあれば上書き
      const existing = entries.get(key);
      if (existing) {
        existing.bitmap.close();
        totalBytesRef.current -= existing.bytes;
        entries.delete(key);
      }

      const bytes = bitmap.width * bitmap.height * 4;

      // 単一エントリが上限を超える場合はキャッシュしない
      if (bytes > maxBytes) {
        bitmap.close();
        return;
      }

      // 空き容量がなければ LRU 追い出し
      evictLRU(bytes);

      entries.set(key, {
        key,
        bitmap,
        bytes,
        lastAccess: ++accessCounterRef.current,
      });
      totalBytesRef.current += bytes;
    },
    [evictLRU, maxBytes],
  );

  const invalidate = useCallback(() => {
    for (const entry of entriesRef.current.values()) {
      entry.bitmap.close();
    }
    entriesRef.current.clear();
    totalBytesRef.current = 0;
  }, []);

  return { get, put, invalidate };
}
