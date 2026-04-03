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
- Shared services (archive_reader, node_registry, indexer) as FastAPI dependencies
- Configuration via environment variables, accessed through settings module
- MOUNT_BASE_DIR は .env → docker-compose.yml 経由でコンテナ環境変数に注入
