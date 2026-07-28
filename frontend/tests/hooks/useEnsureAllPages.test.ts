// useEnsureAllPages の振る舞い検証
// - enabled かつ次ページが残っていて取得中でないとき fetchNextPage を呼ぶ
// - ビューワー表示中の無限スクロール代替（FileBrowser アンマウント時の打ち切り防止）

import { renderHook } from "@testing-library/react";
import { useEnsureAllPages } from "../../src/hooks/useEnsureAllPages";

interface Params {
  enabled?: boolean;
  hasNextPage?: boolean;
  isFetchingNextPage?: boolean;
}

function run({ enabled = true, hasNextPage = true, isFetchingNextPage = false }: Params = {}) {
  const fetchNextPage = vi.fn();
  const { rerender, unmount } = renderHook(
    (props: Required<Params>) => useEnsureAllPages({ ...props, fetchNextPage }),
    { initialProps: { enabled, hasNextPage, isFetchingNextPage } },
  );
  return { fetchNextPage, rerender, unmount };
}

describe("useEnsureAllPages", () => {
  test("enabled かつ hasNextPage のとき fetchNextPage が呼ばれる", () => {
    const { fetchNextPage } = run();
    expect(fetchNextPage).toHaveBeenCalledOnce();
  });

  test("hasNextPage が false なら呼ばれない", () => {
    const { fetchNextPage } = run({ hasNextPage: false });
    expect(fetchNextPage).not.toHaveBeenCalled();
  });

  test("enabled が false なら呼ばれない", () => {
    const { fetchNextPage } = run({ enabled: false });
    expect(fetchNextPage).not.toHaveBeenCalled();
  });

  test("取得中 (isFetchingNextPage) は呼ばれない", () => {
    const { fetchNextPage } = run({ isFetchingNextPage: true });
    expect(fetchNextPage).not.toHaveBeenCalled();
  });

  test("取得完了のたびに次ページを連鎖取得する", () => {
    const { fetchNextPage, rerender } = run();
    expect(fetchNextPage).toHaveBeenCalledTimes(1);
    // 取得中 → 完了(次ページあり) で再度呼ばれる
    rerender({ enabled: true, hasNextPage: true, isFetchingNextPage: true });
    rerender({ enabled: true, hasNextPage: true, isFetchingNextPage: false });
    expect(fetchNextPage).toHaveBeenCalledTimes(2);
    // 最終ページ到達後は呼ばれない
    rerender({ enabled: true, hasNextPage: false, isFetchingNextPage: false });
    expect(fetchNextPage).toHaveBeenCalledTimes(2);
  });
});
