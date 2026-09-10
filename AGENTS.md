# app-cortex-stt / cortex-stt

Multi-model speech-to-text HTTP service for Home Assistant, powered by
[transcribe.cpp](https://github.com/handy-computer/transcribe.cpp)
(single GGUF/ggml runtime for every model family — Whisper, Parakeet,
SenseVoice, Qwen3-ASR, …) via a unified Axum HTTP API (sync + async +
WebSocket streaming) plus a React admin UI. The model catalog is a
vendored snapshot of Handy's catalog (see `docs/adr/0003`). The outer
directory is the HA addon shell + repo metadata; the inner
`cortex-stt/` subdir is the Rust backend, React frontend, Dockerfile,
and rootfs.

## Quick Links

- **Supported models**: [transcribe.cpp supported-models table](https://github.com/handy-computer/transcribe.cpp#supported-models) + per-model cards under [`docs/models/`](https://github.com/handy-computer/transcribe.cpp/tree/main/docs/models); the installable subset is the vendored `cortex-stt/src/model/catalog.json` (synced by `cortex-stt/scripts/sync-catalog.py`).
- **Domain vocabulary**: [`cortex-stt/CONTEXT.md`](cortex-stt/CONTEXT.md) — what is a _Transcription history record_? _Drop audio_ vs _Delete record_? _Retention candidate_? _Evaluation sample_ vs _Pending capture_?
- **Contributor guide**: [`cortex-stt/CONTRIBUTING.md`](cortex-stt/CONTRIBUTING.md) — fork → branch → PR flow.
- **Release runbook**: workspace-level [`docs/release/`](../docs/release/README.md) — pipeline diagram, beta/stable cuts, troubleshooting.
- **Primary consumer**: [`cortex-stt`](https://github.com/hass-cortex/cortex-stt) HACS integration (HA STT platform). Standalone Docker / LXC / systemd packaging was removed before 0.1.0; the HA app is the only supported distribution form.

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
├── images/                    README + DOCS.md screenshots (one per admin-UI page)
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
    ├── Cargo.toml/.lock       Rust workspace root (single crate)
    ├── rust-toolchain.toml    pinned Rust version
    ├── clippy.toml, deny.toml, rustfmt.toml
    ├── CONTEXT.md             domain vocabulary
    ├── CONTRIBUTING.md
    ├── src/                   Rust backend (see Architecture)
    ├── tests/                 integration tests (real DB, mocked engines)
    └── web/                   React + Vite admin UI
```

## Architecture

```
src/
├── main.rs           Axum server bootstrap, signal handling
├── lib.rs            library exports
├── config.rs         clap CLI + env + TOML config (priority: CLI > ENV > config.toml > defaults)
├── state.rs          AppState (Arc<…>) shared across handlers + AppState::assemble (the single assembly point of the service object graph; main.rs and tests both call it)
├── job.rs            Async job concept: JobStore (terminal-is-terminal) + sweeper
├── error.rs          AsrError enum + status()/code()/related_id() (HTTP mapping lives on the variant)
├── cleanup.rs        background retention sweeper (hourly)
├── retention.rs      pure policy: (candidates, policy) → ids to drop
├── settings.rs       Settings struct (single owned shape; db stores it, api exposes it)
├── supervisor.rs     addon → Supervisor adapter: discovery POST + HA event on Install/Uninstall (live sync)
├── http.rs           shared pooled reqwest client (downloads + Supervisor calls)
├── eval/             model evaluation (samples + runs + results + judgements)
│   ├── mod.rs        Eval facade; the four invariants live here
│   ├── runner.rs     EvalRunner: model-major execution, explicit unload, renders like production
│   ├── memory.rs     /proc resident-set probe (does this model fit?)
│   └── store.rs      private SQL + the eval_* tables
├── history/          transcription history records (DB row + paired WAV audio)
│   ├── mod.rs        History struct; Delete record / Drop audio operations
│   ├── analytics.rs  aggregates for /api/metrics
│   └── store.rs      private SQL + types + records table DDL/migrations
├── text/             output rendering: how a transcript is written (ADR 0006)
│   ├── mod.rs        Locale (BCP-47 split) + LanguageModule/TextTransform + Renderer
│   └── mandarin.rs   Chinese script conversion; glyph-level OpenCC configs only
├── transcriber.rs    transcription pipeline (acquire → infer → render → save to history) + StreamSession
├── api/              Axum routes & middleware
│   ├── router.rs     build_router: the SHIPPED route + middleware stack (auth, CORS, access log); main serves it, tests exercise it
│   ├── auth.rs       Bearer token middleware (also `?api_key=` for SSE/WS)
│   ├── error.rs      ApiError DTO + IntoResponse glue (thin)
│   ├── transcribe.rs HTTP shell: decode audio + dispatch to Transcriber (sync/async)
│   ├── stream.rs     WebSocket streaming endpoint (ADR 0001 wire protocol)
│   ├── models.rs     model CRUD + download progress (catalog + downloads)
│   ├── engine.rs     engine status, load/unload, default-model selection
│   ├── settings.rs   GET/PUT runtime settings (RetentionPolicy lives in crate::retention)
│   ├── keys.rs       API key CRUD
│   ├── history.rs    HTTP shell over crate::history; /api/history/cleanup runs retention now
│   ├── eval.rs       HTTP shell over crate::eval (samples, runs, results, judgements)
│   ├── metrics.rs    aggregate stats (consumes history aggregates + catalog count)
│   ├── system.rs     system + storage info
│   ├── discovery.rs  Supervisor /discovery announce (startup task + manual trigger)
│   └── health.rs     /health (no auth)
├── engine/           speech engine lifecycle
│   ├── traits.rs     SpeechEngine trait (transcribe + streaming seam)
│   ├── manager.rs    engine selection, load/unload coordination, LRU eviction
│   ├── pool.rs       per-model thread-safe pool (Arc<Mutex<…>>)
│   ├── register.rs   factory registration + settings→engine sync (apply_engine_settings; idle-timeout projection lives on Settings::engine_idle_timeout)
│   ├── testing.rs    FakeEngine — the single configurable SpeechEngine test double (all tests use it)
│   └── transcribe_bridge.rs transcribe-cpp binding (the single engine path)
├── model/                  model installation
│   ├── catalog_data.rs     vendored catalog (include_str catalog.json; slug/quants/capabilities)
│   ├── catalog.json        converted Handy catalog snapshot (sync-catalog.py output)
│   ├── catalog.rs          ModelCatalog: list / get / resolve (Catalog|Custom) / delete / scan custom *.gguf
│   ├── download_manager.rs DownloadManager facade: start / cancel_download + queue + slots + completion tail
│   ├── download.rs         async download pipeline (HTTP + SHA-256; single-file GGUF)
│   ├── progress.rs         ProgressBoard: shared download-progress snapshots (manager writes, catalog reads)
│   ├── install.rs          ModelInstaller: Install (quant switch + register + notify) / Uninstall
│   ├── maintenance.rs      startup-once model-dir cleanup (legacy .bin/ONNX/.part artifacts)
│   ├── storage.rs          disk usage helpers (layout is flat: `{model_dir}/{filename}.gguf`)
│   └── types.rs            model type definitions (ModelInfo, DownloadPhase, …)
├── audio/              preprocessing
│   ├── canonical.rs    shared 16 kHz / 16-bit / mono constants
│   ├── wav_writer.rs   16-bit PCM WAV encoder (history persistence)
│   ├── resample.rs     WAV decode (PCM 16/24/32, IEEE float) + rubato resample
│   └── stats.rs        AudioStats: rms_db/peak_db/clip_ratio (per-record capture-quality metrics)
├── db/               SQLite (rusqlite, bundled) — settings + api keys storage
│   ├── database.rs   connection pool + migrations
│   ├── settings.rs   key-value settings
│   ├── keys.rs       API keys
│   └── mod.rs
└── bin/asr-cli.rs    one-shot CLI over the SAME Transcriber pipeline (in-memory throwaway history; no HTTP)
```

### Cross-module guarantees

- **`history::History` owns the row + audio file pair.** Delete operations remove the audio file before the DB row so a partial failure can never orphan a file. `audio_retention` triggers **Drop audio** (NULL the `audio_path`, remove the file; row survives), not Delete record. New rows store audio as 16-bit PCM WAV (`.wav`) so replaying history against a different model measures the model, not the codec; `.opus` files from the Ogg Opus era are served as-is via the file-extension-driven MIME on the replay endpoint.
- **`eval::Eval` owns the evaluation domain, and it is decoupled from history in both directions.** A sample copies its audio into its own directory (ADR 0005), so retention never sees it and deleting every history record leaves the set intact; unlabelled audio is a `PendingCapture` in a separate table, so "a sample always has a reference transcript" is a schema fact rather than a `WHERE` clause every query must remember. `EvalRunner` is the one composer of `acquire -> infer` **without** the `-> save to history` step — evaluation transcriptions are not history records, and one run makes more of them than a month of real use.
- **`retention::select_to_delete(candidates, policy)` is pure** — data-in / ids-out, no I/O. **`History::run_retention_sweep(record_policy, audio_policy)` is the single composer** of the gather → `select_to_delete` → apply flow (both policies, one best-effort error rule, returns a `SweepOutcome`); the hourly sweep in `cleanup.rs` and the manual `POST /api/history/cleanup` endpoint both call it rather than re-wiring the ingredients.
- **`transcriber::Transcriber` is the only composer** of `acquire → infer → save_to_history`. The HTTP handlers (sync / async), the WebSocket handler, and `asr-cli` are thin shells over `transcribe()` and `open_stream()` (the CLI wires an in-memory DB so history rows are discarded); a `StreamSession` holds a pool slot from open to finalize/drop and writes an aborted row when dropped mid-session.
- **`job::JobStore` owns the async-job lifecycle**: `complete`/`fail` transition only a still-`Processing` job and `cancel` marks a running job `Cancelled` — _terminal is terminal_, so a cancel that lands mid-inference is not clobbered by the worker's later completion. Handlers call these intent transitions; there is no blind status setter.
- **`model::ModelCatalog` is the single assembly point for model state**: registry + disk scan + live download status (via the shared `ProgressBoard`) + engine load state (via `EngineManager`) — `list_models`/`get_model` return the complete state, so no caller re-joins it. `DownloadManager` is the download facade: `start(model, quant)` and `cancel_download` own path resolution, slot claiming/compensation, and the completion tail (`complete`: Install → slot release → next-launch → delayed progress clear); the `ModelInstaller` is injected at construction.
- **`state::AppState::assemble` is the single assembly point of the service object graph** (`ProgressBoard → ModelCatalog → History → Transcriber → ModelInstaller → DownloadManager → Eval → EvalRunner`): `main.rs` and the test harness (`tests/test_helpers.rs`) both call it, so adding a dependency is a one-place change. `api::build_router` is the analogous seam for the route + middleware stack — `tests/api_router_test.rs` exercises the shipped router including auth. `asr-cli` deliberately wires a documented subset (no installer, throwaway history).
- **`History::metrics_snapshot` owns the metrics aggregate** — one SQL pass computes every history-derived figure; `api/metrics.rs` is a shell that joins in the engine/catalog/key counts. Storage figures likewise come from their owners (`History::audio_disk_usage_bytes`, `Database::disk_usage_bytes`) — handlers never re-derive another module's disk layout.
- **`model::install::ModelInstaller` owns Install and Uninstall** — the only two operations that change the installed model set (CONTEXT.md). Install (quant switch → engine re-registration → HA notify) is fired by the download task itself on `Completed`, before its slot is released, so a same-model re-download stays blocked throughout; it never fires for Failed/Cancelled (the cancel-flag guard). Uninstall (unload → delete files → HA notify) backs `DELETE /api/models/{id}`.

See [`cortex-stt/CONTEXT.md`](cortex-stt/CONTEXT.md) for the domain
vocabulary (Transcription history record, Delete record vs Drop audio,
Retention candidate, …) used in these names.

## Build

```bash
cd cortex-stt

cargo build                    # default features: engine (transcribe-cpp, static CPU)
cargo build --features cuda    # adds CUDA acceleration (requires CUDA toolkit)
cargo build --no-default-features  # mock-only build (tests / fast iteration)
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

| Feature   | Description                                                        |
| --------- | ------------------------------------------------------------------ |
| `default` | `engine` (transcribe-cpp + ouroboros), static CPU (tinyBLAS)       |
| `engine`  | the real inference path; without it every engine call is mock-only |
| `cuda`    | adds `transcribe-cpp/cuda`, requires CUDA toolkit                  |

### GGML_NATIVE=OFF

Set unconditionally via `.cargo/config.toml` — both as the plain env
var and forwarded to transcribe-cpp's CMake configure via
`TRANSCRIBE_CMAKE_ARGS="-DGGML_NATIVE=OFF"`. Without it, ggml compiles
with `-march=native` and crashes with `SIGILL` at model load if the
target host lacks an instruction the build host had (e.g. AVX-512 on
the dev box but not on the HA OS VM).

## Testing

```bash
cargo test                              # everything (unit + integration)
cargo test --lib                        # unit tests only (fast)
cargo test --lib retention              # single module
cargo test --test api_history_test      # single integration test file
cargo test -- --nocapture               # with stdout
```

All engine tests use a mock `SpeechEngine` — no real model files are
needed in CI. Integration tests under `tests/` exercise the real Axum
router + real (in-memory) SQLite, but stub the engine layer.

The shell-driven `tests/integration/model_pipeline_test.sh` is a
manual smoke test for the real download → transcribe pipeline (not
run in CI).

## API Endpoints

All `/api/*` routes require Bearer auth. The first API key is created
on first run via `--api-key` env or auto-generated `discovery_api_key`.

| Method      | Path                                 | Description                                                                   |
| ----------- | ------------------------------------ | ----------------------------------------------------------------------------- |
| POST        | `/api/transcribe`                    | Sync transcription (JSON)                                                     |
| GET         | `/api/transcribe/stream`             | WebSocket streaming session (see ADR 0001)                                    |
| POST        | `/api/transcribe/async`              | Async job submission                                                          |
| GET         | `/api/transcribe/jobs/{id}`          | Job status                                                                    |
| GET         | `/api/transcribe/jobs/{id}/result`   | Job result                                                                    |
| DELETE      | `/api/transcribe/jobs/{id}`          | Cancel job                                                                    |
| GET         | `/api/models`                        | List models                                                                   |
| POST        | `/api/models/scan`                   | Rescan model dir                                                              |
| POST        | `/api/models/{id}/download`          | Start download                                                                |
| DELETE      | `/api/models/{id}/download`          | Cancel download                                                               |
| GET         | `/api/models/{id}/download/progress` | Download progress (SSE)                                                       |
| DELETE      | `/api/models/{id}`                   | Delete downloaded model                                                       |
| GET         | `/api/engine`                        | Engine status                                                                 |
| POST        | `/api/engine/load`                   | Load model into memory                                                        |
| POST        | `/api/engine/unload`                 | Unload model                                                                  |
| PUT         | `/api/engine/default`                | Set default model                                                             |
| GET / PUT   | `/api/settings`                      | Runtime settings                                                              |
| GET         | `/api/keys`                          | List API keys                                                                 |
| POST        | `/api/keys`                          | Create API key                                                                |
| DELETE      | `/api/keys/{id}`                     | Revoke key                                                                    |
| GET         | `/api/history`                       | List transcription history                                                    |
| GET         | `/api/history/live`                  | Live history (SSE)                                                            |
| POST        | `/api/history/cleanup`               | Force retention sweep using current settings                                  |
| GET         | `/api/history/{id}`                  | Single record                                                                 |
| GET         | `/api/history/{id}/audio`            | Replay audio                                                                  |
| DELETE      | `/api/history/{id}`                  | Delete one record                                                             |
| POST        | `/api/history/delete`                | Delete the listed records (`{ids}`)                                           |
| DELETE      | `/api/history`                       | Delete all                                                                    |
| GET         | `/api/eval/overview`                 | Latest run summarised + sample-set composition                                |
| GET         | `/api/eval/samples`                  | List evaluation samples                                                       |
| DELETE      | `/api/eval/samples/{id}`             | Delete a sample (and its audio)                                               |
| POST        | `/api/eval/samples/delete`           | Delete the listed samples (`{ids}`)                                           |
| PUT         | `/api/eval/samples/{id}/reference`   | Correct a reference transcript                                                |
| GET         | `/api/eval/samples/{id}/audio`       | Sample audio (always WAV)                                                     |
| GET/POST    | `/api/eval/pending`                  | Pending captures / capture a history record                                   |
| POST        | `/api/eval/pending/upload`           | Capture an uploaded clip (raw `audio/wav` body)                               |
| DELETE      | `/api/eval/pending/{id}`             | Skip a pending capture                                                        |
| POST        | `/api/eval/pending/{id}/promote`     | Promote to a sample (requires a reference transcript)                         |
| GET         | `/api/eval/taken`                    | Origin record ids already claimed (picker greying)                            |
| GET/POST    | `/api/eval/runs`                     | List runs / start one (`model_ids`, `language`, optional `sample_ids` subset) |
| GET         | `/api/eval/runs/progress`            | Live run progress (SSE)                                                       |
| GET/DEL     | `/api/eval/runs/{id}`                | Run summary / delete                                                          |
| POST        | `/api/eval/runs/{id}/cancel`         | Stop the run in flight (cooperative; partial results are kept)                |
| PUT         | `/api/eval/runs/{id}/note`           | Record the conclusion                                                         |
| GET         | `/api/eval/results`                  | Filtered results (run, sample, model, text, device)                           |
| GET/PUT/DEL | `/api/eval/judgements`               | Rulings, keyed by (sample, output string)                                     |
| GET         | `/api/eval/models/{model_id}`        | One model across every run                                                    |
| GET         | `/api/system`                        | System info                                                                   |
| GET         | `/api/storage`                       | Storage info                                                                  |
| GET         | `/api/metrics`                       | Aggregate metrics                                                             |
| POST        | `/api/discovery/announce`            | Send Supervisor `/discovery` announce (manual re-trigger)                     |
| GET         | `/health`                            | Health check (no auth)                                                        |

## Home Assistant Discovery

Discovery announce to Supervisor `/discovery` is implemented in
`src/api/discovery.rs` (Rust) — there is **no** bashio-based
`discovery/run` service in `rootfs/`. Triggers:

1. **Startup** (auto): `main.rs` spawns a best-effort `tokio::spawn`
   after the HTTP listener binds. Failures log a warning but never
   fatal — the addon keeps serving requests.
2. **Manual** (on demand): `POST /api/discovery/announce` (Bearer
   auth). Used by the Admin UI's "Re-announce to Home Assistant"
   button.

Both call `cortex_stt::api::discovery::announce(&state)` which:

- Reads `SUPERVISOR_TOKEN` from env (else returns `NotInSupervisor`).
- Picks the system-managed API key (DB row with `system=true` and name
  `home-assistant-discovery`, fallback to first system row).
- Posts `{service: "cortex_stt", config: {host, port, api_key}}` where
  `host` comes from `gethostname` and `port` from `state.http_port`
  (so a custom `--http-port` is correctly announced).
- Maps Supervisor 4xx/5xx into
  `AsrError::SupervisorRejected{status, body}` — unlike
  `bashio::discovery`, real HTTP status codes propagate. The transport
  (token, pooled client, POST) lives in `src/supervisor.rs`; discovery
  errors are ordinary `AsrError` variants (codes `NOT_IN_SUPERVISOR`,
  `NO_API_KEY`, `SUPERVISOR_REJECTED`, …).

The integration's `async_step_hassio` consumes
`discovery_info.config['host']` and `['port']` (no scheme) to build
`http://<host>:<port>` and authenticates with `['api_key']`.

## Live model sync (HA event)

So the HA integration can add/remove a model's entities **without a
config-entry reload**, the addon **fires an event on the HA event bus**
whenever the set of downloaded models changes. Implemented in
`src/supervisor.rs` (the addon → Supervisor adapter, shared with the
discovery announce). No inbound endpoint, no URL registration, no
persistence — the integration just listens on the bus.

Uses the official add-on → HA core path:
POST the HA core REST API through the **Supervisor
proxy** at `http://supervisor/core/api/events/cortex_stt_models_changed`,
authenticated with `SUPERVISOR_TOKEN`. This requires
**`homeassistant_api: true`** in `config.yaml` (alongside the existing
`hassio_api: true` used for `/discovery`).

**Notify** — `notify_models_changed(event, model_id)`:

- No-op when `SUPERVISOR_TOKEN` is unset (dev / not under Supervisor).
- POSTs `{"event", "model_id"}` via the shared pooled client
  (`src/http.rs`) with a short per-request timeout and
  `bearer_auth(SUPERVISOR_TOKEN)`.
- Fire-and-forget: failures are logged, never propagated, so a model
  download/delete always succeeds.
- The inner `fire_event(core_api_base, token, …)` is split out so it is
  unit-testable against a mock receiver.

Fired from the two operations owned by `model::install::ModelInstaller`:

- `"model_added"` — at the end of an **Install**, right after
  `register_downloaded_models`. The download task awaits the Install on
  `Completed` (it is already detached, so it blocks nothing).
- `"model_removed"` — at the end of an **Uninstall**,
  **`tokio::spawn`-ed** after `catalog.delete_model` so the DELETE
  response returns immediately.

The payload is advisory — the HA listener re-fetches `/api/models` and
reconciles the full set, so it self-heals on a missed/duplicate event.

**Critical invariant** — `ModelCatalog::list_models` must report a
`DownloadPhase::Completed` model whose file exists as `Downloaded`
(`catalog.rs`), NOT `Downloading`. The event fires on download-complete
while the `Completed` progress entry still lingers (cleared ~later by
`remove_progress`); without this, HA's immediate reconcile would see
`Downloading`, filter the model out, and never add its entities. Any new
path that fires the event depends on this. Guarded end-to-end by
`completion_tail_reports_downloaded_to_catalog_before_progress_clears`
(`download_manager.rs`), which spans the completion tail and the catalog
view together.

## How To

### Syncing the model catalog

The catalog is a vendored snapshot of Handy's `catalog.json` (ADR 0003) — models are never added by hand. To pick up new upstream
models/quants:

```bash
cd cortex-stt
uv run scripts/sync-catalog.py            # fetch Handy main + HF sha256
# or pin a source: --source /path/to/catalog.json
```

The script rewrites `src/model/catalog.json` (with per-file sha256
resolved from Hugging Face LFS metadata). Review the diff, run
`cargo test --no-default-features` (catalog consistency tests), and
commit. Models that are not anonymously downloadable (gated repos) are
skipped with a warning.

One-off local models need no catalog entry: drop any `*.gguf` into the
model dir and hit `POST /api/models/scan` (it appears as a **Custom
model**).

## Distribution

A git-tag push on `hass-cortex/app-cortex-stt` drives the full
pipeline:

1. **`release.yml`** cross-compiles binaries and creates a GitHub
   Release. Tags containing `-` (e.g. `0.1.4-beta.1`) are auto-marked
   prerelease.
2. **`deploy.yaml`** (using `hassio-addons/workflows/app-deploy.yaml@v2.0.6`)
   builds multi-arch images, pushes
   `ghcr.io/hass-cortex/cortex_stt/amd64:<tag>` (plus `aarch64`), and
   dispatches `repository_dispatch` to one or both catalogs:
   - Stable tags → both **`hass-cortex/repository`** and
     **`hass-cortex/repository-beta`**.
   - Prerelease tags → **`hass-cortex/repository-beta`** only.
3. Each catalog's **`repository-updater.yaml`** filters releases per
   its `.apps.yml` `channel:` field (`stable` vs `beta`) and writes
   `cortex-stt/config.yaml`.

See the workspace-level [`docs/release/`](../docs/release/README.md)
for the full pipeline diagram, end-user install paths, and the
maintainer runbook (cutting beta then stable, troubleshooting).

### Release tags

**Re-sync the catalog before every tag.** The vendored snapshot pins a
SHA-256 per model file; when upstream re-uploads one, that model becomes
un-installable for everyone on the release and the failure reads as a
corrupt download rather than a stale pin.

```bash
uv run scripts/sync-catalog.py
git diff --quiet src/model/catalog.json \
  || echo "catalog drifted — review, cargo test, commit before tagging"
```

Use a lightweight tag — `git tag <version> && git push origin <version>`.
Do not pass `-a`/`-m`; annotated tags break the catalog
`repository-updater`.
