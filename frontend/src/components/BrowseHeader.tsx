// ファイルブラウザーのヘッダー
// - 「← トップ」ナビゲーション
// - 現在パス表示
// - 検索バー

import { useNavigate } from "react-router-dom";
import { SearchBar } from "./SearchBar";

interface BrowseHeaderProps {
  currentName: string;
}

export function BrowseHeader({ currentName }: BrowseHeaderProps) {
  const navigate = useNavigate();

  return (
    <header className="flex items-center gap-4 border-b border-gray-700 bg-gray-800 p-4">
      <button
        type="button"
        onClick={() => navigate("/")}
        className="rounded-lg px-3 py-1.5 text-sm text-gray-300 transition-colors hover:bg-gray-700 hover:text-white"
      >
        ← トップ
      </button>
      <span className="text-sm text-gray-400">{currentName}</span>
      <div className="ml-auto w-64">
        <SearchBar />
      </div>
    </header>
  );
}
