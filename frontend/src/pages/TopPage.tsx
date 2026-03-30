// ランディングページ
// - /api/mounts からマウントポイント一覧を取得
// - カードクリックで /browse/:nodeId に遷移

import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { mountListOptions } from "../hooks/api/mountQueries";
import { MountPointCard } from "../components/MountPointCard";
import { SearchBar } from "../components/SearchBar";

export default function TopPage() {
  const navigate = useNavigate();
  const { data, isLoading, error, refetch } = useQuery(mountListOptions());

  return (
    <div className="flex min-h-screen flex-col items-center px-8 py-16">
      <h1 className="mb-12 text-3xl font-bold">Local Content Viewer</h1>

      {isLoading && (
        <div className="mb-12 grid w-full max-w-4xl grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-3">
          {Array.from({ length: 3 }, (_, i) => (
            <div
              key={i}
              className="flex animate-pulse flex-col items-center gap-2 rounded-xl bg-surface-card p-6 ring-1 ring-white/5"
            >
              <div className="h-10 w-10 rounded-full bg-surface-raised" />
              <div className="h-5 w-24 rounded bg-surface-raised" />
              <div className="h-4 w-16 rounded bg-surface-raised" />
            </div>
          ))}
        </div>
      )}

      {error && (
        <div className="flex flex-col items-center gap-4">
          <p className="text-red-400">エラーが発生しました: {error.message}</p>
          <button
            type="button"
            onClick={() => refetch()}
            className="rounded bg-blue-600 px-4 py-2 text-sm text-white hover:bg-blue-500"
            data-testid="retry-button"
          >
            再試行
          </button>
        </div>
      )}

      {data && (
        <div className="mb-12 grid w-full max-w-4xl grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-3">
          {data.mounts.map((mount, index) => (
            <MountPointCard
              key={mount.node_id}
              mount={mount}
              index={index}
              onSelect={(nodeId) => navigate(`/browse/${nodeId}`)}
            />
          ))}
        </div>
      )}

      <div className="w-full max-w-md">
        <SearchBar />
      </div>
    </div>
  );
}
