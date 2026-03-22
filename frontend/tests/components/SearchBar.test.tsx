import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SearchBar } from "../../src/components/SearchBar";

describe("SearchBar", () => {
  test("入力欄が表示される", () => {
    render(<SearchBar />);
    expect(screen.getByPlaceholderText("検索...")).toBeInTheDocument();
  });

  test("テキスト入力ができる", async () => {
    render(<SearchBar />);
    const input = screen.getByPlaceholderText("検索...");
    await userEvent.type(input, "test query");
    expect(input).toHaveValue("test query");
  });

  test("Enterキーでコールバックが呼ばれる", async () => {
    const onSearch = vi.fn();
    render(<SearchBar onSearch={onSearch} />);
    const input = screen.getByPlaceholderText("検索...");
    await userEvent.type(input, "hello{Enter}");
    expect(onSearch).toHaveBeenCalledWith("hello");
  });
});
