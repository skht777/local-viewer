import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { SearchBar } from "../../src/components/SearchBar";

// SearchBar は内部で useSearch + useNavigate を使うためプロバイダーが必要
function renderSearchBar() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>
        <SearchBar />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("SearchBar", () => {
  test("入力欄が表示される", () => {
    renderSearchBar();
    expect(screen.getByPlaceholderText("全体を検索...")).toBeInTheDocument();
  });

  test("テキスト入力ができる", async () => {
    renderSearchBar();
    const input = screen.getByPlaceholderText("全体を検索...");
    await userEvent.type(input, "test query");
    expect(input).toHaveValue("test query");
  });

  test("kindフィルタボタンが表示される", () => {
    renderSearchBar();
    expect(screen.getByTestId("kind-filter-all")).toBeInTheDocument();
    expect(screen.getByTestId("kind-filter-directory")).toBeInTheDocument();
    expect(screen.getByTestId("kind-filter-video")).toBeInTheDocument();
  });

  test("aria-labelが設定されている", () => {
    renderSearchBar();
    expect(screen.getByLabelText("検索")).toBeInTheDocument();
  });
});
