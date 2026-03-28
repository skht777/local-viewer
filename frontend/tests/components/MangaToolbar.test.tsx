import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MangaToolbar } from "../../src/components/MangaToolbar";

const defaultProps = {
  currentIndex: 0,
  totalCount: 10,
  zoomLevel: 100,
  scrollSpeed: 1.0,
  onScrollToImage: vi.fn(),
  onZoomIn: vi.fn(),
  onZoomOut: vi.fn(),
  onZoomChange: vi.fn(),
  onScrollSpeedChange: vi.fn(),
  onToggleFullscreen: vi.fn(),
  onClose: vi.fn(),
};

describe("MangaToolbar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  test("ズーム倍率が % で表示される", () => {
    render(<MangaToolbar {...defaultProps} zoomLevel={150} />);
    expect(screen.getByText("150%")).toBeInTheDocument();
  });

  test("+ ボタンで onZoomIn が呼ばれる", async () => {
    render(<MangaToolbar {...defaultProps} />);
    await userEvent.click(screen.getByRole("button", { name: /ズームイン/i }));
    expect(defaultProps.onZoomIn).toHaveBeenCalledOnce();
  });

  test("- ボタンで onZoomOut が呼ばれる", async () => {
    render(<MangaToolbar {...defaultProps} />);
    await userEvent.click(screen.getByRole("button", { name: /ズームアウト/i }));
    expect(defaultProps.onZoomOut).toHaveBeenCalledOnce();
  });

  test("ページセレクトで onScrollToImage が呼ばれる", async () => {
    render(<MangaToolbar {...defaultProps} />);
    const select = screen.getByRole("combobox", { name: /ページ選択/i });
    await userEvent.selectOptions(select, "3");
    expect(defaultProps.onScrollToImage).toHaveBeenCalledWith(3);
  });

  test("閉じるボタンで onClose が呼ばれる", async () => {
    render(<MangaToolbar {...defaultProps} />);
    await userEvent.click(screen.getByRole("button", { name: /閉じる/i }));
    expect(defaultProps.onClose).toHaveBeenCalledOnce();
  });

  test("フルスクリーンボタンで onToggleFullscreen が呼ばれる", async () => {
    render(<MangaToolbar {...defaultProps} />);
    await userEvent.click(screen.getByRole("button", { name: /フルスクリーン/i }));
    expect(defaultProps.onToggleFullscreen).toHaveBeenCalledOnce();
  });

  test("スクロール速度が表示される", () => {
    render(<MangaToolbar {...defaultProps} scrollSpeed={2.0} />);
    expect(screen.getByText("2x")).toBeInTheDocument();
  });

  test("ズーム倍率に data-testid=manga-zoom-level がある", () => {
    render(<MangaToolbar {...defaultProps} zoomLevel={100} />);
    expect(screen.getByTestId("manga-zoom-level")).toHaveTextContent("100%");
  });

  test("スクロール速度に data-testid=manga-scroll-speed-label がある", () => {
    render(<MangaToolbar {...defaultProps} scrollSpeed={1.5} />);
    expect(screen.getByTestId("manga-scroll-speed-label")).toHaveTextContent("1.5x");
  });
});
