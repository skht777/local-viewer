import { render, screen, act } from "@testing-library/react";
import { Toast } from "../../src/components/Toast";

describe("Toast", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  test("メッセージが表示される", () => {
    render(<Toast message="最後の画像です" onDismiss={() => {}} />);
    expect(screen.getByText("最後の画像です")).toBeInTheDocument();
  });

  test("data-testid が viewer-toast である", () => {
    render(<Toast message="test" onDismiss={() => {}} />);
    expect(screen.getByTestId("viewer-toast")).toBeInTheDocument();
  });

  test("2秒後に onDismiss が呼ばれる", () => {
    const onDismiss = vi.fn();
    render(<Toast message="test" onDismiss={onDismiss} />);
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(onDismiss).toHaveBeenCalledOnce();
  });

  test("カスタム duration で自動消去タイミングを変更できる", () => {
    const onDismiss = vi.fn();
    render(<Toast message="test" onDismiss={onDismiss} duration={3000} />);
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(onDismiss).not.toHaveBeenCalled();
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(onDismiss).toHaveBeenCalledOnce();
  });
});
