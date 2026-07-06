# WebSocket is the sole streaming transport, with a uniform contract over non-streaming models

With the move to transcribe.cpp (which exposes real incremental decoding via
feed/finalize), we needed a way to stream audio in and results out. We chose a
single bidirectional WebSocket endpoint as the only streaming transport, and we
removed the old SSE stage-event variant of `POST /api/transcribe` at the same
time.

## Decisions

1. **WebSocket, not chunked POST or SSE.** One transport serves every
   streaming consumer — the HA integration (which only needs the final text but
   gains latency from decode-while-speaking) and browser clients (which also
   want partials). Chunked POST would have been simpler for HA alone but adds a
   second transport the moment a partial-consuming client appears; SSE is
   server→client only and cannot carry the audio upload.
2. **Uniform contract with server-side fallback.** Models whose
   `capabilities.supports_streaming` is false still accept a stream session:
   the server buffers the audio and runs a single batch inference at finalize,
   emitting only the final transcript. Clients never branch on model
   capability; swapping the model never breaks the caller. The cost is that a
   "stream" against e.g. SenseVoice is silently store-and-forward — that is by
   design, not a bug.
3. **SSE stage events removed.** The `Accept: text/event-stream` variant of
   `POST /api/transcribe` existed to report coarse progress
   (engine_acquired → inference_started → result). It had no known consumers
   (the admin UI never called it; the HA integration uses sync POST) and real
   streaming supersedes it. Sync JSON and async jobs remain.

## Consequences

- Audio that exceeds the model's `max_audio_ms` is rejected with
  `INPUT_TOO_LONG` (see the input-length policy) — including at stream
  finalize for buffered-fallback models.
- The HA integration migrates from buffer-then-POST to feeding chunks over the
  WebSocket as they arrive from the Assist pipeline.
