// テスト用ラッパー
// - MemoryRouter (初期 URL 指定可)
// - QueryClientProvider (テスト用 QueryClient)

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import type { ReactElement } from "react";
import { MemoryRouter } from "react-router-dom";

interface RenderOptions {
  initialEntries?: string[];
}

export function renderWithProviders(ui: ReactElement, options?: RenderOptions) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={options?.initialEntries ?? ["/"]}>
        {ui}
      </MemoryRouter>
    </QueryClientProvider>,
  );
}
