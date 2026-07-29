// ページ跨ぎで重複した browse エントリを取り除く
// - バックエンドは cursor が解決できない場合に先頭ページから返すため、
//   infinite query の pages を単純結合すると同一 node_id が複数回現れうる
// - 初出優先: 最初に現れたエントリを残し、以降の同 node_id は捨てる
//   (表示順の安定と React の key 重複警告の抑止が目的)

interface HasNodeId {
  node_id: string;
}

export function dedupeByNodeId<T extends HasNodeId>(entries: T[]): T[] {
  const seen = new Set<string>();
  const result: T[] = [];
  for (const entry of entries) {
    if (!seen.has(entry.node_id)) {
      seen.add(entry.node_id);
      result.push(entry);
    }
  }
  return result;
}
