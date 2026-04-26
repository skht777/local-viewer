// useFileBrowserActions の振る舞い検証
// - handleAction: kind 別の主アクション分岐（directory/archive→navigate, pdf→onPdfClick, image→onImageClick）
// - handleOpen: オーバーレイ開く（directory/archive→onOpenViewer, image→onImageClick, pdf→onPdfClick）
// - handleEnter: directory/archive のみ進入
// - getOpenHandler / getEnterHandler: kind に応じた undefined or handler

import { renderHook } from "@testing-library/react";
import { useFileBrowserActions } from "../../src/hooks/useFileBrowserActions";
import type { BrowseEntry } from "../../src/types/api";

function makeEntry(kind: BrowseEntry["kind"], id: string = kind): BrowseEntry {
  return {
    node_id: id,
    name: id,
    kind,
    size_bytes: null,
    mime_type: null,
    child_count: null,
    modified_at: null,
    preview_node_ids: null,
  };
}

interface RenderOpts {
  indexMap?: Map<string, number>;
  onNavigate?: ReturnType<typeof vi.fn>;
  onImageClick?: ReturnType<typeof vi.fn>;
  onPdfClick?: ReturnType<typeof vi.fn>;
  onOpenViewer?: ReturnType<typeof vi.fn>;
}

function setup(opts: RenderOpts = {}) {
  const indexMap = opts.indexMap ?? new Map<string, number>();
  const onNavigate = opts.onNavigate ?? vi.fn();
  const onImageClick = opts.onImageClick ?? vi.fn();
  const onPdfClick = opts.onPdfClick ?? vi.fn();
  const onOpenViewer = opts.onOpenViewer ?? vi.fn();
  const { result } = renderHook(() =>
    useFileBrowserActions({ indexMap, onNavigate, onImageClick, onPdfClick, onOpenViewer }),
  );
  return { result, onNavigate, onImageClick, onPdfClick, onOpenViewer };
}

describe("useFileBrowserActions", () => {
  describe("handleAction", () => {
    test("directory は onNavigate(node_id) を呼ぶ", () => {
      const { result, onNavigate } = setup();
      result.current.handleAction(makeEntry("directory", "d1"));
      expect(onNavigate).toHaveBeenCalledWith("d1");
    });

    test("archive は onNavigate(node_id, { tab: 'images' }) を呼ぶ", () => {
      const { result, onNavigate } = setup();
      result.current.handleAction(makeEntry("archive", "a1"));
      expect(onNavigate).toHaveBeenCalledWith("a1", { tab: "images" });
    });

    test("pdf は onPdfClick(node_id) を呼ぶ", () => {
      const { result, onPdfClick } = setup();
      result.current.handleAction(makeEntry("pdf", "p1"));
      expect(onPdfClick).toHaveBeenCalledWith("p1");
    });

    test("image は indexMap で解決した index で onImageClick を呼ぶ", () => {
      const indexMap = new Map([["i1", 5]]);
      const { result, onImageClick } = setup({ indexMap });
      result.current.handleAction(makeEntry("image", "i1"));
      expect(onImageClick).toHaveBeenCalledWith(5);
    });

    test("image で indexMap に未登録のときは onImageClick を呼ばない", () => {
      const { result, onImageClick } = setup();
      result.current.handleAction(makeEntry("image", "missing"));
      expect(onImageClick).not.toHaveBeenCalled();
    });

    test("video は何も呼ばない", () => {
      const { result, onNavigate, onPdfClick, onImageClick, onOpenViewer } = setup();
      result.current.handleAction(makeEntry("video", "v1"));
      expect(onNavigate).not.toHaveBeenCalled();
      expect(onPdfClick).not.toHaveBeenCalled();
      expect(onImageClick).not.toHaveBeenCalled();
      expect(onOpenViewer).not.toHaveBeenCalled();
    });
  });

  describe("handleOpen", () => {
    test("directory は onOpenViewer(node_id) を呼ぶ", () => {
      const { result, onOpenViewer } = setup();
      result.current.handleOpen(makeEntry("directory", "d1"));
      expect(onOpenViewer).toHaveBeenCalledWith("d1");
    });

    test("archive は onOpenViewer(node_id) を呼ぶ", () => {
      const { result, onOpenViewer } = setup();
      result.current.handleOpen(makeEntry("archive", "a1"));
      expect(onOpenViewer).toHaveBeenCalledWith("a1");
    });

    test("image は onImageClick(index) を呼ぶ", () => {
      const indexMap = new Map([["i1", 2]]);
      const { result, onImageClick } = setup({ indexMap });
      result.current.handleOpen(makeEntry("image", "i1"));
      expect(onImageClick).toHaveBeenCalledWith(2);
    });

    test("pdf は onPdfClick(node_id) を呼ぶ", () => {
      const { result, onPdfClick } = setup();
      result.current.handleOpen(makeEntry("pdf", "p1"));
      expect(onPdfClick).toHaveBeenCalledWith("p1");
    });
  });

  describe("handleEnter", () => {
    test("directory/archive で navigate が呼ばれる", () => {
      const { result, onNavigate } = setup();
      result.current.handleEnter(makeEntry("directory", "d1"));
      result.current.handleEnter(makeEntry("archive", "a1"));
      expect(onNavigate).toHaveBeenCalledTimes(2);
      expect(onNavigate).toHaveBeenNthCalledWith(1, "d1");
      expect(onNavigate).toHaveBeenNthCalledWith(2, "a1", { tab: "images" });
    });

    test("image / pdf / video では何もしない", () => {
      const { result, onNavigate } = setup();
      result.current.handleEnter(makeEntry("image", "i1"));
      result.current.handleEnter(makeEntry("pdf", "p1"));
      result.current.handleEnter(makeEntry("video", "v1"));
      expect(onNavigate).not.toHaveBeenCalled();
    });
  });

  describe("getOpenHandler / getEnterHandler", () => {
    test("getOpenHandler は directory/archive/image/pdf には handler を、video には undefined を返す", () => {
      const { result } = setup();
      expect(result.current.getOpenHandler(makeEntry("directory"))).toBeDefined();
      expect(result.current.getOpenHandler(makeEntry("archive"))).toBeDefined();
      expect(result.current.getOpenHandler(makeEntry("image"))).toBeDefined();
      expect(result.current.getOpenHandler(makeEntry("pdf"))).toBeDefined();
      expect(result.current.getOpenHandler(makeEntry("video"))).toBeUndefined();
      expect(result.current.getOpenHandler(makeEntry("other"))).toBeUndefined();
    });

    test("getEnterHandler は directory/archive のみ handler を返す", () => {
      const { result } = setup();
      expect(result.current.getEnterHandler(makeEntry("directory"))).toBeDefined();
      expect(result.current.getEnterHandler(makeEntry("archive"))).toBeDefined();
      expect(result.current.getEnterHandler(makeEntry("image"))).toBeUndefined();
      expect(result.current.getEnterHandler(makeEntry("pdf"))).toBeUndefined();
      expect(result.current.getEnterHandler(makeEntry("video"))).toBeUndefined();
    });
  });
});
