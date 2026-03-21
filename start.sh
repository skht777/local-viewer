#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Start backend (from project root so "backend.main" is importable)
source "$ROOT_DIR/backend/.venv/bin/activate"
uvicorn backend.main:app --reload --port 8000 &
BACKEND_PID=$!

# Start frontend
cd "$ROOT_DIR/frontend"
npm run dev &
FRONTEND_PID=$!

trap "kill $BACKEND_PID $FRONTEND_PID 2>/dev/null; exit" INT TERM
echo "Backend:  http://localhost:8000"
echo "Frontend: http://localhost:5173"
wait
