# ==============================================================================
# Stage 1: Build Rust binary
# ==============================================================================
FROM rust:1.88-slim-trixie AS rust-builder
WORKDIR /build

# hadolint ignore=DL3008
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev libclang-dev cmake g++ make \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add rustfmt

COPY Cargo.toml Cargo.lock ./
# `.cargo/config.toml` sets `GGML_*=OFF` so whisper.cpp does **not**
# inherit GitHub Actions runner SIMD features (which would SIGILL on
# any host lacking them — e.g. Proxmox VE default `kvm64`). Without
# this COPY the env vars are silently dropped and the released GHCR
# image is built with `-march=native`.
COPY .cargo/ ./.cargo/
COPY src/ ./src/
ARG CARGO_FEATURES=default
RUN cargo build --release --features "${CARGO_FEATURES}"

# ==============================================================================
# Stage 2: Build web UI
# ==============================================================================
FROM oven/bun:1.3.0-debian AS web-builder
WORKDIR /web

COPY web/package.json web/bun.lock ./
RUN bun install --frozen-lockfile
COPY web/ ./
RUN bun run build

# ==============================================================================
# Stage 3: Runtime
# ==============================================================================
# A literal FROM: Supervisor injects an Alpine BUILD_FROM default an ARG would accept.
FROM ghcr.io/hassio-addons/debian-base:9.3.0

# hadolint ignore=DL3008
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
        libgomp1 \
        libstdc++6 \
        curl \
        gosu \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -s /bin/false cortex-stt

COPY rootfs /

WORKDIR /app
COPY --from=rust-builder /build/target/release/cortex-stt /app/cortex-stt
RUN chmod +x /app/cortex-stt
COPY --from=web-builder /web/dist /app/web/dist

ARG BUILD_ARCH
ARG BUILD_DATE
ARG BUILD_REF
ARG BUILD_VERSION

LABEL \
    io.hass.name="Cortex STT Server" \
    io.hass.description="Multi-model speech-to-text service" \
    io.hass.arch="${BUILD_ARCH}" \
    io.hass.type="addon" \
    io.hass.version="${BUILD_VERSION}" \
    maintainer="hass-cortex" \
    org.opencontainers.image.title="Cortex STT Server" \
    org.opencontainers.image.description="Multi-model speech-to-text service" \
    org.opencontainers.image.vendor="hass-cortex" \
    org.opencontainers.image.licenses="MIT" \
    org.opencontainers.image.source="https://github.com/hass-cortex/app-cortex-stt" \
    org.opencontainers.image.created="${BUILD_DATE}" \
    org.opencontainers.image.revision="${BUILD_REF}" \
    org.opencontainers.image.version="${BUILD_VERSION}"
