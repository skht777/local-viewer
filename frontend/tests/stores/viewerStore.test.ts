import { useViewerStore } from "../../src/stores/viewerStore";

describe("viewerStore", () => {
  beforeEach(() => {
    // localStorage をクリア（persist middleware のケース間汚染防止）
    localStorage.clear();
    // ストアをリセット
    useViewerStore.setState({
      isSidebarOpen: true,
      expandedNodeIds: new Set(),
      fitMode: "height",
      spreadMode: "single",
      zoomLevel: 100,
      scrollSpeed: 1.0,
      viewerOrigin: null,
      viewerTransitionId: 0,
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

  test("初期状態で fitMode が height", () => {
    const state = useViewerStore.getState();
    expect(state.fitMode).toBe("height");
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

  test("expandedNodeIds が localStorage に配列として保存される", () => {
    useViewerStore.getState().toggleExpanded("node1");
    useViewerStore.getState().toggleExpanded("node2");
    const stored = JSON.parse(localStorage.getItem("viewer-store") ?? "{}");
    const ids = stored.state.expandedNodeIds as string[];
    expect(ids).toContain("node1");
    expect(ids).toContain("node2");
  });

  test("localStorage から expandedNodeIds が Set として復元される", () => {
    // localStorage に配列形式で保存
    localStorage.setItem(
      "viewer-store",
      JSON.stringify({
        state: {
          fitMode: "height",
          spreadMode: "single",
          zoomLevel: 100,
          scrollSpeed: 1.0,
          expandedNodeIds: ["a", "b"],
        },
        version: 0,
      }),
    );
    // ストアを再構築（persist rehydrate）
    useViewerStore.persist.rehydrate();
    const state = useViewerStore.getState();
    expect(state.expandedNodeIds).toBeInstanceOf(Set);
    expect(state.expandedNodeIds.has("a")).toBe(true);
    expect(state.expandedNodeIds.has("b")).toBe(true);
  });

  // --- Phase 3: zoomLevel / scrollSpeed ---

  test("初期状態で zoomLevel が 100", () => {
    expect(useViewerStore.getState().zoomLevel).toBe(100);
  });

  test("zoomIn で 25 ずつ増加する", () => {
    useViewerStore.getState().zoomIn();
    expect(useViewerStore.getState().zoomLevel).toBe(125);
  });

  test("zoomIn で上限 300 を超えない", () => {
    useViewerStore.setState({ zoomLevel: 300 });
    useViewerStore.getState().zoomIn();
    expect(useViewerStore.getState().zoomLevel).toBe(300);
  });

  test("zoomOut で 25 ずつ減少する", () => {
    useViewerStore.getState().zoomOut();
    expect(useViewerStore.getState().zoomLevel).toBe(75);
  });

  test("zoomOut で下限 25 を下回らない", () => {
    useViewerStore.setState({ zoomLevel: 25 });
    useViewerStore.getState().zoomOut();
    expect(useViewerStore.getState().zoomLevel).toBe(25);
  });

  test("setZoomLevel で任意の値を設定できる", () => {
    useViewerStore.getState().setZoomLevel(200);
    expect(useViewerStore.getState().zoomLevel).toBe(200);
  });

  test("setZoomLevel で範囲外の値は clamp される", () => {
    useViewerStore.getState().setZoomLevel(500);
    expect(useViewerStore.getState().zoomLevel).toBe(300);
    useViewerStore.getState().setZoomLevel(0);
    expect(useViewerStore.getState().zoomLevel).toBe(25);
  });

  test("zoomLevel が persist で永続化される", () => {
    useViewerStore.getState().setZoomLevel(150);
    const stored = JSON.parse(localStorage.getItem("viewer-store") ?? "{}");
    expect(stored.state.zoomLevel).toBe(150);
  });

  test("初期状態で scrollSpeed が 1.0", () => {
    expect(useViewerStore.getState().scrollSpeed).toBe(1.0);
  });

  test("setScrollSpeed で範囲内の値を設定できる", () => {
    useViewerStore.getState().setScrollSpeed(2.0);
    expect(useViewerStore.getState().scrollSpeed).toBe(2.0);
  });

  test("setScrollSpeed で範囲外の値は clamp される", () => {
    useViewerStore.getState().setScrollSpeed(5.0);
    expect(useViewerStore.getState().scrollSpeed).toBe(3.0);
    useViewerStore.getState().setScrollSpeed(0.1);
    expect(useViewerStore.getState().scrollSpeed).toBe(0.5);
  });

  // --- viewerOrigin / viewerTransitionId (persist 除外) ---

  test("初期状態で viewerOrigin が null", () => {
    expect(useViewerStore.getState().viewerOrigin).toBeNull();
  });

  test("setViewerOrigin で nodeId と search が保存される", () => {
    useViewerStore
      .getState()
      .setViewerOrigin({ nodeId: "dir1", search: "?tab=images&sort=date-desc" });
    const origin = useViewerStore.getState().viewerOrigin;
    expect(origin).toEqual({ nodeId: "dir1", search: "?tab=images&sort=date-desc" });
  });

  test("setViewerOrigin(null) で起点をクリアできる", () => {
    useViewerStore.getState().setViewerOrigin({ nodeId: "dir1", search: "" });
    useViewerStore.getState().setViewerOrigin(null);
    expect(useViewerStore.getState().viewerOrigin).toBeNull();
  });

  test("viewerOrigin は localStorage に永続化されない", () => {
    useViewerStore.getState().setViewerOrigin({ nodeId: "dir1", search: "?tab=images" });
    const stored = JSON.parse(localStorage.getItem("viewer-store") ?? "{}");
    expect(stored.state?.viewerOrigin).toBeUndefined();
  });

  test("初期状態で viewerTransitionId が 0", () => {
    expect(useViewerStore.getState().viewerTransitionId).toBe(0);
  });

  test("startViewerTransition で ID がインクリメントされ返値と一致する", () => {
    const id = useViewerStore.getState().startViewerTransition();
    expect(id).toBeGreaterThan(0);
    expect(useViewerStore.getState().viewerTransitionId).toBe(id);
  });

  test("endViewerTransition で ID 一致時のみ 0 にリセットされる", () => {
    const id = useViewerStore.getState().startViewerTransition();
    useViewerStore.getState().endViewerTransition(id);
    expect(useViewerStore.getState().viewerTransitionId).toBe(0);
  });

  test("endViewerTransition で stale な ID は無視される", () => {
    const id = useViewerStore.getState().startViewerTransition();
    const newerId = useViewerStore.getState().startViewerTransition();
    // 古い id を渡しても現在のトランジションはクリアされない
    useViewerStore.getState().endViewerTransition(id);
    expect(useViewerStore.getState().viewerTransitionId).toBe(newerId);
  });

  test("viewerTransitionId は localStorage に永続化されない", () => {
    useViewerStore.getState().startViewerTransition();
    const stored = JSON.parse(localStorage.getItem("viewer-store") ?? "{}");
    expect(stored.state?.viewerTransitionId).toBeUndefined();
  });
});
