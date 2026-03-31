// PageSlider ユニットテスト
// - ホバー時の表示維持（isHovering）
// - ドラッグ中の表示維持 + ドラッグ終了後の非表示
// - 基本的な表示/非表示制御

import { render, screen, fireEvent, act } from "@testing-library/react";
import { useRef } from "react";
import { PageSlider } from "../../src/components/PageSlider";

// 初回ヒントを無効化
beforeEach(() => {
  sessionStorage.setItem("slider-hint-shown", "1");
});
afterEach(() => {
  sessionStorage.clear();
});

// containerRef を提供するテスト用ラッパー
interface WrapperProps {
  totalCount?: number;
  currentIndex?: number;
  onGoTo?: (index: number) => void;
  onSliderActivity?: () => void;
}

function TestWrapper({
  totalCount = 5,
  currentIndex = 0,
  onGoTo = () => {},
  onSliderActivity,
}: WrapperProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  return (
    <div ref={containerRef} data-testid="container" style={{ height: 600 }}>
      <PageSlider
        currentIndex={currentIndex}
        totalCount={totalCount}
        onGoTo={onGoTo}
        containerRef={containerRef}
        onSliderActivity={onSliderActivity}
      />
    </div>
  );
}

describe("PageSlider", () => {
  test("totalCount が 1 以下の場合 null を返す", () => {
    render(<TestWrapper totalCount={1} />);
    expect(screen.queryByTestId("page-slider")).not.toBeInTheDocument();
  });

  test("初期状態で opacity-0 クラスが適用される", () => {
    render(<TestWrapper />);
    const slider = screen.getByTestId("page-slider");
    expect(slider.className).toContain("opacity-0");
    expect(slider.className).toContain("pointer-events-none");
  });

  test("pointerenter でスライダーが表示される", () => {
    render(<TestWrapper />);
    const slider = screen.getByTestId("page-slider");

    fireEvent.pointerEnter(slider);

    expect(slider.className).toContain("opacity-100");
    expect(slider.className).not.toContain("pointer-events-none");
  });

  test("pointerleave でスライダーが非表示になる", () => {
    render(<TestWrapper />);
    const slider = screen.getByTestId("page-slider");

    fireEvent.pointerEnter(slider);
    expect(slider.className).toContain("opacity-100");

    fireEvent.pointerLeave(slider);
    expect(slider.className).toContain("opacity-0");
  });

  test("ドラッグ中は pointerleave でも表示維持", () => {
    render(<TestWrapper />);
    const slider = screen.getByTestId("page-slider");
    const input = screen.getByRole("slider", { name: "ページスライダー" });

    // スライダーにホバー → ドラッグ開始 → ラッパーから離脱
    fireEvent.pointerEnter(slider);
    fireEvent.pointerDown(input);
    fireEvent.pointerLeave(slider);

    // ドラッグ中なので表示維持
    expect(slider.className).toContain("opacity-100");
  });

  test("ドラッグ終了後にポインタ離脱済みなら非表示", () => {
    render(<TestWrapper />);
    const slider = screen.getByTestId("page-slider");
    const input = screen.getByRole("slider", { name: "ページスライダー" });

    // ホバー → ドラッグ開始 → ラッパーから離脱 → ドラッグ終了
    fireEvent.pointerEnter(slider);
    fireEvent.pointerDown(input);
    fireEvent.pointerLeave(slider);

    // document レベルで pointerup を発火
    act(() => {
      document.dispatchEvent(new PointerEvent("pointerup"));
    });

    expect(slider.className).toContain("opacity-0");
  });

  test("スライダー操作で onGoTo が呼ばれる", () => {
    const onGoTo = vi.fn();
    render(<TestWrapper onGoTo={onGoTo} />);
    const input = screen.getByRole("slider", { name: "ページスライダー" });

    fireEvent.change(input, { target: { value: "3" } });

    expect(onGoTo).toHaveBeenCalledWith(3);
  });

  test("focus でスライダーが表示される", () => {
    render(<TestWrapper />);
    const slider = screen.getByTestId("page-slider");
    const input = screen.getByRole("slider", { name: "ページスライダー" });

    fireEvent.focus(input);

    expect(slider.className).toContain("opacity-100");
  });
});
