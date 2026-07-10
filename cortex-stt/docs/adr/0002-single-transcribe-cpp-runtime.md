# Single transcribe.cpp runtime; ONNX and whisper-rs paths removed without compatibility

We replaced the dual-runtime engine layer (whisper.cpp via `transcribe-rs`'s
`whisper-cpp` feature + ONNX Runtime via its `onnx` feature) with one runtime:
transcribe.cpp (`transcribe-cpp` crate, GGUF models on ggml). This is a clean
cut — no legacy model formats load, no old registry entries survive, and
leftover `.bin`/ONNX artifacts in the model directory are deleted automatically
at startup (logged; recoverable only by re-downloading).

## Why

- Upstream direction: Handy (the author's own app) has moved all new model
  downloads to transcribe.cpp; `transcribe-rs` is legacy-compat only there, and
  its `whisper-cpp` feature no longer has an upstream consumer. Staying meant
  betting on an abandoned path.
- New model families (Qwen3-ASR, FunASR, Canary-Qwen, streaming models) and the
  official Breeze-ASR-25 GGUF ship only for transcribe.cpp.
- One runtime removes the ONNX Runtime dependency from the Docker image and
  collapses two engine bridges and a seven-way `EngineType` dispatch into one
  bridge; GGUF is self-describing (arch/capabilities read from the file).
- Real streaming (feed/finalize) and inference cancellation, which the ONNX
  path could not provide.

## Consequences

- All model ids change; the HA integration's STT entities are rebuilt (the
  integration is updated in the same release, deployed together).
- Concurrency model: a loaded `Model` admits one inference at a time
  (per-model compute lock), so the engine pool holds N independently loaded
  `Model` instances. transcribe.cpp does **not** mmap GGUF weights — each
  instance is a full in-RAM copy, so pool_size directly multiplies memory
  (default stays 1).
- The `SpeechEngine` trait stays as a thin seam (mock tests, and a single file
  to absorb `transcribe-cpp` 0.x breaking changes).
- Compiled backends: CPU (tinyBLAS, `GGML_NATIVE=OFF`) by default; `cuda`
  remains an opt-in cargo feature. GPU-in-addon (Vulkan/iGPU) is deliberately
  out of scope until it can be verified on real hardware.
