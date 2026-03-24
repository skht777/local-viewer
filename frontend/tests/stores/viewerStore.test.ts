import { useViewerStore } from "../../src/stores/viewerStore";

describe("viewerStore", () => {
  beforeEach(() => {
    // localStorage をクリア（persist middleware のケース間汚染防止）
    localStorage.clear();
    // ストアをリセット
    useViewerStore.setState({
      isSidebarOpen: true,
      expandedNodeIds: new Set(),
      fitMode: "width",
      spreadMode: "single",
    });
  });

  test("初期状態でサイドバーが開いている", () => {
    const state = useViewerStore.getState();
    expect(state.isSidebarOpen).toBe(true);
  });

  test("toggleSidebarでサイドバーが閉じる", () => {
    useViewerStore.getState().toggleSidebar();
    expect(useViewerStore.getState().isSidebarOpen).toBe(false);
  });

  test("toggleSidebar2回で元に戻る", () => {
    useViewerStore.getState().toggleSidebar();
    useViewerStore.getState().toggleSidebar();
    expect(useViewerStore.getState().isSidebarOpen).toBe(true);
  });

  test("toggleExpandedでノードが展開される", () => {
    useViewerStore.getState().toggleExpanded("node1");
    expect(useViewerStore.getState().expandedNodeIds.has("node1")).toBe(true);
  });

  test("再度toggleExpandedでノードが折りたたまれる", () => {
    useViewerStore.getState().toggleExpanded("node1");
    useViewerStore.getState().toggleExpanded("node1");
    expect(useViewerStore.getState().expandedNodeIds.has("node1")).toBe(false);
  });

  // --- Phase 2: fitMode / spreadMode ---

  test("初期状態で fitMode が width", () => {
    const state = useViewerStore.getState();
    expect(state.fitMode).toBe("width");
  });

  test("setFitMode で fitMode が変更される", () => {
    useViewerStore.getState().setFitMode("height");
    expect(useViewerStore.getState().fitMode).toBe("height");
  });

  test("setFitMode で original に変更できる", () => {
    useViewerStore.getState().setFitMode("original");
    expect(useViewerStore.getState().fitMode).toBe("original");
  });

  test("初期状態で spreadMode が single", () => {
    const state = useViewerStore.getState();
    expect(state.spreadMode).toBe("single");
  });

  test("cycleSpreadMode で single → spread → spread-offset → single とサイクルする", () => {
    useViewerStore.getState().cycleSpreadMode();
    expect(useViewerStore.getState().spreadMode).toBe("spread");

    useViewerStore.getState().cycleSpreadMode();
    expect(useViewerStore.getState().spreadMode).toBe("spread-offset");

    useViewerStore.getState().cycleSpreadMode();
    expect(useViewerStore.getState().spreadMode).toBe("single");
  });

  // --- Phase 3: persist middleware ---

  test("persist middleware で fitMode が localStorage に保存される", () => {
    useViewerStore.getState().setFitMode("height");
    const stored = JSON.parse(localStorage.getItem("viewer-store") ?? "{}");
    expect(stored.state.fitMode).toBe("height");
  });

  test("persist middleware で spreadMode が localStorage に保存される", () => {
    useViewerStore.getState().cycleSpreadMode();
    const stored = JSON.parse(localStorage.getItem("viewer-store") ?? "{}");
    expect(stored.state.spreadMode).toBe("spread");
  });

  test("永続化対象外の isSidebarOpen は localStorage に含まれない", () => {
    useViewerStore.getState().toggleSidebar();
    const stored = JSON.parse(localStorage.getItem("viewer-store") ?? "{}");
    expect(stored.state.isSidebarOpen).toBeUndefined();
  });

  test("永続化対象外の expandedNodeIds は localStorage に含まれない", () => {
    useViewerStore.getState().toggleExpanded("node1");
    const stored = JSON.parse(localStorage.getItem("viewer-store") ?? "{}");
    expect(stored.state.expandedNodeIds).toBeUndefined();
  });
});
