// URLSearchParams mutation の共通パターン
// - prev を複製し mutator で変更した結果を返す pure 関数
// - react-router-dom の setSearchParams コールバック形と互換
//
// Finding 3 対応: useViewerParams 内で 8 箇所の `new URLSearchParams(prev)` +
// `set/delete` の繰り返しを一本化する

export type SearchParamsMutator = (next: URLSearchParams) => void;

/**
 * 現在の URLSearchParams を複製し、mutator による変更を適用した結果を返す。
 *
 * `prev` は破壊しない（新しい URLSearchParams を生成）。
 */
export function updateSearchParams(
  current: URLSearchParams,
  mutator: SearchParamsMutator,
): URLSearchParams {
  const next = new URLSearchParams(current);
  mutator(next);
  return next;
}
