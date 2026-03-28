# wyoming-asr

Multi-engine STT service with Wyoming protocol for Home Assistant.

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

- `src/wyoming/` — Wyoming protocol TCP server (event I/O, handler state machine)
- `src/engine/` — Model pool, engine manager, model registry
- `src/config.rs` — CLI + env config parsing
- `src/state.rs` — Shared application state

## Testing

All engine tests use mock `SpeechEngine` implementations. No real model files needed in CI.
