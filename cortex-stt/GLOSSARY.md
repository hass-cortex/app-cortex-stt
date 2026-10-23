# Cortex STT — Domain Language

A speech-to-text service for Home Assistant that runs many speech models on one runtime, keeps a
history of what it transcribed, and measures models against what was actually said.

## Language

### Transcription

**Transcription pipeline**:
The one flow every transcription takes: decode the audio, acquire a **Speech engine**, run inference, render the output, save a **Transcription history record**. Sync, async and stream transcriptions differ only in how audio arrives and results leave.
_Avoid_: "request handler", "transcription service"

**Speech engine**:
A loaded **Variant**, ready to run inference — one inference at a time per instance.
_Avoid_: "model" (a _model_ is what you install; a _speech engine_ is what runs)

**Engine pool**:
The fixed set of **Speech engine** instances for one loaded **Variant**; its size is how many transcriptions that Variant can run at once.
_Avoid_: "instance pool", "model pool"

**Output rendering**:
How a transcript is written, as distinct from what the model heard. A requested language carries both: its base subtag is what the model listens for, and every subtag after it (`zh-TW`, `zh-Hant`, `sr-Latn`) selects a rendering applied on the way out. Renderings cover orthography only — glyph shape and regional spelling — never vocabulary, number formatting or punctuation.
_Avoid_: "normalisation" (a comparison-time idea), "translation" (a different language), "correction" (a rendering does not claim the model was wrong)

**Raw model output**:
What the model emitted before its family's clean-up — speaker markers, timestamp and special tokens, language / emotion / acoustic-event tags, chat-envelope prefixes. Kept only when it differs from the transcript.
_Avoid_: "raw transcript" (it is precisely _not_ a transcript)

**Stream session**:
One live transcription: the client feeds audio as it is spoken and receives a final transcript; models that can stream also emit partial transcripts along the way, the rest produce only the final.
_Avoid_: "streaming request", "live transcription" (collides with the live history tail)

**Async job**:
A long transcription submitted to run in the background and collected later. Lives only in memory; a restart forgets it.
_Avoid_: "async request", "deferred task"

### Models

**Catalog model**:
A model the service knows how to download — an entry in the vendored snapshot of Handy's catalog, with its **Quants**, capabilities and declared languages.
_Avoid_: "built-in model", "supported model"

**Quant**:
A precision level a catalog model is published at (`Q4_K_M` … `F32`). A property of a **Variant**, not an identity on its own.
_Avoid_: "precision variant" (that is a **Variant**)

**Variant**:
One **Catalog model** at one **Quant**, installed — the unit that is installed, loaded, evaluated and offered to Home Assistant. A model can have several Variants installed and in use at once.
_Avoid_: "installed quant", "model" when one quant of it is meant

**Preferred variant**:
The **Variant** that serves a request naming a model but no **Quant**. Chosen by the user; the first Variant installed starts as it, and when it is uninstalled the earliest-installed remaining Variant takes over.

**Retired variant**:
An installed **Variant** whose **Quant** (or whole model) the catalog no longer offers. It keeps its identity and keeps serving; it can be uninstalled but never receives a **Model update**. A model renamed upstream is a new **Catalog model**: the old one's Variants become Retired, and nothing moves to the new name on its own.
_Avoid_: "orphan", "custom" (a Retired variant still belongs to its **Catalog model**)

**Custom model**:
A model file someone placed by hand, outside the catalog: no Quants to choose from, no downloads, capabilities read from the file itself. It has exactly one **Variant**, which has no **Quant**.

**Loaded model**:
A **Variant** whose **Engine pool** exists and is resident in memory.

**Default model**:
The model used when a transcription names none; its **Preferred variant** serves the request.

**Install**:
A finished download becoming a usable **Variant**: the next use reads the new file, and Home Assistant is told. Only a completed download installs — never a failed or cancelled one — and once a download is committed to installing, cancelling it no longer applies. The model's other Variants are untouched.
_Avoid_: "register" (one step of an Install), "post-download hook"

**Uninstall**:
The mirror of **Install** for one **Variant**: its file goes, it is unloaded, a kept download failure is dropped, Home Assistant is told; the model's other Variants keep serving. Removing a whole model is an Uninstall of each of its Variants.
_Avoid_: "delete model" (deleting the file is one step of an Uninstall)

**Fingerprint**:
The checksum an installed file was verified against when it was installed.

**Model update**:
A re-download of a **Variant** after upstream republished its file under the same name — offered when the **Fingerprint** no longer matches the catalog. The installed file keeps serving until the new one verifies and installs.
_Avoid_: "upgrade" (the Quant does not change)

### History + retention

**Transcription history record**:
What is kept of one transcription: its text, timing and metadata, plus — optionally — its audio. The two parts are paired and obey lifecycle rules together.
_Avoid_: "history entry", "log entry", "record" (alone)

**Capture device**:
The microphone or Assist satellite that recorded a transcription's audio, as the client reports it. Kept on the **Transcription history record**, beside the input-level measurements the service takes itself, for per-microphone quality analysis.
_Avoid_: "device" (alone — that is the **compute backend**, e.g. CPU or CUDA), "source device" ("source" is the API the transcription came through)

**Delete record**:
Removing a **Transcription history record** entirely — its audio and its row.

