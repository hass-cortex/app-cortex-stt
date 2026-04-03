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

## HA App

Addon shell files (config.yaml, build.yaml, Dockerfile, run.sh, translations) are in `ha-apps/cortex-stt-server/`.
Deploy via `scripts/deploy.sh cortex-stt-server`. See workspace root `CLAUDE.local.md`.
