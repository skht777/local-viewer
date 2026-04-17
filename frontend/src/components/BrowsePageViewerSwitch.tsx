// ビューワー分岐レンダリング
// - PDF ビューワー (CG / マンガ) を先に判定
// - トランジション中はローディングオーバーレイを表示
// - 画像ビューワー (CG / マンガ) を判定
// - いずれも該当しない場合は null を返し、BrowsePage 側のブラウズ UI に任せる
//
// Finding 2 対応: BrowsePage.tsx から viewer render スイッチを抽出し、
// 履歴モデル 3 点セット (viewerTransitionId / replace:true / viewerOrigin) を
// props 経由で一貫して渡す。

import { CgViewer } from "./CgViewer";
import { MangaViewer } from "./MangaViewer";
import { PdfCgViewer } from "./PdfCgViewer";
import { PdfMangaViewer } from "./PdfMangaViewer";
import type { BrowseEntry } from "../types/api";
import type { SortOrder, ViewerMode } from "../utils/viewerNavigation";

interface DirAncestor {
  node_id: string;
  name: string;
}

interface BrowseData {
  current_name?: string | null;
  current_node_id?: string | null;
  parent_node_id?: string | null;
  ancestors?: DirAncestor[];
  entries?: BrowseEntry[];
}

interface BrowsePageViewerSwitchProps {
  nodeId: string | undefined;
  data: BrowseData | undefined;
  mode: ViewerMode;
  sort: SortOrder;
  // ビューワー可視状態
  isPdfViewerOpen: boolean;
  isViewerOpen: boolean;
  pdfNodeId: string | null;
  pdfPage: number;
  index: number;
  // トランジション ID（セットジャンプ中）
  viewerTransitionId: number;
  // 画像リスト（常に名前昇順）
  viewerImages: BrowseEntry[];
  // 副作用ハンドラ
  setIndex: (index: number) => void;
  setPdfPage: (page: number) => void;
  closeViewer: () => void;
  closePdfViewer: () => void;
}

/**
 * ビューワー表示状態に応じたコンポーネントを返す。
 *
 * - PDF 表示中 → `PdfCgViewer` / `PdfMangaViewer`
 * - トランジション中 → ローディングオーバーレイ
 * - 画像表示中 → `CgViewer` / `MangaViewer`
 * - いずれでもない → `null`（ブラウズ UI を表示する側の責務）
 */
export function BrowsePageViewerSwitch({
  nodeId,
  data,
  mode,
  sort,
  isPdfViewerOpen,
  isViewerOpen,
  pdfNodeId,
  pdfPage,
  index,
  viewerTransitionId,
  viewerImages,
  setIndex,
  setPdfPage,
  closeViewer,
  closePdfViewer,
}: BrowsePageViewerSwitchProps): React.ReactElement | null {
  // PDF ビューワー表示中 (画像ビューワーより先に判定)
  if (isPdfViewerOpen && pdfNodeId) {
    const pdfEntry = (data?.entries ?? []).find((e) => e.node_id === pdfNodeId);
    const pdfName = pdfEntry?.name ?? "";
    const commonProps = {
      pdfNodeId,
      pdfName,
      parentNodeId: data?.current_node_id ?? nodeId ?? null,
      ancestors: data?.ancestors,
      initialPage: pdfPage,
      mode,
      sort,
      onPageChange: setPdfPage,
      onClose: closePdfViewer,
    };
    if (mode === "manga") {
      return <PdfMangaViewer {...commonProps} />;
    }
    return <PdfCgViewer {...commonProps} />;
  }

  // セットジャンプのトランジション中: ローディングオーバーレイを表示
  if (viewerTransitionId > 0) {
    return (
      <div
        data-testid="viewer-transition"
        className="fixed inset-0 z-50 flex items-center justify-center bg-black"
      >
        <div className="text-gray-400">読み込み中...</div>
      </div>
    );
  }

  // 画像ビューワー表示中（viewerImages は常に名前昇順）
  if (isViewerOpen && viewerImages.length > 0) {
    const safeIndex = Math.max(0, Math.min(index, viewerImages.length - 1));
    const commonProps = {
      images: viewerImages,
      currentIndex: safeIndex,
      setName: data?.current_name ?? "",
      parentNodeId: data?.parent_node_id ?? null,
      currentNodeId: data?.current_node_id ?? null,
      ancestors: data?.ancestors,
      mode,
      sort,
      onIndexChange: setIndex,
      onClose: closeViewer,
    };

    if (mode === "manga") {
      return <MangaViewer {...commonProps} />;
    }
    return <CgViewer {...commonProps} />;
  }

  return null;
}
