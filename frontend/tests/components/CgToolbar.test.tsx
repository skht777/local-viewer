import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { CgToolbar } from "../../src/components/CgToolbar";

describe("CgToolbar", () => {
  const defaultProps = {
    fitMode: "width" as const,
    spreadMode: "single" as const,
    currentIndex: 2,
    totalCount: 10,
    setName: "test-set",
    currentPage: 3,
    onFitWidth: vi.fn(),
    onFitHeight: vi.fn(),
    onCycleSpread: vi.fn(),
    onToggleFullscreen: vi.fn(),
    onGoTo: vi.fn(),
    onClose: vi.fn(),
    onPrevSet: vi.fn(),
    onNextSet: vi.fn(),
    isSetJumpDisabled: false,
  };

  test("フィット切替ボタンが表示される", () => {
    render(<CgToolbar {...defaultProps} />);
    expect(screen.getByRole("button", { name: /幅/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /高さ/i })).toBeInTheDocument();
  });

  test("見開き切替ボタンが表示される", () => {
    render(<CgToolbar {...defaultProps} />);
    expect(screen.getByTestId("cg-spread-btn")).toBeInTheDocument();
  });

  test("フルスクリーンボタンが表示される", () => {
    render(<CgToolbar {...defaultProps} />);
    expect(screen.getByRole("button", { name: /フルスクリーン/i })).toBeInTheDocument();
  });

  test("閉じるボタンクリックで onClose が呼ばれる", async () => {
    const onClose = vi.fn();
    render(<CgToolbar {...defaultProps} onClose={onClose} />);
    await userEvent.click(screen.getByRole("button", { name: /閉じる/i }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  test("ページセレクトが表示される", () => {
    render(<CgToolbar {...defaultProps} />);
    const select = screen.getByRole("combobox");
    expect(select).toBeInTheDocument();
  });

  test("ページセレクト変更で onGoTo が呼ばれる", async () => {
    const onGoTo = vi.fn();
    render(<CgToolbar {...defaultProps} onGoTo={onGoTo} />);
    const select = screen.getByRole("combobox");
    await userEvent.selectOptions(select, "5");
    expect(onGoTo).toHaveBeenCalledWith(5);
  });

  test("幅フィット選択時に W ボタンが aria-pressed=true になる", () => {
    render(<CgToolbar {...defaultProps} fitMode="width" />);
    const wBtn = screen.getByRole("button", { name: "幅フィット" });
    expect(wBtn).toHaveAttribute("aria-pressed", "true");
  });

  test("高さフィット選択時に H ボタンが aria-pressed=true になる", () => {
    render(<CgToolbar {...defaultProps} fitMode="height" />);
    const hBtn = screen.getByRole("button", { name: "高さフィット" });
    expect(hBtn).toHaveAttribute("aria-pressed", "true");
  });

  test("非選択のフィットボタンは aria-pressed=false になる", () => {
    render(<CgToolbar {...defaultProps} fitMode="width" />);
    const hBtn = screen.getByRole("button", { name: "高さフィット" });
    expect(hBtn).toHaveAttribute("aria-pressed", "false");
  });

  test("見開きボタンに data-testid=cg-spread-btn がある", () => {
    render(<CgToolbar {...defaultProps} />);
    expect(screen.getByTestId("cg-spread-btn")).toBeInTheDocument();
  });

  test("ページカウンターがツールバー中央に表示される", () => {
    render(<CgToolbar {...defaultProps} />);
    const counter = screen.getByTestId("page-counter");
    expect(counter).toHaveTextContent("test-set 3 / 10");
  });

  test("見開き時のページカウンターが範囲表示される", () => {
    render(<CgToolbar {...defaultProps} currentPage={3} currentPageEnd={4} />);
    const counter = screen.getByTestId("page-counter");
    expect(counter).toHaveTextContent("test-set 3-4 / 10");
  });

  test("見開きボタンにモード別の tooltip が表示される", () => {
    const { rerender } = render(<CgToolbar {...defaultProps} spreadMode="single" />);
    const btn = screen.getByTestId("cg-spread-btn");
    expect(btn).toHaveAttribute("title", "1ページ表示 (Q)");

    rerender(<CgToolbar {...defaultProps} spreadMode="spread" />);
    expect(btn).toHaveAttribute("title", "見開き表示 (Q)");

    rerender(<CgToolbar {...defaultProps} spreadMode="spread-offset" />);
    expect(btn).toHaveAttribute("title", "見開き+1 表示 (Q)");
  });

  test("前のセットボタンに data-testid=cg-prev-set-btn と aria-label がある", () => {
    render(<CgToolbar {...defaultProps} />);
    const btn = screen.getByTestId("cg-prev-set-btn");
    expect(btn).toBeInTheDocument();
    expect(btn).toHaveAttribute("aria-label", "前のセットへ");
  });

  test("次のセットボタンに data-testid=cg-next-set-btn と aria-label がある", () => {
    render(<CgToolbar {...defaultProps} />);
    const btn = screen.getByTestId("cg-next-set-btn");
    expect(btn).toBeInTheDocument();
    expect(btn).toHaveAttribute("aria-label", "次のセットへ");
  });

  test("前のセットボタンクリックで onPrevSet が呼ばれる", async () => {
    const onPrevSet = vi.fn();
    render(<CgToolbar {...defaultProps} onPrevSet={onPrevSet} />);
    await userEvent.click(screen.getByTestId("cg-prev-set-btn"));
    expect(onPrevSet).toHaveBeenCalledOnce();
  });

  test("次のセットボタンクリックで onNextSet が呼ばれる", async () => {
    const onNextSet = vi.fn();
    render(<CgToolbar {...defaultProps} onNextSet={onNextSet} />);
    await userEvent.click(screen.getByTestId("cg-next-set-btn"));
    expect(onNextSet).toHaveBeenCalledOnce();
  });

  test("isSetJumpDisabled=true のときセット間ジャンプボタンが disabled になる", () => {
    render(<CgToolbar {...defaultProps} isSetJumpDisabled={true} />);
    expect(screen.getByTestId("cg-prev-set-btn")).toBeDisabled();
    expect(screen.getByTestId("cg-next-set-btn")).toBeDisabled();
  });

  test("isSetJumpDisabled=true のときクリックしても onPrevSet / onNextSet は呼ばれない", async () => {
    const onPrevSet = vi.fn();
    const onNextSet = vi.fn();
    render(
      <CgToolbar
        {...defaultProps}
        onPrevSet={onPrevSet}
        onNextSet={onNextSet}
        isSetJumpDisabled={true}
      />,
    );
    await userEvent.click(screen.getByTestId("cg-prev-set-btn"));
    await userEvent.click(screen.getByTestId("cg-next-set-btn"));
    expect(onPrevSet).not.toHaveBeenCalled();
    expect(onNextSet).not.toHaveBeenCalled();
  });
});
