import { renderHook, act } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { useViewerParams } from "../../src/hooks/useViewerParams";
import type { ReactNode } from "react";

function createWrapper(initialEntries: string[] = ["/"]) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <MemoryRouter initialEntries={initialEntries}>{children}</MemoryRouter>
    );
  };
}

describe("useViewerParams", () => {
  test("デフォルト値が返される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(),
    });
    expect(result.current.params.tab).toBe("images");
    expect(result.current.params.index).toBe(0);
    expect(result.current.params.mode).toBe("cg");
  });

  test("URLのsearchParamsが反映される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?tab=videos&index=5&mode=manga"]),
    });
    expect(result.current.params.tab).toBe("videos");
    expect(result.current.params.index).toBe(5);
    expect(result.current.params.mode).toBe("manga");
  });

  test("setTabでURLが更新される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(),
    });
    act(() => {
      result.current.setTab("videos");
    });
    expect(result.current.params.tab).toBe("videos");
  });
});
