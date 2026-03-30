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
});
