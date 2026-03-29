// マウントポイント1つを表示するカード
// - 名前と子要素数を表示
// - クリックで onSelect コールバック

import type { MountEntry } from "../types/mount";

interface MountPointCardProps {
  mount: MountEntry;
  onSelect: (nodeId: string) => void;
  index?: number;
}

export function MountPointCard({ mount, onSelect, index }: MountPointCardProps) {
  return (
    <button
      type="button"
      data-testid={`mount-${mount.node_id}`}
      onClick={() => onSelect(mount.node_id)}
      style={{ "--stagger-delay": `${(index ?? 0) * 80}ms` } as React.CSSProperties}
      className="flex cursor-pointer flex-col items-center gap-2 rounded-xl bg-surface-card ring-1 ring-white/5 p-6 animate-fade-in-up transition-all duration-150 hover:bg-surface-raised hover:scale-[1.02]"
    >
      <div className="text-4xl">{"\u{1F4C1}"}</div>
      <h2 className="text-lg font-medium">{mount.name}</h2>
      {mount.child_count != null && (
        <p className="text-sm font-mono tabular-nums text-gray-400">{mount.child_count} sets</p>
      )}
    </button>
  );
}
