# Cortex STT — Domain Language

Multi-engine speech-to-text HTTP service. The vocabulary below covers
concepts callers reach for across the codebase. General programming
patterns (RAII guards, semaphores, broadcast channels) are not listed.

## Language

### Transcription

**Transcription pipeline**:
The shared flow `decode audio → acquire engine → run inference → save to history`. Lives in `api/transcribe.rs` today as a set of private helpers; the three handlers (sync / SSE / async) compose it.
_Avoid_: "request handler", "transcription service"

**Speech engine**:
A loaded model behind the `SpeechEngine` trait — Whisper (ggml), Parakeet (ONNX), SenseVoice (ONNX). Each instance is single-threaded; concurrency comes from the pool.
_Avoid_: "model" (a *model* is a file on disk; a *speech engine* is the loaded runtime)

**Engine pool**:
A fixed-size set of `SpeechEngine` instances for one loaded model, fronted by a semaphore. One instance per slot.
_Avoid_: "instance pool", "model pool"

**Async job**:
In-memory record of a long-running request submitted via `POST /api/transcribe/async`. Lives in `JobStore`; never persisted.
_Avoid_: "async request", "deferred task"

### Models

**Built-in model**:
An entry in the static registry (`engine/registry.rs`) — id, download URL, archive layout. The set of *downloadable* models.
_Avoid_: "supported model"

**Loaded model**:
A built-in model whose pool has been instantiated and is resident in memory. Subset of built-in.

**Default model**:
The model used when a transcription request omits `model=`. Configured via `/api/engine/default`.

### History + retention

**Transcription history record**:
A persisted artifact of one transcription: a DB row, plus an *optional* WAV file on disk. The two parts are paired and obey lifecycle invariants.
_Avoid_: "history entry", "log entry", "record" (alone)

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
- **Record retention** triggers **Delete record**; **Audio retention** triggers **Drop audio**. These two operations are *distinct* — conflating them produces dangling `audio_path` references.
- A **Retention policy** is a pure value; applying it yields a set of ids. The policy never touches storage.
- A **Speech engine** is a loaded **Built-in model**; the **Engine pool** owns one or more instances per loaded model.

## Example dialogue

> **Dev:** "If `audio_retention = Days(7)` fires, do we lose the **Transcription history record**?"
> **Maintainer:** "No — that's **Drop audio**, not **Delete record**. The row survives with `audio_path = NULL` so the text and timing data stay queryable."

> **Dev:** "Can I run retention over the API key table too?"
> **Maintainer:** "Not today. **Retention policy** is value-shaped and reusable, but the only **Retention candidate** source is **Transcription history record**. Add a source if you have a real use case."

## Flagged ambiguities

- "record" alone is ambiguous — `db::records` is a storage backend module; `Record` types are wire shapes. After the history refactor, the public concept is **Transcription history record** and `db::records` collapses into `history::*`.
- "history" as an HTTP path (`/api/history`) refers to **Transcription history records**, not application logs.
- "cleanup" in code means *retention sweep*, not garbage collection in the language sense.
