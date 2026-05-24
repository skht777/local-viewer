// タッチデバイス判定 (pointer: coarse)
// - SSR-safe: window 未定義時は false
// - useSyncExternalStore で MQ 変更を購読し、外付けキーボード/ドック接続等の動的変化に追従

import { useSyncExternalStore } from "react";

const MEDIA_QUERY = "(pointer: coarse)";

function getSnapshot(): boolean {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return false;
  }
  return window.matchMedia(MEDIA_QUERY).matches;
}

function getServerSnapshot(): boolean {
  return false;
}

function subscribe(notify: () => void): () => void {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return () => {};
  }
  const mql = window.matchMedia(MEDIA_QUERY);
  mql.addEventListener("change", notify);
  return () => mql.removeEventListener("change", notify);
}

export function useIsTouchDevice(): boolean {
  return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
}
