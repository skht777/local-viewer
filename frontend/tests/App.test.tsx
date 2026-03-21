import { render, screen } from "@testing-library/react";
import App from "../src/App";

describe("App", () => {
  test("タイトルが表示される", () => {
    render(<App />);
    expect(screen.getByText("Local Content Viewer")).toBeInTheDocument();
  });

  test("ダークテーマの背景クラスが適用される", () => {
    const { container } = render(<App />);
    const root = container.firstElementChild;
    expect(root).toHaveClass("bg-gray-900");
  });
});