**Drop audio**:
Removing only a record's audio; the record itself survives.
_Avoid_: "delete audio", "purge audio" (both blur into Delete record)

**Retention policy**:
A rule for choosing what to drop: by age, by count, by disk size, or keep everything. A pure rule — applying it chooses, it never deletes.

**Retention candidate**:
What a **Retention policy** decides over: an identity, a creation time, and (for the disk-size rule) a size.

**Record retention** / **Audio retention**:
Two independent **Retention policies**: Record retention drives **Delete record**, Audio retention drives **Drop audio**. They can disagree by design — keep records 30 days, but their audio only while it fits under a disk cap.

### Evaluation

**Evaluation sample**:
One lossless audio clip paired with a hand-typed **Reference transcript** — both, always. Born from a **Transcription history record** but owning a copy of the audio, so the two share no lifecycle: history expires, an assertion about what was said does not.
_Avoid_: "test case", "eval record"

**Reference transcript**:
What was actually said, typed by a person. The only source of truth in evaluation; no model produces one.
_Avoid_: "ground truth" (says nothing about where it came from), "expected text"

**Reference locale**:
The orthography a **Reference transcript** is written in. A run rendering to a different one is not comparable against it, so it is flagged rather than scored. Unlabelled means unknown, not "matches whatever the run produced".
_Avoid_: "reference language" (the language is not in question; the script is)

**Pending capture**:
Audio set aside for evaluation that has no **Reference transcript** yet. It is **not** an **Evaluation sample** — promotion, a person confirming what was said, is the only way one comes into existence.
_Avoid_: "draft sample", "unlabelled sample" (both imply it already is one)

**Candidate model**:
An installed **Variant** included in an **Evaluation run**; two Quants of one model are two candidates. Only installed Variants are eligible: evaluating never downloads.

**Evaluation run**:
One pass of the sample set over a set of **Candidate models**, at one point in time and one software version, each candidate measured alone.
_Avoid_: "benchmark" (taken by the library-upgrade latency comparison)

**Evaluation result**:
The outcome for one (**Evaluation sample**, **Candidate model**) pair: transcript, **Raw model output**, inference time — or a load failure, which is a result for that candidate, not the end of the run.

**Judgement**:
A person's ruling on whether an output string is an acceptable transcription of a sample. It belongs to the (sample, output string) pair, never to a model or a run — so the same string from another model, or from the same model next month, inherits the ruling.
_Avoid_: "score", "rating" (both suggest a number a machine computed)

## Relationships

- The **Transcription pipeline** produces a **Transcription history record**; the History API and the retention sweep consume it.
- **Record retention** triggers **Delete record**; **Audio retention** triggers **Drop audio**. The two are _distinct_ — conflating them leaves records pointing at audio that is gone.
- A **Catalog model** has zero or more installed **Variants**, at most one per **Quant**; while any is installed, exactly one is its **Preferred variant**. Home Assistant sees one STT entity per Variant.
- A **Speech engine**, a **Loaded model**, the producer of a **Transcription history record** and an **Evaluation run**'s candidate are all **Variants**.
- **Install** and **Uninstall** are the only operations that change the installed set, and both tell Home Assistant.
- A **Stream session** rides the same **Transcription pipeline** as sync and async transcription.
- An **Evaluation sample** comes only from promoting a **Pending capture**, and is the only thing an **Evaluation run** reads.
- An **Evaluation run** yields one **Evaluation result** per (sample, **Candidate model**) pair; a **Judgement** attaches to the output string, so one ruling covers many results across many runs.
- Evaluation reads history and models and nothing reads back into it: a **Retention policy** never sees an **Evaluation sample**, and deleting every **Transcription history record** leaves the evaluation set intact.

## Example dialogue

> **Dev:** "If audio retention of 7 days fires, do we lose the **Transcription history record**?"
> **Maintainer:** "No — that's **Drop audio**, not **Delete record**. The record survives without its audio, so its text and timing stay queryable."

> **Dev:** "The living room pipeline should use Whisper Small at Q4, the study at Q8. Two models?"
> **Maintainer:** "One **Catalog model**, two **Variants**. Each is its own STT entity in Home Assistant; a request that names only the model gets the **Preferred variant**."

## Flagged ambiguities

- A model's **declared** languages are not the languages it can usefully transcribe: a model can declare `zh` and emit Thai. Declarations are a coarse filter; disproving them is one of the things an **Evaluation run** is for.
- "record" alone is ambiguous — API keys, pipeline metadata and stored history all get called records. The domain concept is **Transcription history record**; say it in full. Evaluation calls its per-pair outcome an **Evaluation result** rather than adding another meaning.
- "history" means **Transcription history records**, never application logs.
- "cleanup" means the retention sweep.
- "multi-engine" predates the single runtime — there is one engine runtime and many **Catalog models**. Say "multi-model".
- "streaming" is overloaded: a **Stream session** is unrelated to the live history tail and to download progress.
- A requested language answers two questions at once — which language the model listens for, and which orthography the result is written in. It reads as one setting and is two, split by subtag (see **Output rendering**).
- "device" is overloaded: the **compute backend** (CPU or CUDA) the engine ran on versus the **Capture device** that recorded the audio. Never mix the two.
- "model" is overloaded: a **Catalog model** (what you browse and download from) versus a **Variant** (what is installed and runs). Say which.
