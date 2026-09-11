# Cortex STT

![Supports amd64 Architecture][amd64-shield] ![Supports aarch64 Architecture][aarch64-shield] ![Supports armv7 Architecture][armv7-shield]

Self-hosted speech-to-text for Home Assistant. Runs
[Whisper](https://github.com/openai/whisper), NVIDIA Parakeet, SenseVoice,
Qwen3-ASR and more — every family on one
[transcribe.cpp](https://github.com/handy-computer/transcribe.cpp) GGUF
runtime, on your own hardware.

Each model you download becomes its own STT entity, so a pipeline can pick the
right one per language or per use case. Audio streams over WebSocket while you
are still speaking, so decoding overlaps capture.

The admin UI downloads models, runs one recording through up to three of them
side by side, and scores candidates against transcripts you typed yourself —
so the choice of model is measured rather than guessed. See `DOCS.md`.

[amd64-shield]: https://img.shields.io/badge/amd64-yes-green.svg
[aarch64-shield]: https://img.shields.io/badge/aarch64-no-red.svg
[armv7-shield]: https://img.shields.io/badge/armv7-no-red.svg
