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
  const { data, isLoading, error } = useQuery(mountListOptions());

  return (
    <div className="flex min-h-screen flex-col items-center px-8 py-16">
      <h1 className="mb-12 text-3xl font-bold">Local Content Viewer</h1>

      {isLoading && <p className="text-gray-400">読み込み中...</p>}

      {error && <p className="text-red-400">エラーが発生しました: {error.message}</p>}

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
