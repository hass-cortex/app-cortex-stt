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
