// BrowsePageViewerSwitch のビューワー分岐 + セットジャンプ remount 検証
// - PDF セット間ジャンプ（同一親ディレクトリ内の兄弟 PDF）では route が変わらず
//   ?pdf= のみ変化する。このとき PdfCgViewer / PdfMangaViewer は別 PDF として
//   remount され、initialPage（=1）にリセットされる必要がある
// - 子ビューワーは stub 化し、実コンポーネントの「initialPage を mount 時のみ取り込む」
//   非制御 state（usePdfPageState / virtualizer initialIndex）を useState で再現する

import { render, screen } from "@testing-library/react";
import { describe, test, expect, vi } from "vitest";
import { BrowsePageViewerSwitch } from "../../src/components/BrowsePageViewerSwitch";
import type { BrowseEntry } from "../../src/types/api";

// 子ビューワーを stub 化（実 PdfCanvas / pdfjs を読み込まない）
vi.mock("../../src/components/PdfCgViewer", async () => {
  const React = await vi.importActual<typeof import("react")>("react");
  return {
    PdfCgViewer: ({ pdfNodeId, initialPage }: { pdfNodeId: string; initialPage: number }) => {
      // usePdfPageState と同じく initialPage は mount 時のみ取り込む
      const [displayedPage] = React.useState(initialPage);
      return React.createElement("div", {
        "data-testid": "pdf-cg-stub",
        "data-node": pdfNodeId,
        "data-displayed-page": String(displayedPage),
      });
    },
  };
});

vi.mock("../../src/components/PdfMangaViewer", async () => {
  const React = await vi.importActual<typeof import("react")>("react");
  return {
    PdfMangaViewer: ({ pdfNodeId, initialPage }: { pdfNodeId: string; initialPage: number }) => {
      const [displayedPage] = React.useState(initialPage);
      return React.createElement("div", {
        "data-testid": "pdf-manga-stub",
        "data-node": pdfNodeId,
        "data-displayed-page": String(displayedPage),
      });
    },
  };
});

vi.mock("../../src/components/CgViewer", () => ({
  CgViewer: () => null,
}));

vi.mock("../../src/components/MangaViewer", () => ({
  MangaViewer: () => null,
}));

type SwitchProps = React.ComponentProps<typeof BrowsePageViewerSwitch>;

function pdfProps(pdfNodeId: string, pdfPage: number, mode: "cg" | "manga" = "cg"): SwitchProps {
  return {
    nodeId: "dir",
    data: { current_node_id: "dir", entries: [] as BrowseEntry[] },
    mode,
    sort: "name-asc",
    isPdfViewerOpen: true,
    isViewerOpen: false,
    pdfNodeId,
    pdfPage,
    index: -1,
    viewerTransitionId: 0,
    viewerImages: [],
    setIndex: vi.fn(),
    setPdfPage: vi.fn(),
    closeViewer: vi.fn(),
    closePdfViewer: vi.fn(),
  };
}

describe("BrowsePageViewerSwitch — PDF セット間ジャンプ", () => {
  test("別 PDF へジャンプすると PdfCgViewer は remount され initialPage にリセットされる", () => {
    // PDF A をページ 5 で表示中（A の initialPage=5 を mount 時に取り込む）
    const { rerender } = render(<BrowsePageViewerSwitch {...pdfProps("A", 5)} />);
    expect(screen.getByTestId("pdf-cg-stub").getAttribute("data-displayed-page")).toBe("5");

    // セットジャンプ相当: pdfNodeId=B / page=1（同一親ディレクトリで route 不変）
    rerender(<BrowsePageViewerSwitch {...pdfProps("B", 1)} />);
    const stub = screen.getByTestId("pdf-cg-stub");
    expect(stub.getAttribute("data-node")).toBe("B");
    // key={pdfNodeId} が無いと remount されず displayed-page=5（前 PDF のページ）が残る
    expect(stub.getAttribute("data-displayed-page")).toBe("1");
  });

  test("別 PDF へジャンプすると PdfMangaViewer も remount され initialPage にリセットされる", () => {
    const { rerender } = render(<BrowsePageViewerSwitch {...pdfProps("A", 5, "manga")} />);
    expect(screen.getByTestId("pdf-manga-stub").getAttribute("data-displayed-page")).toBe("5");

    rerender(<BrowsePageViewerSwitch {...pdfProps("B", 1, "manga")} />);
    const stub = screen.getByTestId("pdf-manga-stub");
    expect(stub.getAttribute("data-node")).toBe("B");
    expect(stub.getAttribute("data-displayed-page")).toBe("1");
  });

  test("同一 PDF 内のページ移動では remount されず内部 state を維持する", () => {
    // 同一 PDF で page だけ変化（ページ送り）。remount すると現在ページが失われるため不可
    const { rerender } = render(<BrowsePageViewerSwitch {...pdfProps("A", 1)} />);
    expect(screen.getByTestId("pdf-cg-stub").getAttribute("data-displayed-page")).toBe("1");

    // pdfNodeId 不変・page だけ変化 → remount しない（displayed-page は mount 時の 1 のまま）
    rerender(<BrowsePageViewerSwitch {...pdfProps("A", 3)} />);
    expect(screen.getByTestId("pdf-cg-stub").getAttribute("data-displayed-page")).toBe("1");
  });
});
