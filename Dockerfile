# ============================================================================
# wyoming-asr Dockerfile
# Multi-stage build with CPU, CUDA, and HA App targets
# ============================================================================

# ---------- Args ----------
ARG BUILD_FROM
ARG CUDA_VERSION=12.8.1
ARG RUST_VERSION=1.85
ARG NODE_VERSION=22
ARG TARGET_ARCH=x86_64-unknown-linux-gnu

# ============================================================================
# Stage 1: Rust builder
# ============================================================================
FROM rust:${RUST_VERSION}-bookworm AS rust-builder

ARG TARGET_ARCH
ARG CARGO_FEATURES="all-engines,vad-silero"

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake \
    pkg-config \
    libssl-dev \
    libclang-dev \
    clang \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release --features "${CARGO_FEATURES}" || true \
    && rm -rf src

# Copy source and build
COPY src/ src/
RUN touch src/main.rs \
    && cargo build --release --features "${CARGO_FEATURES}" \
    && strip target/release/wyoming-asr \
    && cp target/release/wyoming-asr /wyoming-asr

# ============================================================================
# Stage 2: Web UI builder
# ============================================================================
FROM node:${NODE_VERSION}-bookworm-slim AS web-builder

WORKDIR /build/web

# Install dependencies first for caching
COPY web/package.json web/package-lock.json* ./
RUN npm ci

# Build React app
COPY web/ ./
RUN npm run build

# ============================================================================
# Stage 3a: CPU runtime
# ============================================================================
FROM debian:bookworm-slim AS cpu

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    libgomp1 \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary and web assets
COPY --from=rust-builder /wyoming-asr /app/wyoming-asr
COPY --from=web-builder /build/web/dist /app/web/dist

# Create data directories
RUN mkdir -p /data/models /data/audio /config \
    && useradd -r -s /bin/false wyoming \
    && chown -R wyoming:wyoming /data /config

USER wyoming

EXPOSE 10300 10400

HEALTHCHECK --interval=30s --timeout=5s --start-period=600s --retries=3 \
    CMD curl -sf http://localhost:10400/health || exit 1

ENTRYPOINT ["/app/wyoming-asr"]
CMD ["--data-dir", "/data", "--static-dir", "/app/web/dist"]

# ============================================================================
# Stage 3b: CUDA runtime
# ============================================================================
FROM nvidia/cuda:${CUDA_VERSION}-runtime-ubuntu24.04 AS cuda

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3t64 \
    libgomp1 \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary and web assets
COPY --from=rust-builder /wyoming-asr /app/wyoming-asr
COPY --from=web-builder /build/web/dist /app/web/dist

# Create data directories
RUN mkdir -p /data/models /data/audio /config \
    && useradd -r -s /bin/false wyoming \
    && chown -R wyoming:wyoming /data /config

USER wyoming

ENV NVIDIA_VISIBLE_DEVICES=all
ENV NVIDIA_DRIVER_CAPABILITIES=compute,utility

EXPOSE 10300 10400

HEALTHCHECK --interval=30s --timeout=5s --start-period=600s --retries=3 \
    CMD curl -sf http://localhost:10400/health || exit 1

ENTRYPOINT ["/app/wyoming-asr"]
CMD ["--data-dir", "/data", "--static-dir", "/app/web/dist", "--gpu-mode", "cuda"]

# ============================================================================
# Stage 3c: HA App (S6-overlay base)
# ============================================================================
FROM ${BUILD_FROM} AS addon

# Install runtime dependencies
RUN \
    apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
        libgomp1 \
        curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary and web assets
COPY --from=rust-builder /wyoming-asr /app/wyoming-asr
COPY --from=web-builder /build/web/dist /app/web/dist

# Copy S6-overlay service definitions
COPY rootfs /

# Create data directories (S6 runs as root, binary drops privileges)
RUN mkdir -p /data/models /data/audio

EXPOSE 10300 10400

# Healthcheck: 10 minute start period for first-run model download
HEALTHCHECK --interval=30s --timeout=5s --start-period=600s --retries=3 \
    CMD curl -sf http://localhost:10400/health || exit 1
