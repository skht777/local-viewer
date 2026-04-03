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
- zustand for UI-only state (viewer mode, zoom level, sidebar open/close)
- Never duplicate server state in zustand
- TopPage は `GET /api/mounts` からマウントポイント一覧を取得して表示する

## Hooks
- Custom hooks prefixed with `use`
- Keyboard shortcuts via react-hotkeys-hook with focus context scoping
- Disable hotkeys when input/search bar is focused

## Styling
- Tailwind CSS v4 utility classes exclusively
- No inline styles, no CSS modules
- Dark theme fixed (bg-surface-base, text-white base, @theme tokens)
