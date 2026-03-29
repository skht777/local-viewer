---
paths:
  - "backend/**/*.py"
---

# FastAPI Conventions

## Routing
- One router per resource: `browse.py`, `file.py`, `search.py`
- All routes under `/api/` prefix
- Use node_id path parameters, not raw filesystem paths

## Responses
- Common error model: `{"error": str, "code": str, "detail"?: any}`
- File responses: use `FileResponse` with ETag and Cache-Control headers
- Archive entries: stream via `StreamingResponse`

## Async
- Route handlers are `async def`
- CPU-bound work (archive extraction, Pillow, PDF): `run_in_threadpool`
- Never block the event loop

## Dependencies
- Shared services (archive_reader, node_registry, indexer) as FastAPI dependencies
- Configuration via environment variables, accessed through settings module
