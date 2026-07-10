# Cortex STT — Domain Language

Multi-model speech-to-text HTTP service (single transcribe.cpp runtime). The vocabulary below covers
concepts callers reach for across the codebase. General programming
patterns (RAII guards, semaphores, broadcast channels) are not listed.

## Language

### Transcription

**Transcription pipeline**:
The shared flow `decode audio → acquire engine → run inference → save to history`. Audio decoding happens at the HTTP boundary; the rest lives in `transcriber.rs` (`Transcriber`), which the handlers (sync / async / stream session) and `asr-cli` drive.
_Avoid_: "request handler", "transcription service"

**Speech engine**:
A loaded model behind the `SpeechEngine` trait (transcribe.cpp / ggml runtime). Each instance admits one inference at a time; concurrency comes from the pool.
_Avoid_: "model" (a _model_ is a file on disk; a _speech engine_ is the loaded runtime)

**Stream session**:
One WebSocket transcription: the client feeds audio chunks and receives a final transcript; models that support streaming also emit partial transcripts along the way. Models that don't are buffered server-side and produce only the final — same wire contract either way.
_Avoid_: "streaming request", "live transcription" (overloaded with `/api/history/live`)

**Engine pool**:
A fixed-size set of `SpeechEngine` instances for one loaded model, fronted by a semaphore. One instance per slot.
_Avoid_: "instance pool", "model pool"

**Async job**:
In-memory record of a long-running request submitted via `POST /api/transcribe/async`. Lives in `JobStore`; never persisted.
_Avoid_: "async request", "deferred task"

### Models

**Catalog model**:
An entry in the vendored catalog (a converted snapshot of Handy's `catalog.json`) — slug, quant matrix, capabilities, languages. The set of _downloadable_ models.
_Avoid_: "built-in model" (pre-catalog term), "supported model"

**Quant**:
A precision variant of a catalog model, baked into its GGUF file (`Q4_K_M` … `F32`). Chosen at download time; at most one quant of a model exists on disk, so model identity never carries a quant dimension.

**Loaded model**:
A catalog (or custom) model whose pool has been instantiated and is resident in memory.

**Custom model**:
Any `.gguf` file placed in the model directory by hand and picked up by a rescan. Outside the catalog: no quant matrix, no download lifecycle; capabilities are read from the file itself.

**Default model**:
The model used when a transcription request omits `model=`. Configured via `/api/engine/default`.

**Install**:
The transition "download reached Completed → model usable": remove any other quant (one quant per model), refresh the engine factory registration, announce the change to HA (live model sync). Runs on the download task before its slot is released, and only for Completed — never Failed or Cancelled. Best-effort: a failed step logs and continues.
_Avoid_: "register" alone (the engine-factory step is one part of an Install), "post-download hook", "completion watch" (the old polling mechanism)

**Uninstall**:
The mirror operation: unload the Loaded model, delete its files, announce to HA. Deleting files is one step of an Uninstall, not the whole of it.
_Avoid_: "delete model" when the whole operation is meant

### History + retention

**Transcription history record**:
A persisted artifact of one transcription: a DB row, plus an _optional_ WAV file on disk. The two parts are paired and obey lifecycle invariants.
_Avoid_: "history entry", "log entry", "record" (alone)

**Capture device**:
The microphone / Assist satellite that recorded a transcription's audio, as reported by the client (`capture_device` on the request; free text, e.g. a HA device name). Persisted on the Transcription history record for per-microphone quality analysis, alongside the input-signal level stats (`rms_db`, `peak_db`, `clip_ratio`) the server computes itself.
_Avoid_: "device" (alone — that is the **compute backend** the engine ran on, e.g. "CPU"/"CUDA"), "source device" ("source" means http_api/ws_api)

**Delete record**:
The operation "drop a history record entirely" — removes the WAV (if present) **and** the DB row.

**Drop audio**:
The operation "remove only the audio portion" — removes the WAV **and** sets the row's `audio_path` to NULL. The row survives.
_Avoid_: "delete audio", "purge audio" (overloaded with Delete record)

**Retention policy**:
A rule for selecting which candidates to drop. Variants: `Days(n)`, `Count(n)`, `DiskLimitMb(n)`, `Unlimited`. Pure data — performs no I/O itself.

**Retention candidate**:
The minimal shape fed to the retention algorithm: `{id, created_at, size_bytes?}`. `size_bytes` is populated only when DiskLimitMb is in play.

**Record retention** / **Audio retention**:
Two independent retention policies applied separately. `record_retention` drives Delete record; `audio_retention` drives Drop audio. They can disagree by design (e.g. keep rows 30 days, but only keep their WAVs while they fit under a disk cap).

## Relationships

- A **Transcription history record** is produced by the **Transcription pipeline** and consumed by the History API + retention sweep.
- **Record retention** triggers **Delete record**; **Audio retention** triggers **Drop audio**. These two operations are _distinct_ — conflating them produces dangling `audio_path` references.
- A **Retention policy** is a pure value; applying it yields a set of ids. The policy never touches storage.
- A **Speech engine** is a loaded **Catalog model** (at its downloaded **Quant**); the **Engine pool** owns one or more instances per loaded model.
- A **Stream session** rides the same **Transcription pipeline** as sync/async — only the audio arrival and result delivery differ.
- **Install** and **Uninstall** are the only two operations that change the installed set, and both announce the change to HA. An Install is triggered by a download reaching Completed; an Uninstall by `DELETE /api/models/{id}`.

## Example dialogue

> **Dev:** "If `audio_retention = Days(7)` fires, do we lose the **Transcription history record**?"
> **Maintainer:** "No — that's **Drop audio**, not **Delete record**. The row survives with `audio_path = NULL` so the text and timing data stay queryable."

> **Dev:** "Can I run retention over the API key table too?"
> **Maintainer:** "Not today. **Retention policy** is value-shaped and reusable, but the only **Retention candidate** source is **Transcription history record**. Add a source if you have a real use case."

## Flagged ambiguities

- "record" alone is ambiguous — `db::records` is a storage backend module; `Record` types are wire shapes. After the history refactor, the public concept is **Transcription history record** and `db::records` collapses into `history::*`.
- "history" as an HTTP path (`/api/history`) refers to **Transcription history records**, not application logs.
- "cleanup" in code means _retention sweep_, not garbage collection in the language sense.
- "multi-engine" is a pre-transcribe.cpp phrase — there is one engine runtime now, many **Catalog models**. Say "multi-model".
- "streaming" is overloaded: a **Stream session** (WS transcription) is unrelated to `/api/history/live` (SSE tail of new records) and to download-progress SSE.
- "device" is overloaded: the `device` column/field is the **compute backend** ("CPU"/"CUDA"); the **Capture device** (`capture_device`) is the microphone that recorded the audio. Never mix the two.
