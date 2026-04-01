// SliderTooltip ユニットテスト
// - ページ番号テキストの表示形式
// - visible 制御による表示/非表示
// - data-testid の存在

import { render, screen } from "@testing-library/react";
import { SliderTooltip } from "../../src/components/SliderTooltip";

describe("SliderTooltip", () => {
  test("ページ番号が 'N / M' 形式で表示される", () => {
    render(
      <SliderTooltip
        currentIndex={2}
        totalCount={10}
        position={50}
        orientation="horizontal"
        visible={true}
      />,
    );
    expect(screen.getByTestId("slider-tooltip")).toHaveTextContent("3 / 10");
  });

  test("visible=false のとき非表示クラスが適用される", () => {
    render(
      <SliderTooltip
        currentIndex={0}
        totalCount={5}
        position={0}
        orientation="horizontal"
        visible={false}
      />,
    );
    const tooltip = screen.getByTestId("slider-tooltip");
    expect(tooltip.className).toContain("opacity-0");
    expect(tooltip.className).toContain("pointer-events-none");
  });

  test("visible=true のとき表示クラスが適用される", () => {
    render(
      <SliderTooltip
        currentIndex={0}
        totalCount={5}
        position={0}
        orientation="horizontal"
        visible={true}
      />,
    );
    const tooltip = screen.getByTestId("slider-tooltip");
    expect(tooltip.className).toContain("opacity-100");
    expect(tooltip.className).not.toContain("pointer-events-none");
  });

  test("水平モードで left スタイルが設定される", () => {
    render(
      <SliderTooltip
        currentIndex={3}
        totalCount={10}
        position={120}
        orientation="horizontal"
        visible={true}
      />,
    );
    const tooltip = screen.getByTestId("slider-tooltip");
    expect(tooltip.style.left).toBe("120px");
  });

  test("縦モードで top スタイルが設定される", () => {
    render(
      <SliderTooltip
        currentIndex={3}
        totalCount={10}
        position={80}
        orientation="vertical"
        visible={true}
      />,
    );
    const tooltip = screen.getByTestId("slider-tooltip");
    expect(tooltip.style.top).toBe("80px");
  });
});
