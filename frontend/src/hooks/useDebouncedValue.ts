// 値のデバウンスフック
// - `value` が変わっても `delayMs` ミリ秒静止するまで更新を遅延する
// - `equalityFn` を渡すと参照比較ではなく構造比較で「同一」と判定できる
//   （配列の新規生成による不要な再計算を回避）
//
// Finding 4 対応: useBatchThumbnails の react-hooks/exhaustive-deps 抑制を
// 解消するための汎用 hook。

import { useEffect, useRef, useState } from "react";

/**
 * 入力値のデバウンス結果を返す。
 *
 * @param value 監視対象の値
 * @param delayMs 静止待ち時間（ミリ秒）
 * @param equalityFn 2 値が等価かの判定（省略時は参照比較）
 */
export function useDebouncedValue<T>(
  value: T,
  delayMs: number,
  equalityFn?: (a: T, b: T) => boolean,
): T {
  const [debounced, setDebounced] = useState(value);
  const lastEmittedRef = useRef<T>(value);

  useEffect(() => {
    // 直前に emit した値と一致するなら更新・タイマー開始を抑止
    const isEqual = equalityFn
      ? equalityFn(lastEmittedRef.current, value)
      : Object.is(lastEmittedRef.current, value);
    if (isEqual) {
      return;
    }

    const timer = setTimeout(() => {
      lastEmittedRef.current = value;
      setDebounced(value);
    }, delayMs);
    return () => clearTimeout(timer);
  }, [value, delayMs, equalityFn]);

  return debounced;
}

/**
 * 2 つの文字列配列が同順同要素かを判定する。
 * `useDebouncedValue(nodeIds, 50, areNodeIdsEqual)` で配列新規生成による
 * 再デバウンスを回避できる。
 */
export function areNodeIdsEqual(a: string[], b: string[]): boolean {
  if (a === b) {
    return true;
  }
  if (a.length !== b.length) {
    return false;
  }
  for (let i = 0; i < a.length; i += 1) {
    if (a[i] !== b[i]) {
      return false;
    }
  }
  return true;
}
