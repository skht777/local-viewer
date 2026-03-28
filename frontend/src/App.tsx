// ルートコンポーネント
// - QueryClientProvider でサーバー状態管理を提供
// - Routes でページルーティングを定義

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Routes, Route } from "react-router-dom";
import TopPage from "./pages/TopPage";
import BrowsePage from "./pages/BrowsePage";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30 * 1000,
      gcTime: 10 * 60 * 1000, // キャッシュ保持を10分に延長
      retry: 1,
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
        </Routes>
      </div>
    </QueryClientProvider>
  );
}

export default App;
