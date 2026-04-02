# cortex-stt-server

Multi-engine STT HTTP service powered by transcribe-rs.

## Build & Test

```bash
cargo build                    # No engines (for development/testing)
cargo build --features whisper # With Whisper engine (whisper.cpp)
cargo build --features onnx   # With ONNX engines (parakeet, sense-voice, etc.)
cargo build --features all-engines # All engines + VAD
cargo test                     # Run all tests (no real models needed)
cargo fmt --check              # Check formatting
cargo clippy -- -D warnings    # Lint
```

## Architecture

- `src/engine/` — Model pool, engine manager, model registry
- `src/api/` — HTTP API routes (transcribe, models, engine, health, etc.)
- `src/config.rs` — CLI + env config parsing
- `src/state.rs` — Shared application state

## Testing

All engine tests use mock `SpeechEngine` implementations. No real model files needed in CI.

## HA Addon

Dockerfile multi-stage build: stages 1-2 compile Rust binary and web UI, stage 3c (`addon`) packages them into an HA-compatible image.

### Addon Files

| File | Purpose |
|------|---------|
| `config.yaml` | HA addon metadata, options schema |
| `build.yaml` | Build args (CARGO_FEATURES), base image |
| `run.sh` | Bashio entrypoint, reads HA config, launches binary |
| `translations/en.yaml` | UI labels for config options |
| `Dockerfile` | Multi-stage: `rust-builder` → `web-builder` → `addon` |

### Addon Slug

- Published: `cortex_stt_server`
- Local dev: `local_cortex_stt_server`

### Port

| Port | Purpose |
|------|---------|
| 10400 | HTTP API + Admin UI (via Ingress) |

### Volume Mounts

| Container Path | Source | Purpose |
|---|---|---|
| `/data` | Docker volume | Database, audio files |
| `/config` | addon_config | config.toml persistence |
| `/share/cortex-stt/models` | share | Models (persist across rebuilds) |
