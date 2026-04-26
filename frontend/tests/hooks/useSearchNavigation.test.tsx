// useSearchNavigation の振る舞い検証
// - handleSelect: kind 別の遷移分岐
//   - directory/archive: push 遷移
//   - PDF: scope ありで viewerOrigin 設定 + push、scope 無しは push のみ
//   - image/video: scope ありで viewerOrigin 設定 + replace、scope 無しは push
// - navigateToSearchPage: q.trim() ≥ 2 文字で /search?q=... に遷移、それ未満なら何もしない

import { renderHook } from "@testing-library/react";
import type { Location } from "react-router-dom";
import { useSearchNavigation } from "../../src/hooks/useSearchNavigation";
import type { SearchResult } from "../../src/types/api";

function makeLocation(search = ""): Location {
  // Location 型の internal フィールドは undefined キャストで埋める
  return {
    pathname: "/browse/x",
    search,
    hash: "",
    state: null,
    key: "default",
  } as unknown as Location;
}

function makeResult(
  overrides: Partial<SearchResult> & { kind: SearchResult["kind"] },
): SearchResult {
  return {
    node_id: "n",
    parent_node_id: "parent",
    name: "n",
    relative_path: "n",
    size_bytes: null,
    ...overrides,
  };
}

interface SetupOpts {
  scope?: string;
  effectiveScope?: string;
  query?: string;
  kind?: string | null;
  search?: string;
}

function setup(opts: SetupOpts = {}) {
  const navigate = vi.fn();
  const setViewerOrigin = vi.fn();
  const setQuery = vi.fn();
  const setIsOpen = vi.fn();
  const { result } = renderHook(() =>
    useSearchNavigation({
      scope: opts.scope,
      effectiveScope: opts.effectiveScope,
      query: opts.query ?? "",
      kind: opts.kind ?? null,
      location: makeLocation(opts.search),
      navigate,
      setViewerOrigin,
      setQuery,
      setIsOpen,
    }),
  );
  return { result, navigate, setViewerOrigin, setQuery, setIsOpen };
}

describe("useSearchNavigation - handleSelect", () => {
  test("directory は /browse/{id} に push 遷移し setIsOpen(false) + setQuery('') を呼ぶ", () => {
    const { result, navigate, setIsOpen, setQuery, setViewerOrigin } = setup();
    result.current.handleSelect(makeResult({ kind: "directory", node_id: "d1" }));
    expect(navigate).toHaveBeenCalledWith("/browse/d1");
    expect(setIsOpen).toHaveBeenCalledWith(false);
    expect(setQuery).toHaveBeenCalledWith("");
    expect(setViewerOrigin).not.toHaveBeenCalled();
  });

  test("archive も /browse/{id} に push 遷移する", () => {
    const { result, navigate } = setup();
    result.current.handleSelect(makeResult({ kind: "archive", node_id: "a1" }));
    expect(navigate).toHaveBeenCalledWith("/browse/a1");
  });

  test("parent_node_id が null のときは何もしない (image/pdf/video)", () => {
    const { result, navigate } = setup();
    result.current.handleSelect(makeResult({ kind: "image", node_id: "i1", parent_node_id: null }));
    expect(navigate).not.toHaveBeenCalled();
  });

  test("PDF + scope あり: viewerOrigin 設定 + push 遷移", () => {
    const { result, navigate, setViewerOrigin } = setup({ scope: "scope-1", search: "?q=foo" });
    result.current.handleSelect(
      makeResult({ kind: "pdf", node_id: "p1", parent_node_id: "parent-1" }),
    );
    expect(setViewerOrigin).toHaveBeenCalledWith({ pathname: "/browse/scope-1", search: "?q=foo" });
    expect(navigate).toHaveBeenCalledWith(expect.stringMatching(/^\/browse\/parent-1\?pdf=p1/));
    // push 遷移 = options 未指定
    const lastCall = navigate.mock.calls.at(-1)!;
    expect(lastCall.length).toBe(1);
  });

  test("PDF + scope なし: viewerOrigin 設定なし + push 遷移", () => {
    const { result, navigate, setViewerOrigin } = setup();
    result.current.handleSelect(
      makeResult({ kind: "pdf", node_id: "p1", parent_node_id: "parent-1" }),
    );
    expect(setViewerOrigin).not.toHaveBeenCalled();
    expect(navigate).toHaveBeenCalledWith(expect.stringContaining("pdf=p1"));
  });

  test("image + scope あり: viewerOrigin 設定 + replace 遷移", () => {
    const { result, navigate, setViewerOrigin } = setup({ scope: "scope-1" });
    result.current.handleSelect(
      makeResult({ kind: "image", node_id: "i1", parent_node_id: "parent-1" }),
    );
    expect(setViewerOrigin).toHaveBeenCalled();
    expect(navigate).toHaveBeenCalledWith(
      expect.stringMatching(/^\/browse\/parent-1\?.*tab=images.*select=i1/),
      { replace: true },
    );
  });

  test("video + scope なし: viewerOrigin 設定なし + push 遷移", () => {
    const { result, navigate, setViewerOrigin } = setup();
    result.current.handleSelect(
      makeResult({ kind: "video", node_id: "v1", parent_node_id: "parent-1" }),
    );
    expect(setViewerOrigin).not.toHaveBeenCalled();
    expect(navigate).toHaveBeenCalledWith(
      expect.stringMatching(/^\/browse\/parent-1\?.*tab=videos.*select=v1/),
    );
  });

  test("URL から mode/sort を継承する", () => {
    const { result, navigate } = setup({ search: "?mode=manga&sort=date-desc" });
    result.current.handleSelect(
      makeResult({ kind: "image", node_id: "i1", parent_node_id: "parent-1" }),
    );
    const url = navigate.mock.calls.at(-1)![0] as string;
    expect(url).toContain("mode=manga");
    expect(url).toContain("sort=date-desc");
  });
});

describe("useSearchNavigation - navigateToSearchPage", () => {
  test("query が 2 文字未満なら何もしない", () => {
    const { result, navigate, setIsOpen } = setup({ query: "a" });
    result.current.navigateToSearchPage();
    expect(navigate).not.toHaveBeenCalled();
    expect(setIsOpen).not.toHaveBeenCalled();
  });

  test("query が 2 文字以上で /search?q=... に遷移する", () => {
    const { result, navigate, setIsOpen } = setup({ query: "abc" });
    result.current.navigateToSearchPage();
    expect(setIsOpen).toHaveBeenCalledWith(false);
    expect(navigate).toHaveBeenCalledWith("/search?q=abc");
  });

  test("effectiveScope と kind が URL に乗る", () => {
    const { result, navigate } = setup({
      query: "  hello  ",
      effectiveScope: "scope-1",
      kind: "image",
    });
    result.current.navigateToSearchPage();
    const url = navigate.mock.calls[0]![0] as string;
    expect(url).toContain("q=hello");
    expect(url).toContain("scope=scope-1");
    expect(url).toContain("kind=image");
  });

  test("query が前後空白でも trim 後 2 文字未満なら何もしない", () => {
    const { result, navigate } = setup({ query: "  a  " });
    result.current.navigateToSearchPage();
    expect(navigate).not.toHaveBeenCalled();
  });
});
