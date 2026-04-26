// 検索結果ページのコールバック群
// - handleImageClick: filteredImages 基準 index → viewerImages 昇順 index に変換して URL を組み立てる
// - handlePdfClick: ?pdf= 付きで PDF ビューワーを開く
// - handleKindChange: kind フィルタ更新 (viewer 関連パラメータをリセット)
// - handleSortChange: sort 更新 (relevance ならパラメータ削除)
// - handleNavigate: directory/archive クリックで /browse へ遷移

import { useCallback } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useViewerStore } from "../stores/viewerStore";
import type { SearchSort } from "./api/browseQueries";
import type { BrowseEntry } from "../types/api";
import type { ViewerTab } from "../utils/viewerNavigation";

export interface SearchResultsCallbacks {
  handleImageClick: (browseIndex: number) => void;
  handlePdfClick: (pdfNodeId: string) => void;
  handleKindChange: (newKind: string | null) => void;
  handleSortChange: (newSort: SearchSort) => void;
  handleNavigate: (id: string, options?: { tab?: ViewerTab }) => void;
}

interface UseSearchResultsCallbacksProps {
  filteredImages: BrowseEntry[];
  viewerIndexMap: Map<string, number>;
}

// /search 起点として viewer origin を保存する
function saveSearchOrigin(searchParams: URLSearchParams): void {
  const search = searchParams.toString() ? `?${searchParams.toString()}` : "";
  useViewerStore.getState().setViewerOrigin({ pathname: "/search", search });
}

export function useSearchResultsCallbacks({
  filteredImages,
  viewerIndexMap,
}: UseSearchResultsCallbacksProps): SearchResultsCallbacks {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();

  const handleImageClick = useCallback(
    (browseIndex: number) => {
      const img = filteredImages[browseIndex];
      if (!img) {
        return;
      }
      const viewerIdx = viewerIndexMap.get(img.node_id) ?? 0;
      const next = new URLSearchParams(searchParams);
      next.set("tab", "images");
      next.set("index", String(viewerIdx));
      next.delete("pdf");
      next.delete("page");
      saveSearchOrigin(searchParams);
      setSearchParams(next);
    },
    [filteredImages, viewerIndexMap, searchParams, setSearchParams],
  );

  const handlePdfClick = useCallback(
    (pdfNodeId: string) => {
      const next = new URLSearchParams(searchParams);
      next.set("pdf", pdfNodeId);
      next.set("page", "1");
      next.delete("index");
      next.delete("tab");
      saveSearchOrigin(searchParams);
      setSearchParams(next);
    },
    [searchParams, setSearchParams],
  );

  const handleKindChange = useCallback(
    (newKind: string | null) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          if (newKind) {
            next.set("kind", newKind);
          } else {
            next.delete("kind");
          }
          // viewer 関連はリセット
          next.delete("index");
          next.delete("pdf");
          next.delete("page");
          next.delete("tab");
          return next;
        },
        { replace: true },
      );
    },
    [setSearchParams],
  );

  const handleSortChange = useCallback(
    (newSort: SearchSort) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          if (newSort === "relevance") {
            next.delete("sort");
          } else {
            next.set("sort", newSort);
          }
          return next;
        },
        { replace: true },
      );
    },
    [setSearchParams],
  );

  const handleNavigate = useCallback(
    (id: string, options?: { tab?: ViewerTab }) => {
      const tab = options?.tab;
      const browseSearch = tab && tab !== "filesets" ? `?tab=${tab}` : "";
      navigate(`/browse/${id}${browseSearch}`);
    },
    [navigate],
  );

  return { handleImageClick, handlePdfClick, handleKindChange, handleSortChange, handleNavigate };
}
