# cortex-stt-server

Multi-engine STT HTTP service powered by transcribe-rs.

## Build & Test

```bash
cargo build                    # No engines (for development/testing)
cargo build --features whisper # With Whisper engine (whisper.cpp)
cargo build --features onnx   # With ONNX engines (parakeet, sense-voice, etc.)
cargo build --features all-engines # All engines + VAD
cargo test --lib               # Unit tests only (fast)
cargo test                     # All tests (unit + integration)
cargo fmt --check              # Check formatting
cargo clippy -- -D warnings    # Lint
```

## Architecture

- `src/engine/` — Model pool, engine manager, model registry
- `src/api/` — HTTP API routes (transcribe, models, engine, health, etc.)
- `src/config.rs` — CLI + env config parsing
- `src/state.rs` — Shared application state

## Adding a New Model

Model archives come from diverse sources (HuggingFace, custom builds, third-party) with inconsistent packaging. **Before adding a model to the registry, always inspect the archive structure first:**

```bash
# Inspect tar.gz structure before registering
tar tzf <model>.tar.gz | head -20
```

Verify:
1. **Directory nesting** — is it `model/files...` or `model/model/files...`?
2. **Extraneous files** — macOS resource forks (`._*`), `.DS_Store`, `__MACOSX/`
3. **Expected filenames** — engine code looks for specific filenames (e.g. `cohere-encoder.int4.onnx`). Confirm they match.
4. **`archive_dir_name`** — set this to the top-level directory name inside the archive. The download logic unwraps single-directory nesting automatically (only counts directories, ignoring loose files).

The download extractor (`src/model/download.rs`) handles nested directories and stray files, but the `filename` field in `ModelDefinition` must match the final directory name that contains the model files.

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

### Deployment

The HA app stage (`addon` in Dockerfile) uses a **pre-built binary** — it does NOT compile Rust inside Docker. Deployment flow:

```bash
# 1. Build release binary locally
cargo build --release --features all-engines,vad-silero

# 2. Copy binary to CIFS-mounted addon directory
cp target/release/cortex-stt-server /mnt/ha/addons/cortex-stt-server/cortex-stt-server

# 3. Rebuild via Supervisor (packages binary into runtime image, no Rust compilation)
# ha_addon_action --slug=local_cortex_stt_server --action=rebuild

# 4. Wait for health check
while ! curl -sf http://192.168.10.34:10400/health > /dev/null 2>&1; do
  echo "Waiting..."; sleep 30
done && curl -s http://192.168.10.34:10400/health | jq .
```

**Notes:**
- Version unchanged → use `rebuild`. Version bumped → use `ha_store_reload` then `update`.
- Rebuild is async, typically ~1-2 minutes.
- If web UI changed, also rsync `web/dist/` to `/mnt/ha/addons/cortex-stt-server/web/dist/`.

### Volume Mounts

| Container Path | Source | Purpose |
|---|---|---|
| `/data` | Docker volume | Database, audio files |
| `/config` | addon_config | config.toml persistence |
| `/share/cortex-stt/models` | share | Models (persist across rebuilds) |
