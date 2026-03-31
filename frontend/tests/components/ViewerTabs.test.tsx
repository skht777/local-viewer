import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ViewerTabs } from "../../src/components/ViewerTabs";

describe("ViewerTabs", () => {
  test("3つのタブが表示される", () => {
    render(<ViewerTabs activeTab="filesets" onTabChange={() => {}} />);
    expect(screen.getByText("ファイルセット")).toBeInTheDocument();
    expect(screen.getByText("画像")).toBeInTheDocument();
    expect(screen.getByText("動画")).toBeInTheDocument();
  });

  test("タブクリックでonTabChangeが呼ばれる", async () => {
    const onTabChange = vi.fn();
    render(<ViewerTabs activeTab="filesets" onTabChange={onTabChange} />);
    await userEvent.click(screen.getByText("画像"));
    expect(onTabChange).toHaveBeenCalledWith("images");
  });

  test("アクティブタブにハイライトクラスが適用される", () => {
    render(<ViewerTabs activeTab="images" onTabChange={() => {}} />);
    const imagesTab = screen.getByText("画像");
    expect(imagesTab).toHaveClass("border-b-2");
    expect(imagesTab).toHaveClass("border-blue-500");
  });

  // --- ソートトグル ---

  test("ソートトグルが表示される", () => {
    const onSortChange = vi.fn();
    render(
      <ViewerTabs activeTab="filesets" onTabChange={() => {}} sort="name-asc" onSortChange={onSortChange} />,
    );
    expect(screen.getByTestId("sort-name")).toBeInTheDocument();
    expect(screen.getByTestId("sort-date")).toBeInTheDocument();
  });

  test("名前ボタンクリックでonSortChangeが呼ばれる", async () => {
    const onSortChange = vi.fn();
    render(
      <ViewerTabs activeTab="filesets" onTabChange={() => {}} sort="date-desc" onSortChange={onSortChange} />,
    );
    await userEvent.click(screen.getByTestId("sort-name"));
    // 別キーをクリック → デフォルト方向 (name-asc)
    expect(onSortChange).toHaveBeenCalledWith("name-asc");
  });

  test("アクティブキー再クリックで方向が反転する", async () => {
    const onSortChange = vi.fn();
    render(
      <ViewerTabs activeTab="filesets" onTabChange={() => {}} sort="name-asc" onSortChange={onSortChange} />,
    );
    await userEvent.click(screen.getByTestId("sort-name"));
    expect(onSortChange).toHaveBeenCalledWith("name-desc");
  });

  test("アクティブなソートに矢印が表示される", () => {
    const onSortChange = vi.fn();
    render(
      <ViewerTabs activeTab="filesets" onTabChange={() => {}} sort="date-desc" onSortChange={onSortChange} />,
    );
    // date-desc → 更新日ボタンに ↓ が表示される
    expect(screen.getByTestId("sort-date").textContent).toContain("↓");
  });
});
