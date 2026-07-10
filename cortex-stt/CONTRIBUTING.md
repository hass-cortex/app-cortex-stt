# Contributing to Cortex STT Server

Thank you for your interest in contributing!

## Development Setup

### Prerequisites

- Rust (stable, see `rust-toolchain.toml`)
- [Bun](https://bun.sh) 1.3+ (for Web UI)

### Build

```bash
# Rust (no real models needed for development)
cargo build
cargo test

# Web UI
cd web
bun install
bun run dev    # Development server at http://localhost:5173
bun run build  # Production build
```

### Code Quality

```bash
# Rust
cargo fmt --check
cargo clippy -- -D warnings
cargo deny check

# Web UI
cd web
bun run lint
bun run typecheck
```

## Commit Convention

This project uses [Conventional Commits](https://www.conventionalcommits.org/):

| Prefix      | Use Case                                 |
| ----------- | ---------------------------------------- |
| `feat:`     | New feature                              |
| `fix:`      | Bug fix                                  |
| `docs:`     | Documentation only                       |
| `chore:`    | Maintenance / tooling                    |
| `refactor:` | Code restructure without behavior change |
| `test:`     | Adding or updating tests                 |

Examples:

```
feat: add Qwen3 ASR model support
fix: handle timeout in transcription handler gracefully
docs: add API key management guide
chore: update transcribe-cpp to 0.4.0
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
   cd web && bun run lint && bun run typecheck && bun run build
   ```
4. Write a clear PR description using the template
5. Request review

## Testing

All engine tests use mock `SpeechEngine` implementations. No real model files are needed in CI or local development.

```bash
# Run all tests
cargo test

# Run a specific test
cargo test test_pool_acquire_release

# Run with output
cargo test -- --nocapture
```

## Architecture

See [`AGENTS.md`](../AGENTS.md) for the full module tree, cross-module
invariants, and the API endpoint reference. [`CONTEXT.md`](CONTEXT.md)
defines the domain vocabulary those docs use.

Top-level modules:

- `src/engine/` — `SpeechEngine` trait + pool + LRU eviction
- `src/transcriber.rs` — transcription pipeline (acquire → infer → save)
- `src/history/` — transcription history records (DB row + paired Opus audio)
- `src/retention.rs` — pure retention policy (`Days` / `Count` / `DiskLimitMb`)
- `src/model/` — model catalog (`catalog.rs`) + download coordinator (`download_manager.rs`)
- `src/api/` — Axum routes; handlers are thin shells over the modules above
- `src/db/` — SQLite storage for settings + API keys
- `web/` — React Admin UI

## Adding New Models

Models are never added by hand — the catalog is a vendored snapshot of
Handy's `catalog.json` (see `docs/adr/0003`). When upstream adds new
models or quants:

1. Run `uv run scripts/sync-catalog.py` (rewrites `src/model/catalog.json` and regenerates `MODELS.md`)
2. Review the diff and run `cargo test --no-default-features` (catalog consistency tests)
3. Commit both files together

New model families need no code changes — every catalog model runs on
the single transcribe.cpp runtime behind the `SpeechEngine` trait.

## Questions?

Open a [Discussion](https://github.com/hass-cortex/app-cortex-stt/discussions) for questions or ideas.
