# Changelog — Cortex STT Server

> **Note:** This project was originally named "Wyoming ASR" and was renamed to "Cortex STT Server" in March 2026. Historical entries below reflect the original naming.

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - Unreleased

### Added

- Wyoming protocol TCP server with per-connection state machine
- Multi-engine support via transcribe-rs (Whisper, Parakeet, SenseVoice, GigaAM, Moonshine, Canary)
- Model pool with lazy loading, LRU eviction, and idle unloading
- Built-in model registry with 11 pre-configured models
- Custom model scanning (GGML `.bin` and ONNX directories)
- HTTP API with sync, SSE streaming, and async job transcription modes
- Web Admin UI (React) for model management, engine control, history, and API keys
- HA App with S6-overlay, Wyoming discovery, and Ingress support
- Docker images (CPU and CUDA variants)
- Docker Compose configurations
- systemd service unit
- Proxmox LXC installation script
- API key authentication for HTTP endpoints
- Transcription history with audio playback
- Audio resampling for non-16kHz input
- Healthcheck endpoints
- GitHub Actions CI/CD (lint, test, build, release)
