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
  };

  test("フィット切替ボタンが表示される", () => {
    render(<CgToolbar {...defaultProps} />);
    expect(screen.getByRole("button", { name: /幅/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /高さ/i })).toBeInTheDocument();
  });

  test("見開き切替ボタンが表示される", () => {
    render(<CgToolbar {...defaultProps} />);
    expect(screen.getByRole("button", { name: /見開き/i })).toBeInTheDocument();
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
});
