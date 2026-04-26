// useFileBrowserInfiniteScroll の振る舞い検証
// - hasMore=false / onLoadMore 未指定なら observer を設置しない
// - sentinel が intersect したとき onLoadMore を呼ぶ
// - isLoadingMore / isError 中は onLoadMore を抑止
// - cleanup で observer.disconnect される

import { render } from "@testing-library/react";
import { useFileBrowserInfiniteScroll } from "../../src/hooks/useFileBrowserInfiniteScroll";

interface ObserverCallback {
  cb: (entries: { isIntersecting: boolean }[]) => void;
  options: IntersectionObserverInit | undefined;
  observed: Element[];
  disconnected: boolean;
}

let observers: ObserverCallback[] = [];

class MockIntersectionObserver {
  cb: ObserverCallback["cb"];
  options: IntersectionObserverInit | undefined;
  observed: Element[] = [];
  disconnected = false;
  constructor(cb: ObserverCallback["cb"], options?: IntersectionObserverInit) {
    this.cb = cb;
    this.options = options;
    observers.push(this);
  }
  observe(el: Element) {
    this.observed.push(el);
  }
  disconnect() {
    this.disconnected = true;
  }
  unobserve() {}
  takeRecords() {
    return [];
  }
}

beforeEach(() => {
  observers = [];
  globalThis.IntersectionObserver =
    MockIntersectionObserver as unknown as typeof IntersectionObserver;
});

interface TestComponentProps {
  hasMore?: boolean;
  isLoadingMore?: boolean;
  isError?: boolean;
  onLoadMore?: () => void;
  rootMargin?: string;
}

function TestComponent(props: TestComponentProps) {
  const { sentinelRef } = useFileBrowserInfiniteScroll(props);
  return <div ref={sentinelRef} data-testid="sentinel" />;
}

describe("useFileBrowserInfiniteScroll", () => {
  test("hasMore=false のとき IntersectionObserver は作られない", () => {
    render(<TestComponent hasMore={false} onLoadMore={vi.fn()} />);
    expect(observers.length).toBe(0);
  });

  test("onLoadMore 未指定のとき IntersectionObserver は作られない", () => {
    render(<TestComponent hasMore={true} />);
    expect(observers.length).toBe(0);
  });

  test("sentinel intersect で onLoadMore が呼ばれる", () => {
    const onLoadMore = vi.fn();
    render(<TestComponent hasMore={true} onLoadMore={onLoadMore} />);
    expect(observers.length).toBe(1);
    observers[0]!.cb([{ isIntersecting: true }]);
    expect(onLoadMore).toHaveBeenCalledOnce();
  });

  test("isLoadingMore=true のとき intersect しても onLoadMore は呼ばれない", () => {
    const onLoadMore = vi.fn();
    render(<TestComponent hasMore={true} onLoadMore={onLoadMore} isLoadingMore={true} />);
    observers[0]!.cb([{ isIntersecting: true }]);
    expect(onLoadMore).not.toHaveBeenCalled();
  });

  test("isError=true のとき intersect しても onLoadMore は呼ばれない", () => {
    const onLoadMore = vi.fn();
    render(<TestComponent hasMore={true} onLoadMore={onLoadMore} isError={true} />);
    observers[0]!.cb([{ isIntersecting: true }]);
    expect(onLoadMore).not.toHaveBeenCalled();
  });

  test("intersect が isIntersecting=false のとき onLoadMore は呼ばれない", () => {
    const onLoadMore = vi.fn();
    render(<TestComponent hasMore={true} onLoadMore={onLoadMore} />);
    observers[0]!.cb([{ isIntersecting: false }]);
    expect(onLoadMore).not.toHaveBeenCalled();
  });

  test("rootMargin の default は '200px'", () => {
    render(<TestComponent hasMore={true} onLoadMore={vi.fn()} />);
    expect(observers[0]?.options?.rootMargin).toBe("200px");
  });

  test("rootMargin を上書きできる", () => {
    render(<TestComponent hasMore={true} onLoadMore={vi.fn()} rootMargin="50px" />);
    expect(observers[0]?.options?.rootMargin).toBe("50px");
  });

  test("unmount で disconnect が呼ばれる", () => {
    const { unmount } = render(<TestComponent hasMore={true} onLoadMore={vi.fn()} />);
    expect(observers[0]!.disconnected).toBe(false);
    unmount();
    expect(observers[0]!.disconnected).toBe(true);
  });
});
