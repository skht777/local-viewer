import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MangaToolbar } from "../../src/components/MangaToolbar";

const defaultProps = {
  currentIndex: 0,
  totalCount: 10,
  zoomLevel: 100,
  scrollSpeed: 1.0,
  setName: "test-set",
  onScrollToImage: vi.fn(),
  onZoomIn: vi.fn(),
  onZoomOut: vi.fn(),
  onZoomChange: vi.fn(),
  onScrollSpeedChange: vi.fn(),
  onToggleFullscreen: vi.fn(),
  onClose: vi.fn(),
  onPrevSet: vi.fn(),
  onNextSet: vi.fn(),
  isSetJumpDisabled: false,
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

  test("前のセットボタンに data-testid=manga-prev-set-btn と aria-label がある", () => {
    render(<MangaToolbar {...defaultProps} />);
    const btn = screen.getByTestId("manga-prev-set-btn");
    expect(btn).toBeInTheDocument();
    expect(btn).toHaveAttribute("aria-label", "前のセットへ");
  });

  test("次のセットボタンに data-testid=manga-next-set-btn と aria-label がある", () => {
    render(<MangaToolbar {...defaultProps} />);
    const btn = screen.getByTestId("manga-next-set-btn");
    expect(btn).toBeInTheDocument();
    expect(btn).toHaveAttribute("aria-label", "次のセットへ");
  });

  test("前のセットボタンクリックで onPrevSet が呼ばれる", async () => {
    const onPrevSet = vi.fn();
    render(<MangaToolbar {...defaultProps} onPrevSet={onPrevSet} />);
    await userEvent.click(screen.getByTestId("manga-prev-set-btn"));
    expect(onPrevSet).toHaveBeenCalledOnce();
  });

  test("次のセットボタンクリックで onNextSet が呼ばれる", async () => {
    const onNextSet = vi.fn();
    render(<MangaToolbar {...defaultProps} onNextSet={onNextSet} />);
    await userEvent.click(screen.getByTestId("manga-next-set-btn"));
    expect(onNextSet).toHaveBeenCalledOnce();
  });

  test("isSetJumpDisabled=true のときセット間ジャンプボタンが disabled になる", () => {
    render(<MangaToolbar {...defaultProps} isSetJumpDisabled={true} />);
    expect(screen.getByTestId("manga-prev-set-btn")).toBeDisabled();
    expect(screen.getByTestId("manga-next-set-btn")).toBeDisabled();
  });

  test("isSetJumpDisabled=true のときクリックしても onPrevSet / onNextSet は呼ばれない", async () => {
    const onPrevSet = vi.fn();
    const onNextSet = vi.fn();
    render(
      <MangaToolbar
        {...defaultProps}
        onPrevSet={onPrevSet}
        onNextSet={onNextSet}
        isSetJumpDisabled={true}
      />,
    );
    await userEvent.click(screen.getByTestId("manga-prev-set-btn"));
    await userEvent.click(screen.getByTestId("manga-next-set-btn"));
    expect(onPrevSet).not.toHaveBeenCalled();
    expect(onNextSet).not.toHaveBeenCalled();
  });
});
