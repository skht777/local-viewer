// ルートコンポーネント
// - QueryClientProvider でサーバー状態管理を提供
// - Routes でページルーティングを定義

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Route, Routes } from "react-router-dom";
import TopPage from "./pages/TopPage";
import BrowsePage from "./pages/BrowsePage";
import SearchResultsPage from "./pages/SearchResultsPage";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5 * 60 * 1000, // ローカルファイルは頻繁に変わらない
      gcTime: 10 * 60 * 1000,
      retry: 1,
      refetchOnWindowFocus: false, // タブ切替での不要な再フェッチを防止
    },
  },
});

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <div className="min-h-screen bg-surface-base text-white">
        <Routes>
          <Route path="/" element={<TopPage />} />
          <Route path="/browse/:nodeId" element={<BrowsePage />} />
          <Route path="/search" element={<SearchResultsPage />} />
        </Routes>
      </div>
    </QueryClientProvider>
  );
}

export default App;
