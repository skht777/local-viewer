import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { PreviewGrid } from "../../src/components/PreviewGrid";

describe("PreviewGrid", () => {
  test("1枚のプレビュー画像が表示される", () => {
    const { container } = render(<PreviewGrid previewNodeIds={["node1"]} />);
    const imgs = container.querySelectorAll("img");
    expect(imgs).toHaveLength(1);
    expect(imgs[0]).toHaveAttribute("src", "/api/thumbnail/node1");
  });

  test("2枚のプレビュー画像がグリッド表示される", () => {
    const { container } = render(
      <PreviewGrid previewNodeIds={["node1", "node2"]} />,
    );
    const imgs = container.querySelectorAll("img");
    expect(imgs).toHaveLength(2);
    expect(imgs[0]).toHaveAttribute("src", "/api/thumbnail/node1");
    expect(imgs[1]).toHaveAttribute("src", "/api/thumbnail/node2");
  });

  test("3枚のプレビュー画像がグリッド表示される", () => {
    const { container } = render(
      <PreviewGrid previewNodeIds={["node1", "node2", "node3"]} />,
    );
    const imgs = container.querySelectorAll("img");
    expect(imgs).toHaveLength(3);
  });

  test("画像読み込みエラー時にフォールバックする", () => {
    const onAllError = vi.fn();
    const { container } = render(
      <PreviewGrid previewNodeIds={["node1"]} onAllError={onAllError} />,
    );
    const img = container.querySelector("img")!;
    fireEvent.error(img);
    expect(onAllError).toHaveBeenCalledOnce();
  });

  test("一部の画像エラーでは onAllError が呼ばれない", () => {
    const onAllError = vi.fn();
    const { container } = render(
      <PreviewGrid
        previewNodeIds={["node1", "node2"]}
        onAllError={onAllError}
      />,
    );
    const imgs = container.querySelectorAll("img");
    // 1枚だけエラー
    fireEvent.error(imgs[0]);
    expect(onAllError).not.toHaveBeenCalled();
  });
});
