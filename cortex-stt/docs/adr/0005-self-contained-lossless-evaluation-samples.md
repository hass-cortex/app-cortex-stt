# Evaluation samples own a lossless copy of their audio, decoupled from history

An **Evaluation sample** pairs one audio clip with a hand-typed **Reference
transcript**. A sample is created from a Transcription history record, but it
copies the audio into its own storage rather than referencing the record, and
only lossless audio is eligible.

## Why copy instead of reference

Referencing would require both retention policies to grow an exception:
`record_retention` (`Days(30)`) deletes the row and `audio_retention`
(`Days(14)`) deletes the file, so a referenced sample dies twice over.
`CONTEXT.md` defines a **Retention policy** as pure data over
`{id, created_at, size_bytes?}` that performs no I/O; teaching it "except the
pinned ones" inverts the dependency — retention would have to know about
evaluation.

The domain reading is cleaner too. A Transcription history record is evidence
that _a transcription happened_, and it expires. An Evaluation sample is an
assertion about _what was said_, and it does not. Different lifecycles, so
different things. Copying also makes audio provenance irrelevant, which admits
samples that never were history records.

The duplicated bytes are not a real cost: the median clip is 3.2 s ≈ 100 KB.

## Reference versus provenance

A sample does record the id of the history record it came from, and the
`capture_device` that recorded it. This is not the referencing rejected above,
and the distinction is the rule: **provenance is written, never read for data.**
The values are copied at creation and the sample is complete without them; when
retention later deletes the origin record, the stored id becomes a string that
points at nothing and the sample is unharmed.

They earn their place by enabling two things that need no data from history:
the import picker greys out records already taken, and evaluation results can be
sliced by microphone — which is what `CONTEXT.md` says `capture_device` exists
for.

## Why lossless only

Measured 2026-08-21 on the 37 lossless clips then in history, replaying each
through the encoder history used before 0.4.0 (libopus, 24 kbps VBR, VOIP,
16 kHz mono) and transcribing both copies with the same model:

| Model             | Transcript changed by the round trip |
| ----------------- | ------------------------------------ |
| Fun-ASR-Nano-2512 | 12/37 (32.4%)                        |
| SenseVoiceSmall   | 11/37 (29.7%)                        |

A control — the same file transcribed twice — differed 0/37, so inference is
deterministic and the whole difference is the codec. The failures are not
subtle: `关闭路况灯` became `刷屏不好呗`, `你好电视你好小智打开客厅灯` became
`你好天猫`.

A lossy sample therefore measures a model's tolerance of a codec this project
no longer writes, and can reorder the ranking it exists to produce. Admitting
lossy audio with a marker was rejected: it makes a broken evaluation set look
legitimate, and a footnote beside a score does not survive being read six
months later.

The rule is "the audio must be lossless", not "the audio must be WAV", so a
future lossless format is a new predicate rather than a changed rule.

## Consequences

- Evaluation storage is self-contained and exportable as a unit. This matters
  because the addon's `/data` is in no Home Assistant backup, and hand-typed
  reference transcripts cannot be regenerated.
- History records written before 0.4.0 hold Ogg Opus and can never become
  Evaluation samples. They are not migrated; retention reclaims them.
- Retention, and the `CONTEXT.md` vocabulary around it, is untouched by this
  feature.
