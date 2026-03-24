# Stage 1: Frontend build
FROM node:24-alpine AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ .
RUN npm run build

# Stage 2: Production runtime
FROM python:3.14-slim AS runtime

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1

# curl for HEALTHCHECK, unrar-free for RAR archives
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    unrar-free \
    && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN useradd -m -r -s /bin/bash viewer

WORKDIR /app

# Python deps (layer cached independently from code)
COPY backend/requirements.txt ./backend/
RUN pip install --no-cache-dir -r backend/requirements.txt

# Backend code
COPY backend/ ./backend/

# Frontend static assets from Stage 1
COPY --from=frontend /app/frontend/dist ./static/

USER viewer

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8000/api/health || exit 1

EXPOSE 8000

CMD ["python", "-m", "uvicorn", "backend.main:app", "--host", "0.0.0.0", "--port", "8000", "--loop", "uvloop"]
