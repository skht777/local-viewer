---
paths:
  - "backend/**/*.py"
---

# FastAPI Conventions

## Routing
- One router per resource: `mounts.py`, `browse.py`, `file.py`, `thumbnail.py`, `search.py`
- All routes under `/api/` prefix
- Use node_id path parameters, not raw filesystem paths
- `GET /api/mounts` — マウントポイント一覧（TopPage で使用）
- `GET /api/browse/{node_id}` — ディレクトリ一覧（ページネーション: limit, cursor, sort パラメータ）
- `GET /api/browse/{node_id}/first-viewable` — 再帰的に最初の閲覧対象を探索
- `GET /api/browse/{parent_node_id}/sibling` — 次/前の兄弟セットを返す
- `POST /api/thumbnails/batch` — バッチサムネイル（最大50件、base64 JSON）
- `GET /api/browse` のルート一覧機能は廃止 → `/api/mounts` に移譲

## Responses
- Common error model: `{"error": str, "code": str, "detail"?: any}`
- File responses: use `FileResponse` with ETag and Cache-Control headers
- Archive entries: `Response(content=bytes)` (小) or `FileResponse` via TempFileCache (大)

## Async
- Route handlers are `async def`
- CPU-bound work (archive extraction, pyvips, PDF): `run_in_threadpool`
- Never block the event loop

## Dependencies
- Shared services (archive_reader, node_registry, indexer, dir_index, thumbnail_warmer) as FastAPI dependencies
- browse_cursor はサービスではなく関数群（ステートレス、DI 不要）
- Configuration via environment variables, accessed through settings module
- MOUNT_BASE_DIR は .env → docker-compose.yml 経由でコンテナ環境変数に注入
