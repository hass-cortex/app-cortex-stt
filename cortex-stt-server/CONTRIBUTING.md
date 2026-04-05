# Contributing to Cortex STT Server

Thank you for your interest in contributing!

## Development Setup

### Prerequisites

- Rust (stable, see `rust-toolchain.toml`)
- Node.js 22+ (for Web UI)
- npm (for Web UI dependencies)

### Build

```bash
# Rust (no real models needed for development)
cargo build
cargo test

# Web UI
cd web
npm install
npm run dev    # Development server at http://localhost:5173
npm run build  # Production build
```

### Code Quality

```bash
# Rust
cargo fmt --check
cargo clippy -- -D warnings
cargo deny check

# Web UI
cd web
npx biome check .
npx tsc --noEmit
```

## Commit Convention

This project uses [Conventional Commits](https://www.conventionalcommits.org/):

| Prefix | Use Case |
|--------|----------|
| `feat:` | New feature |
| `fix:` | Bug fix |
| `docs:` | Documentation only |
| `chore:` | Maintenance / tooling |
| `refactor:` | Code restructure without behavior change |
| `test:` | Adding or updating tests |

Examples:
```
feat: add Qwen3 ASR engine support
fix: handle timeout in transcription handler gracefully
docs: add API key management guide
chore: update transcribe-rs to 0.4.0
refactor: extract audio resampling to separate module
test: add pool eviction edge case tests
```

## Pull Request Process

1. Fork the repository and create a feature branch from `main`
2. Make your changes following the code quality standards
3. Ensure all checks pass:
   ```bash
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo test
   cargo deny check
   cd web && npx biome check . && npx tsc --noEmit && npm run build
   ```
4. Write a clear PR description using the template
5. Request review

## Testing

All engine tests use mock `SpeechModel` implementations. No real model files are needed in CI or local development.

```bash
# Run all tests
cargo test

# Run a specific test
cargo test test_pool_acquire_release

# Run with output
cargo test -- --nocapture
```

## Architecture

See the [design spec](docs/design/) for architecture details.

Key modules:
- `src/engine/` - Model pool and engine management
- `src/api/` - Axum HTTP API and static file serving
- `src/model/` - Model download and storage
- `web/` - React Admin UI

## Adding a New Engine

When transcribe-rs adds a new engine:

1. Add feature flag to `Cargo.toml`
2. Add model entries to `src/engine/registry.rs`
3. Update `build.yaml` `CARGO_FEATURES` if it should be included by default
4. Update the supported models table in `README.md`
5. No architectural changes needed - engines are abstracted behind the `SpeechModel` trait

## Questions?

Open a [Discussion](https://github.com/hass-cortex/cortex-stt-server/discussions) for questions or ideas.
