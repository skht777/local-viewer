// マウントポイント1つを表示するカード
// - 名前と子要素数を表示
// - クリックで onSelect コールバック

import type { BrowseEntry } from "../types/api";

interface MountPointCardProps {
  entry: BrowseEntry;
  onSelect: (nodeId: string) => void;
  index?: number;
}

export function MountPointCard({ entry, onSelect, index }: MountPointCardProps) {
  return (
    <button
      type="button"
      data-testid={`mount-${entry.node_id}`}
      onClick={() => onSelect(entry.node_id)}
      style={{ "--stagger-delay": `${(index ?? 0) * 80}ms` } as React.CSSProperties}
      className="flex cursor-pointer flex-col items-center gap-2 rounded-xl bg-gray-800 p-6 animate-fade-in-up transition-colors hover:bg-gray-700"
    >
      <div className="text-4xl">{entry.kind === "directory" ? "\u{1F4C1}" : "\u{1F4E6}"}</div>
      <h2 className="text-lg font-medium">{entry.name}</h2>
      {entry.child_count != null && (
        <p className="text-sm font-mono tabular-nums text-gray-400">{entry.child_count} sets</p>
      )}
    </button>
  );
}
