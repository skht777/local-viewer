# Stage 1: Frontend build
FROM node:24-alpine AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ .
RUN npm run build

# Stage 2: Rust build
FROM rust:1-bookworm AS builder
WORKDIR /app

# mold リンカーで高速リンク
RUN apt-get update && apt-get install -y --no-install-recommends mold && rm -rf /var/lib/apt/lists/*

# 依存クレートのキャッシュ (ソース変更時に再コンパイルを最小化)
COPY backend/Cargo.toml backend/Cargo.lock ./backend/
COPY backend/.cargo ./backend/.cargo
RUN mkdir -p backend/src && \
    echo 'fn main() { println!("stub"); }' > backend/src/main.rs && \
    cd backend && cargo build --release && \
    rm -rf src

# 実ソースをコピーしてビルド (変更分のみ再コンパイル)
COPY backend/src ./backend/src
RUN cd backend && \
    touch src/main.rs && \
    cargo build --release

# Stage 3: Production runtime (Python 不要、Rust バイナリのみ)
FROM debian:bookworm-slim AS runtime

# curl: HEALTHCHECK, ffmpeg: 動画, unrar-free: RAR, p7zip-full: 7z,
# libvips42: PDF サムネイル (poppler), poppler-utils: pdftoppm
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    ffmpeg \
    unrar-free \
    p7zip-full \
    poppler-utils \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN useradd -m -r -s /bin/bash viewer

WORKDIR /app

# Rust バイナリ (単一ファイル、stripped)
COPY --from=builder /app/backend/target/release/local-viewer-backend ./

# Frontend 静的ファイル
COPY --from=frontend /app/frontend/dist ./static/

# マウント設定・インデックス DB の永続化ディレクトリ
RUN mkdir -p /app/config /app/data && chown -R viewer:viewer /app/config /app/data

USER viewer

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8000/api/health || exit 1

EXPOSE 8000

CMD ["./local-viewer-backend", "--port", "8000", "--bind", "0.0.0.0"]
