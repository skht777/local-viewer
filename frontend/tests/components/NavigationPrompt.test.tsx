import { render, screen, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { NavigationPrompt } from "../../src/components/NavigationPrompt";

describe("NavigationPrompt", () => {
  test("メッセージが表示される", () => {
    render(<NavigationPrompt message="次のディレクトリに移動しますか？" onConfirm={() => {}} onCancel={() => {}} />);
    expect(screen.getByText("次のディレクトリに移動しますか？")).toBeInTheDocument();
  });

  test("Y/Enter のヒントが表示される", () => {
    render(<NavigationPrompt message="test" onConfirm={() => {}} onCancel={() => {}} />);
    expect(screen.getByText(/Y.*Enter/)).toBeInTheDocument();
  });

  test("確認ボタンクリックで onConfirm が呼ばれる", async () => {
    const onConfirm = vi.fn();
    render(<NavigationPrompt message="test" onConfirm={onConfirm} onCancel={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: /はい/i }));
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  test("キャンセルボタンクリックで onCancel が呼ばれる", async () => {
    const onCancel = vi.fn();
    render(<NavigationPrompt message="test" onConfirm={() => {}} onCancel={onCancel} />);
    await userEvent.click(screen.getByRole("button", { name: /いいえ/i }));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  test("5秒後に自動で onCancel が呼ばれる", () => {
    vi.useFakeTimers();
    const onCancel = vi.fn();
    render(<NavigationPrompt message="test" onConfirm={() => {}} onCancel={onCancel} />);
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(onCancel).toHaveBeenCalledOnce();
    vi.useRealTimers();
  });
});
