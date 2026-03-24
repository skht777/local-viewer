import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { CgViewer } from "../../src/components/CgViewer";
import type { BrowseEntry } from "../../src/types/api";
import { useViewerStore } from "../../src/stores/viewerStore";

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
  return { node_id: id, name, kind: "image", size_bytes: 1024, mime_type: "image/jpeg", child_count: null };
}

const images = [makeImage("a", "img1.jpg"), makeImage("b", "img2.jpg"), makeImage("c", "img3.jpg")];

describe("CgViewer", () => {
  test("画像が表示される", () => {
    render(
      <CgViewer
        images={images}
        currentIndex={0}
        setName="photos"
        parentNodeId={null}
        onIndexChange={() => {}}
        onClose={() => {}}
      />,
    );
    // メイン画像は draggable=false で区別
    const imgs = screen.getAllByRole("img", { name: "img1.jpg" });
    const mainImg = imgs.find((img) => img.getAttribute("draggable") === "false");
    expect(mainImg).toHaveAttribute("src", "/api/file/a");
  });

  test("ページカウンターが表示される", () => {
    render(
      <CgViewer
        images={images}
        currentIndex={1}
        setName="photos"
        parentNodeId={null}
        onIndexChange={() => {}}
        onClose={() => {}}
      />,
    );
    expect(screen.getByText("photos 2 / 3")).toBeInTheDocument();
  });

  test("ツールバーが表示される", () => {
    render(
      <CgViewer
        images={images}
        currentIndex={0}
        setName="photos"
        parentNodeId={null}
        onIndexChange={() => {}}
        onClose={() => {}}
      />,
    );
    // ツールバーのフィットボタンが存在する
    expect(screen.getByRole("button", { name: /幅/i })).toBeInTheDocument();
  });

  test("画像の右半分クリックで次の画像に進む", async () => {
    const onIndexChange = vi.fn();
    render(
      <CgViewer
        images={images}
        currentIndex={0}
        setName="photos"
        parentNodeId={null}
        onIndexChange={onIndexChange}
        onClose={() => {}}
      />,
    );
    // メイン画像エリアをクリック（右半分をシミュレート）
    const imgArea = screen.getByTestId("cg-image-area");
    imgArea.getBoundingClientRect = vi.fn().mockReturnValue({ left: 0, width: 800 });
    // fireEvent で clientX を直接指定
    fireEvent.click(imgArea, { clientX: 600 });
    expect(onIndexChange).toHaveBeenCalledWith(1);
  });

  test("閉じるボタンで onClose が呼ばれる", async () => {
    const onClose = vi.fn();
    render(
      <CgViewer
        images={images}
        currentIndex={0}
        setName="photos"
        parentNodeId={null}
        onIndexChange={() => {}}
        onClose={onClose}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: /閉じる/i }));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
