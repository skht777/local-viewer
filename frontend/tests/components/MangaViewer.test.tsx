import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MangaViewer } from "../../src/components/MangaViewer";
import type { BrowseEntry } from "../../src/types/api";
import { useViewerStore } from "../../src/stores/viewerStore";
import { renderWithProviders } from "../helpers/renderWithProviders";

// jsdom は scrollIntoView / requestFullscreen / ResizeObserver 未実装
beforeEach(() => {
  localStorage.clear();
  Element.prototype.scrollIntoView = vi.fn();
  document.documentElement.requestFullscreen = vi.fn().mockResolvedValue(undefined);
  document.exitFullscreen = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(document, "fullscreenElement", {
    value: null,
    writable: true,
    configurable: true,
  });
  // ResizeObserver モック（@tanstack/react-virtual が使用）
  global.ResizeObserver = class {
    observe = vi.fn();
    unobserve = vi.fn();
    disconnect = vi.fn();
  } as unknown as typeof ResizeObserver;
  useViewerStore.setState({
    isSidebarOpen: true,
    zoomLevel: 100,
    scrollSpeed: 1.0,
  });
});

function makeImage(id: string, name: string): BrowseEntry {
  return { node_id: id, name, kind: "image", size_bytes: 1024, mime_type: "image/jpeg", child_count: null };
}

const images = [makeImage("a", "img1.jpg"), makeImage("b", "img2.jpg"), makeImage("c", "img3.jpg")];

const defaultProps = {
  images,
  currentIndex: 0,
  setName: "photos",
  parentNodeId: null,
  currentNodeId: null,
  mode: "manga" as const,
  onIndexChange: vi.fn(),
  onModeChange: vi.fn(),
  onClose: vi.fn(),
};

describe("MangaViewer", () => {
  test("スクロールエリアが表示される", () => {
    renderWithProviders(<MangaViewer {...defaultProps} />);
    expect(screen.getByTestId("manga-scroll-area")).toBeInTheDocument();
  });

  test("ツールバーが表示される", () => {
    renderWithProviders(<MangaViewer {...defaultProps} />);
    expect(screen.getByRole("combobox", { name: /ページ選択/i })).toBeInTheDocument();
  });

  test("ページカウンターが表示される", () => {
    renderWithProviders(<MangaViewer {...defaultProps} />);
    expect(screen.getByText(/photos/)).toBeInTheDocument();
  });

  test("閉じるボタンで onClose が呼ばれる", async () => {
    const onClose = vi.fn();
    renderWithProviders(<MangaViewer {...defaultProps} onClose={onClose} />);
    await userEvent.click(screen.getByRole("button", { name: /閉じる/i }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  test("ズーム倍率が表示される", () => {
    renderWithProviders(<MangaViewer {...defaultProps} />);
    expect(screen.getByText("100%")).toBeInTheDocument();
  });
});
