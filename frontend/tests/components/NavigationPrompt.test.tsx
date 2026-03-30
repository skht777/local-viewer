import { render, screen, act, fireEvent } from "@testing-library/react";
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

  test("Y キー押下で onConfirm が呼ばれる", async () => {
    const onConfirm = vi.fn();
    render(<NavigationPrompt message="test" onConfirm={onConfirm} onCancel={() => {}} />);
    await userEvent.keyboard("y");
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  test("Enter キー押下で onConfirm が呼ばれる", async () => {
    const onConfirm = vi.fn();
    render(<NavigationPrompt message="test" onConfirm={onConfirm} onCancel={() => {}} />);
    await userEvent.keyboard("{Enter}");
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  test("N キー押下で onCancel が呼ばれる", async () => {
    const onCancel = vi.fn();
    render(<NavigationPrompt message="test" onConfirm={() => {}} onCancel={onCancel} />);
    await userEvent.keyboard("n");
    expect(onCancel).toHaveBeenCalledOnce();
  });

  test("extraConfirmKeys の X キーで onConfirm が呼ばれる", async () => {
    const onConfirm = vi.fn();
    render(
      <NavigationPrompt message="test" onConfirm={onConfirm} onCancel={() => {}} extraConfirmKeys={["x"]} />,
    );
    await userEvent.keyboard("x");
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  test("extraConfirmKeys の Z キーで onConfirm が呼ばれる", async () => {
    const onConfirm = vi.fn();
    render(
      <NavigationPrompt message="test" onConfirm={onConfirm} onCancel={() => {}} extraConfirmKeys={["z"]} />,
    );
    await userEvent.keyboard("z");
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  test("extraConfirmKeys 未指定時は X キーで onConfirm が呼ばれない", async () => {
    const onConfirm = vi.fn();
    render(<NavigationPrompt message="test" onConfirm={onConfirm} onCancel={() => {}} />);
    await userEvent.keyboard("x");
    expect(onConfirm).not.toHaveBeenCalled();
  });

  test("extraConfirmKeys 指定時にヒントテキストにキーが含まれる", () => {
    render(
      <NavigationPrompt message="test" onConfirm={() => {}} onCancel={() => {}} extraConfirmKeys={["x"]} />,
    );
    expect(screen.getByText(/X.*Y.*Enter/i)).toBeInTheDocument();
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

  test("ホバー中は5秒経過しても onCancel が呼ばれない", () => {
    vi.useFakeTimers();
    const onCancel = vi.fn();
    render(<NavigationPrompt message="test" onConfirm={() => {}} onCancel={onCancel} />);
    const prompt = screen.getByTestId("navigation-prompt");

    // 2秒後にホバー開始
    act(() => { vi.advanceTimersByTime(2000); });
    act(() => { fireEvent.mouseEnter(prompt); });

    // さらに4秒経過（合計6秒）しても呼ばれない
    act(() => { vi.advanceTimersByTime(4000); });
    expect(onCancel).not.toHaveBeenCalled();

    vi.useRealTimers();
  });

  test("ホバー解除後に残り時間で onCancel が呼ばれる", () => {
    vi.useFakeTimers();
    const onCancel = vi.fn();
    render(<NavigationPrompt message="test" onConfirm={() => {}} onCancel={onCancel} />);
    const prompt = screen.getByTestId("navigation-prompt");

    // 2秒後にホバー開始
    act(() => { vi.advanceTimersByTime(2000); });
    act(() => { fireEvent.mouseEnter(prompt); });

    // 1秒後にホバー解除（残り約3秒）
    act(() => { vi.advanceTimersByTime(1000); });
    act(() => { fireEvent.mouseLeave(prompt); });

    // 残り時間（約3秒）が経過する前は呼ばれない
    act(() => { vi.advanceTimersByTime(2000); });
    expect(onCancel).not.toHaveBeenCalled();

    // 残り時間（約3秒）が経過すると呼ばれる
    act(() => { vi.advanceTimersByTime(2000); });
    expect(onCancel).toHaveBeenCalledOnce();

    vi.useRealTimers();
  });
});
