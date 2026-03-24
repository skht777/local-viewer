import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { CgToolbar } from "../../src/components/CgToolbar";

describe("CgToolbar", () => {
  const defaultProps = {
    fitMode: "width" as const,
    spreadMode: "single" as const,
    currentIndex: 2,
    totalCount: 10,
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
});
