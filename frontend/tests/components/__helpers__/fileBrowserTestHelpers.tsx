// FileBrowser テスト共通ヘルパー
// - 4 kind 標準フィクスチャ + 派生関数
// - render ごとに新規 QueryClient を生成（テスト独立性維持）
// - IntersectionObserver mock ファクトリ
//
// 注意: vi.mock("../../src/lib/pdfjs", ...) は Vitest hoist 仕様のため
// 各テストファイル冒頭で個別宣言が必要（ここでは扱わない）

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import type { ReactElement } from "react";
import type { BrowseEntry } from "../../../src/types/api";

// 4 kind 標準フィクスチャ（directory/image/video/pdf）
export const mockEntries: BrowseEntry[] = [
  {
    node_id: "dir1",
    name: "photos",
    kind: "directory",
    size_bytes: null,
    mime_type: null,
    child_count: 10,
    modified_at: 1_700_000_000,
    preview_node_ids: null,
  },
  {
    node_id: "file1",
    name: "image.jpg",
    kind: "image",
    size_bytes: 2048,
    mime_type: "image/jpeg",
    child_count: null,
    modified_at: 1_700_000_100,
    preview_node_ids: null,
  },
  {
    node_id: "file2",
    name: "movie.mp4",
    kind: "video",
    size_bytes: 10_240,
    mime_type: "video/mp4",
    child_count: null,
    modified_at: 1_700_000_200,
    preview_node_ids: null,
  },
  {
    node_id: "file3",
    name: "doc.pdf",
    kind: "pdf",
    size_bytes: 4096,
    mime_type: "application/pdf",
    child_count: null,
    modified_at: 1_700_000_300,
    preview_node_ids: null,
  },
];

interface ImageEntryOverrides {
  node_id: string;
  name: string;
  modified_at?: number | null;
  size_bytes?: number;
}

// 画像エントリー生成（ソート系テスト用）
export function makeImageEntry(o: ImageEntryOverrides): BrowseEntry {
  return {
    node_id: o.node_id,
    name: o.name,
    kind: "image",
    size_bytes: o.size_bytes ?? 100,
    mime_type: "image/jpeg",
    child_count: null,
    modified_at: o.modified_at ?? null,
    preview_node_ids: null,
  };
}

interface DirectoryEntryOverrides {
  node_id: string;
  name: string;
  child_count?: number;
  modified_at?: number | null;
}

// ディレクトリエントリー生成
export function makeDirectoryEntry(o: DirectoryEntryOverrides): BrowseEntry {
  return {
    node_id: o.node_id,
    name: o.name,
    kind: "directory",
    size_bytes: null,
    mime_type: null,
    child_count: o.child_count ?? 5,
    modified_at: o.modified_at ?? null,
    preview_node_ids: null,
  };
}

interface ArchiveEntryOverrides {
  node_id: string;
  name: string;
  size_bytes?: number;
  modified_at?: number | null;
}

// アーカイブエントリー生成
export function makeArchiveEntry(o: ArchiveEntryOverrides): BrowseEntry {
  return {
    node_id: o.node_id,
    name: o.name,
    kind: "archive",
    size_bytes: o.size_bytes ?? 500,
    mime_type: "application/zip",
    child_count: null,
    modified_at: o.modified_at ?? null,
    preview_node_ids: null,
  };
}

interface PdfEntryOverrides {
  node_id: string;
  name: string;
  size_bytes?: number;
  modified_at?: number | null;
}

// PDF エントリー生成
export function makePdfEntry(o: PdfEntryOverrides): BrowseEntry {
  return {
    node_id: o.node_id,
    name: o.name,
    kind: "pdf",
    size_bytes: o.size_bytes ?? 100,
    mime_type: "application/pdf",
    child_count: null,
    modified_at: o.modified_at ?? null,
    preview_node_ids: null,
  };
}

// render ごとに新規 QueryClient を生成（テスト独立性維持）
// - 共有 QueryClient は state リーク源になるため避ける
// - retry: false でテストを高速化
export function renderFileBrowser(ui: ReactElement) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}

// IntersectionObserver の最小モック
// - observe 時に isIntersecting: true で即座にコールバック発火
// - onLoadMore 発火条件などの検証で使用
//
// 戻り値: { install, restore } - install で globalThis を差し替え、restore で原状復帰
export function installMockIntersectionObserver(): {
  restore: () => void;
} {
  const original = globalThis.IntersectionObserver;
  let capturedCallback: IntersectionObserverCallback | null = null;
  globalThis.IntersectionObserver = class MockIO {
    // IntersectionObserver の API シグネチャはコールバックベースのため async/await 不可
    // oxlint-disable-next-line promise/prefer-await-to-callbacks
    constructor(callback: IntersectionObserverCallback) {
      capturedCallback = callback;
    }
    observe() {
      capturedCallback?.(
        [{ isIntersecting: true } as IntersectionObserverEntry],
        this as unknown as IntersectionObserver,
      );
    }
    disconnect() {}
    unobserve() {}
    takeRecords() {
      return [];
    }
    root = null;
    rootMargin = "";
    thresholds = [];
  } as unknown as typeof IntersectionObserver;
  return {
    restore() {
      globalThis.IntersectionObserver = original;
    },
  };
}
