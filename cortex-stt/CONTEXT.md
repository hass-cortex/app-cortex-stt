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

**Output rendering**:
How a transcript is written, as distinct from what the model heard. A request's `language` carries both: the base subtag is the hint the engine matches against the model's declared set, and every subtag after it (`zh-TW`, `zh-Hant`, `sr-Latn`) selects a rendering applied on the way out. Renderings cover orthography only — glyph shape and regional spelling — never vocabulary, number formatting or punctuation. Omitting the subtag renders nothing. See `docs/adr/0006`.
_Avoid_: "normalisation" (that is a comparison-time idea, not an output one), "translation" (a different language, and a different parameter), "correction" (a rendering does not claim the model was wrong)

**Raw model output**:
What the model emitted before its family's post-processing cleaned it up — inline speaker markers, timestamp and special tokens, language / emotion / acoustic-event tags, chat-envelope prefixes. Families that emit clean text produce the same string as the transcript; only a difference is kept on the Transcription history record.
_Avoid_: "raw transcript" (it is precisely _not_ a transcript — the transcript is the cleaned text)

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
A persisted artifact of one transcription: a DB row, plus an _optional_ audio file on disk. The two parts are paired and obey lifecycle invariants.
_Avoid_: "history entry", "log entry", "record" (alone)

**Capture device**:
The microphone / Assist satellite that recorded a transcription's audio, as reported by the client (`capture_device` on the request; free text, e.g. a HA device name). Persisted on the Transcription history record for per-microphone quality analysis, alongside the input-signal level stats (`rms_db`, `peak_db`, `clip_ratio`) the server computes itself.
_Avoid_: "device" (alone — that is the **compute backend** the engine ran on, e.g. "CPU"/"CUDA"), "source device" ("source" means http_api/ws_api)

**Delete record**:
The operation "drop a history record entirely" — removes the audio file (if present) **and** the DB row.

**Drop audio**:
The operation "remove only the audio portion" — removes the audio file **and** sets the row's `audio_path` to NULL. The row survives.
_Avoid_: "delete audio", "purge audio" (overloaded with Delete record)

**Retention policy**:
A rule for selecting which candidates to drop. Variants: `Days(n)`, `Count(n)`, `DiskLimitMb(n)`, `Unlimited`. Pure data — performs no I/O itself.

**Retention candidate**:
The minimal shape fed to the retention algorithm: `{id, created_at, size_bytes?}`. `size_bytes` is optional; only DiskLimitMb reads it.

**Record retention** / **Audio retention**:
Two independent retention policies applied separately. `record_retention` drives Delete record; `audio_retention` drives Drop audio. They can disagree by design (e.g. keep rows 30 days, but only keep their audio while it fits under a disk cap).

### Evaluation

**Evaluation sample**:
One lossless audio clip paired with a hand-typed **Reference transcript** — both, always. It is born from a Transcription history record but owns a copy of the audio, so the two have no lifecycle in common: history expires, an assertion about what was said does not. See `docs/adr/0005`.
_Avoid_: "test case", "eval record" ("record" is already three things — see Flagged ambiguities)

**Reference transcript**:
What was actually said, typed by a person. The only source of truth in the evaluation domain; no model produces one.
_Avoid_: "ground truth" (says nothing about where it came from, which is the one thing that matters here), "expected text"

**Reference locale**:
The orthography a Reference transcript is written in. A reference is necessarily written in _some_ script, and an Evaluation run that renders to a different one is not comparable against it — so a run whose locale disagrees is flagged rather than scored. Unlabelled means unknown, not "matches whatever the run produced".
_Avoid_: "reference language" (the language is not in question; which script it is written in is)

**Pending capture**:
Audio copied into the evaluation store that has no Reference transcript yet. It is **not** an Evaluation sample — promotion, the moment a person confirms what was said, is the only way one comes into existence. Carries the transcript that existed at capture time as a starting point, and drops it on promotion.
_Avoid_: "draft sample", "unlabelled sample" (both imply it already is one)

**Candidate model**:
An installed model included in an **Evaluation run**. Only the installed set is eligible: evaluation never downloads, because a "compare these" button must not start gigabytes of traffic and a string of Installs.

