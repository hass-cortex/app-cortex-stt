# app-cortex-stt / cortex-stt

Multi-engine STT HTTP service powered by transcribe-rs.

> **Shell + Source**: Rust backend (`src/`, `Cargo.toml`), React frontend (`web/`) + HA addon shell (`Dockerfile`, `config.yaml`, `build.yaml`, `rootfs/`, `translations/`). 3-stage Dockerfile: Stage 1 (Rust toolchain) → Stage 2 (Node + Vite web UI) → Stage 3 (runtime from `hassio-addons/debian-base`).
>
> **Local dev**: `./scripts/dev.sh cortex-stt` → syncs built binary + `web/dist/` to `/mnt/ha/share/.dev/cortex-stt/`, then restart addon.
>
> **Publishing**: `gh release create` on `hass-cortex/app-cortex-stt` → CI builds multi-arch images → `ghcr.io/hass-cortex/cortex-stt` → dispatches to `ha-apps` (stable) + `ha-apps-beta`.

## Overview

Multi-engine speech-to-text service with model management, admin UI, and transcription history. Supports Whisper (ggml), ONNX (Parakeet/SenseVoice), and Silero VAD.

**Primary consumer:** `cortex-stt` HACS integration (HA STT platform).

## Build & Test

```bash
cd cortex-stt

cargo build                    # Default: all engines, CPU only
cargo build --features cuda    # All engines + CUDA GPU acceleration
cargo test --lib               # Unit tests only (fast)
cargo test                     # All tests (unit + integration)
cargo fmt --check              # Check formatting
cargo clippy -- -D warnings    # Lint

# Frontend
cd web && bun run build        # Vite build
```

### Feature Flags

| Feature | Description |
|---------|-------------|
| `default` | All engines (Whisper + ONNX + VAD), CPU only |
| `cuda` | All engines + CUDA GPU acceleration (requires CUDA toolkit) |

Internal features (`whisper`, `onnx`, `vad-silero`) exist for faster dev builds but are not intended for direct use.

### GGML_NATIVE=OFF

Set automatically via `.cargo/config.toml`. Prevents whisper.cpp from compiling with `-march=native` — if the target machine lacks build-machine instructions (e.g. AVX-512), Whisper crashes with SIGILL at model load.

## Architecture

- `src/engine/` — Model pool, engine manager, model registry
- `src/api/` — HTTP API routes (Axum)
- `src/config.rs` — CLI + env config parsing
- `src/state.rs` — Shared application state
- `web/` — React + Vite admin UI (model management, transcription history, settings)

All engine tests use mock `SpeechEngine` implementations — no real model files needed in CI.

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/transcribe` | Sync transcription (or SSE via `Accept: text/event-stream`) |
| POST | `/api/transcribe/async` | Async job submission |
| GET | `/api/transcribe/jobs/{id}` | Job status |
| GET | `/api/transcribe/jobs/{id}/result` | Job result |
| DELETE | `/api/transcribe/jobs/{id}` | Cancel job |
| GET | `/api/models` | List models |
| POST | `/api/models/scan` | Scan for new models |
| DELETE | `/api/models/{id}` | Delete model |
| POST | `/api/models/{id}/download` | Start download |
| DELETE | `/api/models/{id}/download` | Cancel download |
| GET | `/api/models/{id}/download/progress` | Download progress |
| GET | `/api/engine` | Engine status |
| PUT | `/api/engine/default` | Set default model |
| POST | `/api/engine/load` | Load model |
| POST | `/api/engine/unload` | Unload model |
| GET/PUT | `/api/settings` | Settings |
| CRUD | `/api/keys` | API key management |
| GET/DELETE | `/api/history/*` | Transcription history |
| GET | `/api/history/live` | Live history (SSE) |
| GET | `/api/system` | System info |
| GET | `/api/storage` | Storage info |
| GET | `/api/metrics` | Metrics |
| GET | `/health` | Health check (no auth) |

## Adding a New Model

Model archives have inconsistent packaging. **Always inspect before registering:**

```bash
tar tzf <model>.tar.gz | head -20
```

Verify: (1) directory nesting, (2) no extraneous files (`._*`, `.DS_Store`), (3) expected filenames match engine code, (4) `archive_dir_name` matches top-level directory inside archive. The download extractor (`src/model/download.rs`) handles nested dirs automatically.
