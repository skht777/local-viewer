import { useViewerStore } from "../../src/stores/viewerStore";

describe("viewerStore", () => {
  beforeEach(() => {
    // ストアをリセット
    useViewerStore.setState({
      isSidebarOpen: true,
      expandedNodeIds: new Set(),
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
});
