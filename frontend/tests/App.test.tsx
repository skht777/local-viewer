import { screen } from "@testing-library/react";
import App from "../src/App";
import { renderWithProviders } from "./helpers/renderWithProviders";

describe("App", () => {
  test("ダークテーマの背景クラスが適用される", () => {
    const { container } = renderWithProviders(<App />);
    const root = container.querySelector(".bg-gray-900");
    expect(root).toBeInTheDocument();
  });

  test("ルート / で TopPage の見出しが表示される", () => {
    renderWithProviders(<App />);
    expect(screen.getByText("Local Content Viewer")).toBeInTheDocument();
  });
});
