---
paths:
  - "frontend/src/**/*.{ts,tsx}"
---

# React Conventions

## Components
- Functional components only, no class components
- One component per file, filename matches component name (PascalCase)
- Props as interface, not type alias
- Destructure props in function signature

## State Management
- TanStack Query for all server data (API responses, caching, prefetch)
- `useInfiniteQuery` でページネーション（browse: limit=100, カーソルベース）
- **infinite query のプリフェッチは `prefetchInfiniteQuery` / `fetchInfiniteQuery` を使用**（`fetchQuery` は `pages/pageParams` 形状を満たさないため使用不可）。キャッシュキーは必ず `browseInfiniteOptions` 側に統一し、BrowsePage の読み込みと一致させる
- zustand for UI-only state (viewer mode, zoom level, sidebar open/close)
- Never duplicate server state in zustand
- 一時的なナビ状態（`viewerOrigin`, `viewerTransitionId` 等）は zustand の `partialize` で persist 除外する（リロード後に恒久状態化するバグを防止）
- TopPage は `GET /api/mounts` からマウントポイント一覧を取得して表示する
- バッチサムネイル: `useBatchThumbnails` で Blob URL 管理（BATCH_SIZE=100, 安定チャンク分割, 自動 revokeObjectURL）
- 仮想グリッド: `useVirtualGrid` で FileBrowser の大量カードを効率レンダリング

## Hooks
- Custom hooks prefixed with `use`
- Keyboard shortcuts via react-hotkeys-hook with focus context scoping
- Disable hotkeys when input/search bar is focused
- ビューワー起動（ディレクトリ/アーカイブ/▶開く/Space 経由）は `useOpenViewerFromEntry` フック経由で行う。ページ側で `navigate()` を直接呼ばない（`setViewerOrigin` / `startViewerTransition` / `prefetchInfiniteQuery` / push 遷移を内包）
- 履歴モデル:
  - **viewer 起動経路は push**（`useOpenViewerFromEntry` / `useViewerParams.openViewer` / `useViewerParams.openPdfViewer` / `SearchBar` の PDF 検索結果クリック）。ブラウザバックで open 直前の URL に戻れることを保証
  - **close は現状維持**: `viewerOrigin` あれば `navigate(originUrl, { replace: true })` で起点復帰、無ければ `setSearchParams(buildCloseImageSearch)` で search 削除（deep link fallback）
  - **セットジャンプは replace 維持**: `useSetJump` の navigate は `{ replace: true }` のまま、履歴を汚染しない
  - **ブラウズ間 navigate は自身重複を抑制**: `BrowsePage` の `navigateBrowse` callback で `targetNodeId === nodeId` を早期 return、ツリー/パンくず往復で history が膨らむのを防ぐ
  - **viewer 起動でない検索結果遷移**: `SearchBar` の image/video 結果は scope ありで `viewerOrigin` 設定 + replace（既存挙動）、scope なしで push

## Styling
- Tailwind CSS v4 utility classes exclusively
- No inline styles, no CSS modules
- Dark theme fixed (bg-surface-base, text-white base, @theme tokens)
- 例外: ランタイム計算値（仮想化の座標計算 `translateY` / `height` / grid 列数、スタガーアニメーションの CSS 変数注入 `--stagger-delay` など）は inline style を許容する。Tailwind 任意値で表現できる静的値には適用しない

## PWA
- vite-plugin-pwa でオフラインキャッシュ
- サムネイル: CacheFirst (30日)
- API: NetworkFirst (5分)
