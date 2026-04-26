import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router-dom";
import { CgViewer } from "../../src/components/CgViewer";
import type { BrowseEntry } from "../../src/types/api";
import { useViewerStore } from "../../src/stores/viewerStore";
import { renderWithProviders } from "../helpers/renderWithProviders";

// jsdom は scrollIntoView / requestFullscreen 未実装
beforeEach(() => {
  Element.prototype.scrollIntoView = vi.fn();
  document.documentElement.requestFullscreen = vi.fn().mockResolvedValue(undefined);
  document.exitFullscreen = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(document, "fullscreenElement", {
    value: null,
    writable: true,
    configurable: true,
  });
  useViewerStore.setState({ fitMode: "width", spreadMode: "single", isSidebarOpen: true });
});

function makeImage(id: string, name: string): BrowseEntry {
  return {
    node_id: id,
    name,
    kind: "image",
    size_bytes: 1024,
    mime_type: "image/jpeg",
    child_count: null,
    modified_at: null,
    preview_node_ids: null,
  };
}

const images = [makeImage("a", "img1.jpg"), makeImage("b", "img2.jpg"), makeImage("c", "img3.jpg")];

const defaultProps = {
  images,
  currentIndex: 0,
  setName: "photos",
  parentNodeId: null,
  currentNodeId: null,
  mode: "cg" as const,
  onIndexChange: () => {},
  onClose: () => {},
};

describe("CgViewer", () => {
  test("画像が表示される", () => {
    renderWithProviders(<CgViewer {...defaultProps} />);
    const imgs = screen.getAllByRole("img", { name: "img1.jpg" });
    const mainImg = imgs.find((img) => img.getAttribute("draggable") === "false");
    expect(mainImg).toHaveAttribute("src", "/api/file/a");
  });

  test("ページカウンターが表示される", () => {
    renderWithProviders(<CgViewer {...defaultProps} currentIndex={1} />);
    expect(screen.getByText("photos 2 / 3")).toBeInTheDocument();
  });

  test("ツールバーが表示される", () => {
    renderWithProviders(<CgViewer {...defaultProps} />);
    expect(screen.getByRole("button", { name: /幅/i })).toBeInTheDocument();
  });

  test("画像の右半分クリックで次の画像に進む", async () => {
    const onIndexChange = vi.fn();
    renderWithProviders(<CgViewer {...defaultProps} onIndexChange={onIndexChange} />);
    const imgArea = screen.getByTestId("cg-image-area");
    imgArea.getBoundingClientRect = vi.fn().mockReturnValue({ left: 0, width: 800 });
    fireEvent.click(imgArea, { clientX: 600 });
    expect(onIndexChange).toHaveBeenCalledWith(1);
  });

  test("閉じるボタンで onClose が呼ばれる", async () => {
    const onClose = vi.fn();
    renderWithProviders(<CgViewer {...defaultProps} onClose={onClose} />);
    await userEvent.click(screen.getByRole("button", { name: /閉じる/i }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  // 回帰防止: ページ送り時に React が <img> を unmount/mount して空白フレームを
  // 出さないこと（key を表示位置ベースにすることで DOM 再利用される仕様）。
  // 参照: 975d594 fix(frontend): CGモードのページ移動時の画像ちらつきを解消
  describe("DOM identity 維持（ちらつき回帰防止）", () => {
    function makeWrapper() {
      const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      return function Wrapper({ children }: { children: ReactNode }) {
        return (
          <QueryClientProvider client={queryClient}>
            <MemoryRouter>{children}</MemoryRouter>
          </QueryClientProvider>
        );
      };
    }

    function findMainImages(): HTMLElement[] {
      return screen.getAllByRole("img").filter((img) => img.getAttribute("draggable") === "false");
    }

    test("single モードで currentIndex を進めてもメイン img の DOM ノードが同一", () => {
      const Wrapper = makeWrapper();
      const { rerender } = render(
        <Wrapper>
          <CgViewer {...defaultProps} currentIndex={0} />
        </Wrapper>,
      );

      const before = findMainImages();
      expect(before).toHaveLength(1);
      expect(before[0]).toHaveAttribute("src", "/api/file/a");

      rerender(
        <Wrapper>
          <CgViewer {...defaultProps} currentIndex={1} />
        </Wrapper>,
      );

      const after = findMainImages();
      expect(after).toHaveLength(1);
      // DOM ノード再利用の確認: 同一参照であること
      expect(after[0]).toBe(before[0]);
      // src だけが切り替わる
      expect(after[0]).toHaveAttribute("src", "/api/file/b");
    });

    test("見開き spread モードでも両スロットの img DOM が再利用される", () => {
      useViewerStore.setState({ spreadMode: "spread" });
      const four = [
        makeImage("a", "1.jpg"),
        makeImage("b", "2.jpg"),
        makeImage("c", "3.jpg"),
        makeImage("d", "4.jpg"),
      ];
      const Wrapper = makeWrapper();
      const { rerender } = render(
        <Wrapper>
          <CgViewer {...defaultProps} images={four} currentIndex={0} />
        </Wrapper>,
      );

      const before = findMainImages();
      expect(before).toHaveLength(2);
      expect(before[0]).toHaveAttribute("src", "/api/file/a");
      expect(before[1]).toHaveAttribute("src", "/api/file/b");

      rerender(
        <Wrapper>
          <CgViewer {...defaultProps} images={four} currentIndex={2} />
        </Wrapper>,
      );

      const after = findMainImages();
      expect(after).toHaveLength(2);
      // 左右スロット個別に DOM 再利用を確認
      expect(after[0]).toBe(before[0]);
      expect(after[1]).toBe(before[1]);
      expect(after[0]).toHaveAttribute("src", "/api/file/c");
      expect(after[1]).toHaveAttribute("src", "/api/file/d");
    });
  });
});
