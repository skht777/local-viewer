# CLAUDE.md -- Project Conventions for local-viewer

Local Content Viewer: a web app to browse images, videos, and PDFs
from local directories. FastAPI backend + React frontend, distributed
via Docker.

## Tech Stack
- Backend: FastAPI + uvicorn (Python 3.13)
- Frontend: React + Vite + TypeScript
- Styling: Tailwind CSS v4 (Vite plugin, no PostCSS config)
- State: TanStack Query (server) + zustand (UI only)
- Lint/Format: Ruff (Python), oxlint + oxfmt (TypeScript)
- Container: Docker multi-stage build

## Environment Setup
- Python 3.13 via pyenv (`pyenv install 3.13`)
- Node.js 24 via nodenv (`nodenv install 24.11.1`)
- Both version files (`.python-version`, `.node-version`) are in repo root

## Commands

```bash
# Dev (both servers)
./start.sh

# First-time setup
python -m venv backend/.venv && source backend/.venv/bin/activate && pip install -r backend/requirements-dev.txt
cd frontend && npm install && cd ..

# Docker
cp .env.example .env  # edit DATA_DIR first
docker compose up --build

# Lint (backend)
source backend/.venv/bin/activate && ruff check backend/ && ruff format --check backend/ && mypy backend/

# Lint (frontend)
npx oxlint frontend/src/ && npx oxfmt --check frontend/src/

# Test
source backend/.venv/bin/activate && pytest
cd frontend && npx vitest run
```

## Key Files
- `pyproject.toml` — Ruff + mypy + pytest config (at repo root, not backend/)
- `.oxlintrc.json` — oxlint config (react/typescript plugins)
- `backend/main.py` — FastAPI entry point
- `frontend/vite.config.ts` — Vite + Tailwind v4 + /api proxy + Vitest
- `.env.example` — Docker volume/port/resource config template

## Gotchas
- **uvicorn must run from project root** — `uvicorn backend.main:app`, not from `backend/`
- **Tailwind v4** — no `tailwind.config.js` or `postcss.config.js`, uses `@tailwindcss/vite` plugin
- **lint-staged uses venv path** — ruff is called as `backend/.venv/bin/ruff` in `package.json`
- **node_id opaque IDs** — API never exposes raw filesystem paths to client
- **Default bind 127.0.0.1** — LAN access requires explicit `BIND_HOST=0.0.0.0` in `.env`
