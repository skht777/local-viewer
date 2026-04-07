import "@testing-library/jest-dom/vitest";

// ResizeObserver モック (jsdom 環境で未定義)
if (!globalThis.ResizeObserver) {
  globalThis.ResizeObserver = class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof globalThis.ResizeObserver;
}
