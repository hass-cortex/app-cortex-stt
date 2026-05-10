# app-cortex-stt / cortex-stt

Multi-engine speech-to-text HTTP service for Home Assistant, powered by
[transcribe-rs](https://github.com/cjpais/transcribe-rs). Supports Whisper
(ggml), ONNX (Parakeet, SenseVoice) and Silero VAD via a unified Axum HTTP
API plus a React admin UI.

> **Repo layout**: outer dir = HA addon shell + repo metadata, inner
> `cortex-stt/` subdir = Rust backend + React frontend + Dockerfile +
> rootfs.
>
> **Distribution**: `gh release create` on `hass-cortex/app-cortex-stt` →
> `deploy.yaml` (hassio-addons/workflows app-deploy) builds multi-arch
> images → `ghcr.io/hass-cortex/cortex_stt/amd64` → dispatches
> `repository_dispatch` to `hass-cortex/repository` (catalog) → HA
> Supervisor pulls the image when users install.
>
> **Primary consumer**: [`cortex-stt`](https://github.com/hass-cortex/cortex-stt)
> HACS integration (HA STT platform). Standalone Docker / LXC / systemd
> distribution was removed before 0.1.0; HA addon is the only supported
> channel.

## Repository Layout

```
.
├── .github/workflows/         CI + release pipeline
│   ├── ci.yaml                hassio-addons app-ci (HA addon shell lint)
│   ├── ci.yml                 Rust + Bun checks (fmt/clippy/test/deny + lint/typecheck/build)
│   ├── deploy.yaml            release-triggered: app-deploy → GHCR + dispatch
│   └── release.yml            release-triggered: cross-compile binaries + GitHub Release
├── .yamllint, .mdlrc          lint configs (consumed by ci.yaml)
├── .pre-commit-config.yaml    pre-commit hook config
├── README.md                  user-facing install instructions
├── LICENSE.md                 MIT
├── images/                    README screenshots (history.png, models.png)
└── cortex-stt/                ── ADDON / SOURCE SUBDIR ──
    ├── config.yaml            HA addon metadata (slug=cortex_stt, port=8769, ingress)
    ├── build.yaml             multi-arch base image: hassio-addons/debian-base
    ├── Dockerfile             3-stage: rust-builder → web-builder → runtime
    ├── Dockerfile.local       single-stage variant for `scripts/dev.sh --init`
    ├── DOCS.md                HA App Store documentation page
    ├── icon.png, logo.png     addon icons (HA App Store + sidebar)
    ├── translations/en.yaml   addon UI translations
    ├── rootfs/                s6-overlay services (init oneshot + cortex-stt main)
    ├── .cargo/config.toml     GGML_NATIVE=OFF (avoid SIGILL on non-AVX-512 hosts)
    ├── .dockerignore
    ├── Cargo.toml/.lock       Rust workspace root (single crate)
    ├── rust-toolchain.toml    pinned Rust version
    ├── clippy.toml, deny.toml, rustfmt.toml
    ├── CONTRIBUTING.md
    ├── src/                   Rust backend (see Architecture below)
    ├── tests/                 integration tests (real DB, mocked engines)
    └── web/                   React + Vite admin UI
```

## Build & Test

```bash
cd cortex-stt

cargo build                    # default: all engines, CPU only
cargo build --features cuda    # all engines + CUDA GPU
cargo test --lib               # unit tests (fast)
cargo test                     # unit + integration (uses mock SpeechEngine)
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo deny check               # license + advisory audit

cd web
bun install --frozen-lockfile
bun run lint                   # biome
bun run typecheck              # tsc
bun run build                  # vite production build
```

### Feature flags

| Feature   | Description                                                           |
| --------- | --------------------------------------------------------------------- |
| `default` | `whisper` + `onnx` + `vad-silero`, CPU only                           |
| `cuda`    | adds `transcribe-rs/whisper-cuda` + `ort-cuda`, requires CUDA toolkit |

Internal features `whisper` / `onnx` / `vad-silero` exist for faster dev
builds but aren't intended as a public selector.

### GGML_NATIVE=OFF

Set unconditionally via `.cargo/config.toml`. Without it, whisper.cpp
compiles with `-march=native` and crashes with `SIGILL` at model load if
the target host lacks an instruction the build host had (e.g. AVX-512 on
the dev box but not on the HA OS VM).

## Architecture

```
src/
├── main.rs           Axum server bootstrap, signal handling
├── lib.rs            library exports
├── config.rs         clap CLI + env + TOML config (priority: CLI > ENV > config.toml > defaults)
├── state.rs          AppState (Arc<…>) shared across handlers
├── error.rs          custom error types (thiserror)
├── cleanup.rs        background task: expire old transcriptions / failed downloads
├── api/              Axum routes & middleware
│   ├── auth.rs       Bearer token middleware
│   ├── error.rs      HTTP error mapping
│   ├── transcribe.rs sync + SSE + async-job endpoints
│   ├── models.rs     model CRUD + download progress
│   ├── engine.rs     engine status, load/unload, default-model selection
│   ├── settings.rs   GET/PUT runtime settings
│   ├── keys.rs       API key CRUD
│   ├── history.rs    transcription history (incl. live SSE)
│   ├── metrics.rs    aggregate stats
│   ├── system.rs     system + storage info
│   ├── discovery.rs  Supervisor /discovery announce (startup task + manual trigger)
│   └── health.rs     /health (no auth)
├── engine/           model lifecycle
│   ├── traits.rs     SpeechEngine trait (the abstraction)
│   ├── manager.rs    engine selection, load/unload coordination
│   ├── pool.rs       per-model thread-safe pool (Arc<Mutex<…>>)
│   ├── registry.rs   builtin model catalog (id → URL, archive dir, etc.)
│   ├── register.rs   engine registration
│   ├── whisper_bridge.rs ggml binding via transcribe-rs
│   └── onnx_bridge.rs    ONNX Runtime binding (Parakeet/SenseVoice)
├── model/            download + storage
│   ├── manager.rs    metadata + availability
│   ├── storage.rs    on-disk layout (`{data_dir}/models/{id}/`)
│   ├── download.rs   async download w/ progress + archive extract
│   └── types.rs      model type definitions
├── audio/            preprocessing
│   ├── resample.rs   rubato-based rate conversion
│   └── wav_writer.rs WAV encoding for history snapshots
├── db/               SQLite (rusqlite, bundled)
│   ├── database.rs   connection pool + migrations
│   ├── settings.rs   key-value settings
│   ├── keys.rs       API keys
│   ├── records.rs    history records
│   └── mod.rs
└── bin/asr-cli.rs    one-shot CLI for direct STT testing (no HTTP)
```

All engine tests use a mock `SpeechEngine` — no real model files in CI.

## Testing

```bash
cargo test                              # everything (unit + integration)
cargo test --test api_health_test       # single integration test
cargo test --lib engine::pool::tests    # single unit test module
```

Integration tests under `tests/` exercise the real Axum router + real
SQLite, but stub the engine layer. The shell-driven
`tests/integration/model_pipeline_test.sh` is a manual smoke test for
the real download → transcribe pipeline (not run in CI).

## API Endpoints

| Method    | Path                                 | Description                                                 |
| --------- | ------------------------------------ | ----------------------------------------------------------- |
| POST      | `/api/transcribe`                    | Sync transcription (or SSE via `Accept: text/event-stream`) |
| POST      | `/api/transcribe/async`              | Async job submission                                        |
| GET       | `/api/transcribe/jobs/{id}`          | Job status                                                  |
| GET       | `/api/transcribe/jobs/{id}/result`   | Job result                                                  |
| DELETE    | `/api/transcribe/jobs/{id}`          | Cancel job                                                  |
| GET       | `/api/models`                        | List models                                                 |
| POST      | `/api/models/scan`                   | Rescan model dir                                            |
| POST      | `/api/models/{id}/download`          | Start download                                              |
| DELETE    | `/api/models/{id}/download`          | Cancel download                                             |
| GET       | `/api/models/{id}/download/progress` | Download progress (SSE)                                     |
| DELETE    | `/api/models/{id}`                   | Delete downloaded model                                     |
| GET       | `/api/engine`                        | Engine status                                               |
| POST      | `/api/engine/load`                   | Load model into memory                                      |
| POST      | `/api/engine/unload`                 | Unload model                                                |
| PUT       | `/api/engine/default`                | Set default model                                           |
| GET / PUT | `/api/settings`                      | Runtime settings                                            |
| GET       | `/api/keys`                          | List API keys                                               |
| POST      | `/api/keys`                          | Create API key                                              |
| DELETE    | `/api/keys/{id}`                     | Revoke key                                                  |
| GET       | `/api/history`                       | List transcription history                                  |
| GET       | `/api/history/live`                  | Live history (SSE)                                          |
| POST      | `/api/history/cleanup`               | Manual cleanup                                              |
| GET       | `/api/history/{id}`                  | Single record                                               |
| GET       | `/api/history/{id}/audio`            | Replay audio                                                |
| DELETE    | `/api/history/{id}`                  | Delete one record                                           |
| DELETE    | `/api/history`                       | Delete all                                                  |
| GET       | `/api/system`                        | System info                                                 |
| GET       | `/api/storage`                       | Storage info                                                |
| GET       | `/api/metrics`                       | Aggregate metrics                                           |
| POST      | `/api/discovery/announce`            | Send Supervisor `/discovery` announce (manual re-trigger)   |
| GET       | `/health`                            | Health check (no auth)                                      |

All `/api/*` routes require Bearer auth. The first API key is created on
first run via `--api-key` env or auto-generated `discovery_api_key`.

## Home Assistant Discovery

Discovery announce to Supervisor `/discovery` is implemented in
`src/api/discovery.rs` (Rust) — there is **no** bashio-based `discovery/run`
service in `rootfs/`. Triggers:

1. **Startup** (auto): `main.rs` spawns a best-effort `tokio::spawn` after the
   HTTP listener binds. Failures log a warning but never fatal — the addon keeps
   serving requests.
2. **Manual** (on demand): `POST /api/discovery/announce` (Bearer auth). Used
   by the Admin UI's "Re-announce to Home Assistant" button.

Both call `cortex_stt::api::discovery::announce(&state)` which:

- Reads `SUPERVISOR_TOKEN` from env (else returns `NotInSupervisor`).
- Picks the system-managed API key (DB row with `system=true` and name
  `home-assistant-discovery`, fallback to first system row).
- Posts `{service: "cortex_stt", config: {host, port, api_key}}` where `host`
  comes from `gethostname` and `port` from `state.http_port` (so a custom
  `--http-port` is correctly announced).
- Maps Supervisor 4xx/5xx into `DiscoveryError::SupervisorRejected{status, body}`
  — unlike `bashio::discovery`, real HTTP status codes propagate.

The integration's `async_step_hassio` consumes `discovery_info.config['host']`
and `['port']` (no scheme) to build `http://<host>:<port>` and authenticates
with `['api_key']`.

## Adding a New Model

Model archives ship with inconsistent packaging. **Always inspect before
registering:**

```bash
tar tzf <model>.tar.gz | head -20
```

Verify: (1) directory nesting depth, (2) no extraneous files (`._*`,
`.DS_Store`, etc.), (3) expected filenames match what the engine bridge
loads, (4) `archive_dir_name` in the registry matches the top-level dir
inside the archive. The extractor in `src/model/download.rs` handles
single-level nesting automatically.

Add a new entry to `builtin_models()` in `src/engine/registry.rs`,
covering: `id`, `engine`, archive URL, expected files, languages,
size hint, and optional default flags. Update tests in
`tests/registry_test.rs`.
