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
- ビューワー起動（ディレクトリ/アーカイブ/▶開く/Space 経由）は `useOpenViewerFromEntry` フック経由で行う。ページ側で `navigate()` を直接呼ばない（`setViewerOrigin` / `startViewerTransition` / `prefetchInfiniteQuery` / `replace: true` を内包）
- 履歴モデル: 開く系はすべて `{ replace: true }`、閉じる系は `navigate(-1)` + `viewerOrigin` フォールバック、セットジャンプも replace

## Styling
- Tailwind CSS v4 utility classes exclusively
- No inline styles, no CSS modules
- Dark theme fixed (bg-surface-base, text-white base, @theme tokens)

## PWA
- vite-plugin-pwa でオフラインキャッシュ
- サムネイル: CacheFirst (30日)
- API: NetworkFirst (5分)