**Evaluation run**:
One execution of the sample set against a set of Candidate models, at one point in time and one software version. Runs model-major — load a candidate, run every sample, unload — so a candidate is measured alone.
_Avoid_: "benchmark" (taken by the library-upgrade latency A/B)

**Evaluation result**:
The outcome for one (Evaluation sample, Candidate model) pair: transcript, **Raw model output**, inference time — or a load failure, which is a result for that model and not the end of the run.

**Judgement**:
A person's ruling on whether an output string is an acceptable transcription of a sample. Keyed by (sample, output string), never by (sample, model, run) — which is what lets the same string from another model, or from the same model next month, inherit the ruling instead of asking again.
_Avoid_: "score", "rating" (both suggest a number a machine computed)

## Relationships

- A **Transcription history record** is produced by the **Transcription pipeline** and consumed by the History API + retention sweep.
- **Record retention** triggers **Delete record**; **Audio retention** triggers **Drop audio**. These two operations are _distinct_ — conflating them produces dangling `audio_path` references.
- A **Retention policy** is a pure value; applying it yields a set of ids. The policy never touches storage.
- A **Speech engine** is a loaded **Catalog model** (at its downloaded **Quant**); the **Engine pool** owns one or more instances per loaded model.
- A **Stream session** rides the same **Transcription pipeline** as sync/async — only the audio arrival and result delivery differ.
- An **Evaluation sample** is produced by promoting a **Pending capture**, and is the only thing an **Evaluation run** reads. Unlabelled audio cannot be scored because it is not a sample.
- An **Evaluation run** yields one **Evaluation result** per (sample, **Candidate model**) pair; a **Judgement** attaches to the _output string_, so one ruling can cover many results across many runs.
- The evaluation domain reads the history and model APIs as sources, and nothing reads back into it: a **Retention policy** never sees an Evaluation sample, and deleting every Transcription history record leaves the evaluation set intact.
- **Install** and **Uninstall** are the only two operations that change the installed set, and both announce the change to HA. An Install is triggered by a download reaching Completed; an Uninstall by `DELETE /api/models/{id}`.

## Example dialogue

> **Dev:** "If `audio_retention = Days(7)` fires, do we lose the **Transcription history record**?"
> **Maintainer:** "No — that's **Drop audio**, not **Delete record**. The row survives with `audio_path = NULL` so the text and timing data stay queryable."

> **Dev:** "Can I run retention over the API key table too?"
> **Maintainer:** "Not today. **Retention policy** is value-shaped and reusable, but the only **Retention candidate** source is **Transcription history record**. Add a source if you have a real use case."

## Flagged ambiguities

- A model's **declared** languages are not the languages it can usefully transcribe. The catalog list is what the GGUF reports about itself; a model can declare `zh` and emit Thai. Declarations are a coarse filter, and disproving them is one of the things an **Evaluation run** is for.
- "record" alone is ambiguous — `ApiKeyRecord` is a different domain object entirely, `RecordMeta` is pipeline-internal and never stored, and `records` is the SQLite table. The domain concept is **Transcription history record**; say it in full. The evaluation domain deliberately calls its per-pair outcome an **Evaluation result**, not a record, rather than adding a fourth meaning.
- "history" as an HTTP path (`/api/history`) refers to **Transcription history records**, not application logs.
- "cleanup" in code means _retention sweep_, not garbage collection in the language sense.
- "multi-engine" is a pre-transcribe.cpp phrase — there is one engine runtime now, many **Catalog models**. Say "multi-model".
- "streaming" is overloaded: a **Stream session** (WS transcription) is unrelated to `/api/history/live` (SSE tail of new records) and to download-progress SSE.
- `language` answers two questions at once — which language the model listens for, and which orthography the result is written in. The split is by subtag: base to the engine, the rest to the **Output rendering**. It reads as one setting and is two, which is why a response reports `applied_transforms` rather than leaving a reader to infer it from the characters.
- "device" is overloaded: the `device` column/field is the **compute backend** ("CPU"/"CUDA"); the **Capture device** (`capture_device`) is the microphone that recorded the audio. Never mix the two.
