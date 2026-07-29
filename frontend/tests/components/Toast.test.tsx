import { render, screen, act, fireEvent } from "@testing-library/react";
import { Toast } from "../../src/components/Toast";
import { useToast } from "../../src/hooks/useToast";

describe("Toast", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  test("メッセージが表示される", () => {
    render(<Toast message="最後の画像です" />);
    expect(screen.getByText("最後の画像です")).toBeInTheDocument();
  });

  test("data-testid が viewer-toast である", () => {
    render(<Toast message="test" />);
    expect(screen.getByTestId("viewer-toast")).toBeInTheDocument();
  });

  // 表示時間の管理は useToast に一本化されている。Toast は自前タイマーを持たない
  test("自前タイマーを持たず時間が経過しても表示され続ける", () => {
    render(<Toast message="test" />);
    act(() => {
      vi.advanceTimersByTime(10_000);
    });
    expect(screen.getByTestId("viewer-toast")).toBeInTheDocument();
    expect(vi.getTimerCount()).toBe(0);
  });

  // V13 追補: useToast と組み合わせた実利用形態での表示時間を固定する。
  // Toast が独自タイマーを持つと、同一メッセージ・同一 duration の再表示では
  // effect の依存配列が変化せず旧タイマーが生き残り、早期に消えてしまう。
  describe("useToast と組み合わせた表示時間", () => {
    // viewer 各種と同じ配線（useToast がタイマーを持ち、Toast は表示のみ）
    function ToastHarness() {
      const { toastMessage, showToast } = useToast();
      return (
        <>
          <button type="button" data-testid="show" onClick={() => showToast("セット境界です")}>
            show
          </button>
          {toastMessage && <Toast message={toastMessage} />}
        </>
      );
    }

    test("同一メッセージを再表示すると最後の表示から2秒間は消えない", () => {
      render(<ToastHarness />);
      act(() => {
        fireEvent.click(screen.getByTestId("show"));
      });
      expect(screen.getByTestId("viewer-toast")).toBeInTheDocument();

      act(() => {
        vi.advanceTimersByTime(1500);
      });
      // 同じ文言で再表示（message / duration が不変なので effect は再実行されない）
      act(() => {
        fireEvent.click(screen.getByTestId("show"));
      });

      act(() => {
        vi.advanceTimersByTime(1500);
      });
      // 旧タイマーが生き残っていると初回表示から 2000ms 経過時点で消えてしまう
      expect(screen.getByTestId("viewer-toast")).toBeInTheDocument();

      act(() => {
        vi.advanceTimersByTime(500);
      });
      expect(screen.queryByTestId("viewer-toast")).not.toBeInTheDocument();
    });

    test("再表示しなければ duration 経過でトーストが消える", () => {
      render(<ToastHarness />);
      act(() => {
        fireEvent.click(screen.getByTestId("show"));
      });
      act(() => {
        vi.advanceTimersByTime(1999);
      });
      expect(screen.getByTestId("viewer-toast")).toBeInTheDocument();
      act(() => {
        vi.advanceTimersByTime(1);
      });
      expect(screen.queryByTestId("viewer-toast")).not.toBeInTheDocument();
    });
  });
});
