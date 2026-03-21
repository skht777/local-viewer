"""Local Content Viewer -- FastAPI entry point."""

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

app = FastAPI(
    title="Local Content Viewer",
    version="0.1.0",
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:5173"],  # Vite dev server
    allow_methods=["GET"],
    allow_headers=["*"],
)


@app.get("/api/health")
async def health() -> dict[str, str]:
    """Health check endpoint."""
    return {"status": "ok"}
