// ビューワーナビゲーションの pure helper
// - URL 構築 (open 系) と close 先の決定 (close 系) のみを担当
// - navigate / setSearchParams / setViewerOrigin 等の副作用は呼び出し側 hook が実行
//
// Finding 3 + レビュー H2 対応: useViewerParams と useOpenViewerFromEntry の
// 両方から同一ロジックを参照できるよう、hook ではなく utils として提供する

import { updateSearchParams } from "./searchParamUpdater";

export type ViewerTab = "filesets" | "images" | "videos";
export type ViewerMode = "cg" | "manga";
export type SortOrder = "name-asc" | "name-desc" | "date-asc" | "date-desc";

export const VALID_SORT_ORDERS: Set<string> = new Set([
  "name-asc",
  "name-desc",
  "date-asc",
  "date-desc",
]);

export interface ViewerOrigin {
  nodeId: string;
  search: string;
}

/**
 * 画像ビューワーを開く URLSearchParams を構築する。
 * - tab=images + index を設定し pdf/page を削除する
 */
export function buildOpenImageSearch(
  current: URLSearchParams,
  options: { index: number; tab?: ViewerTab },
): URLSearchParams {
  return updateSearchParams(current, (next) => {
    next.set("tab", options.tab ?? "images");
    next.set("index", String(options.index));
    next.delete("pdf");
    next.delete("page");
  });
}

/**
 * PDF ビューワーを開く URLSearchParams を構築する。
 * - pdf/page を設定し index/tab を削除する
 */
export function buildOpenPdfSearch(
  current: URLSearchParams,
  options: { pdfNodeId: string; page?: number },
): URLSearchParams {
  return updateSearchParams(current, (next) => {
    next.set("pdf", options.pdfNodeId);
    next.set("page", String(options.page ?? 1));
    next.delete("index");
    next.delete("tab");
  });
}

/**
 * 画像ビューワーを現在ディレクトリに留めたまま閉じる URLSearchParams を構築する。
 * - origin が無い deep link 系の fallback 用
 */
export function buildCloseImageSearch(current: URLSearchParams): URLSearchParams {
  return updateSearchParams(current, (next) => {
    next.delete("index");
  });
}

/**
 * PDF ビューワーを現在ディレクトリに留めたまま閉じる URLSearchParams を構築する。
 */
export function buildClosePdfSearch(current: URLSearchParams): URLSearchParams {
  return updateSearchParams(current, (next) => {
    next.delete("pdf");
    next.delete("page");
  });
}

/**
 * browse スコープのパラメータ（mode / tab / sort / 任意で index）のみを残した search 文字列を返す。
 * viewer スコープ（pdf/page 等）は除外する。
 */
export function buildBrowseSearch(
  current: URLSearchParams,
  overrides?: { tab?: string; index?: number },
): string {
  const next = new URLSearchParams();
  const currentMode = current.get("mode");
  if (currentMode === "manga") next.set("mode", "manga");
  const nextTab = overrides?.tab ?? current.get("tab");
  if (nextTab && nextTab !== "filesets") next.set("tab", nextTab);
  const currentSort = current.get("sort");
  if (currentSort && VALID_SORT_ORDERS.has(currentSort) && currentSort !== "name-asc") {
    next.set("sort", currentSort);
  }
  if (overrides?.index != null) next.set("index", String(overrides.index));
  return next.toString() ? `?${next}` : "";
}

/**
 * ビューワーを閉じる時の navigate 先を決定する。
 * - origin があれば `{nodeId, search}` を返す（ナビゲーション先）
 * - 無ければ null（呼び出し側は現在ディレクトリに留める fallback を使う）
 */
export function resolveCloseTarget(origin: ViewerOrigin | null): ViewerOrigin | null {
  return origin;
}
