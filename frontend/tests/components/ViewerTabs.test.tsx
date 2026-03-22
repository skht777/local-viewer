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
});
