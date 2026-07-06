# Cortex STT Server

[![GitHub Release](https://img.shields.io/github/v/release/hass-cortex/app-cortex-stt)](https://github.com/hass-cortex/app-cortex-stt/releases)
[![HA Version](https://img.shields.io/badge/HA-2026.3.0+-green.svg)](https://www.home-assistant.io/)
[![GitHub License](https://img.shields.io/github/license/hass-cortex/app-cortex-stt)](https://github.com/hass-cortex/app-cortex-stt/blob/main/LICENSE)
[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/hass-cortex/app-cortex-stt)

Home Assistant app providing multi-engine speech-to-text with Whisper, Parakeet, SenseVoice, and more. See [MODELS.md](MODELS.md) for the full list of built-in models.

## Screenshots

**Models** — browse and download speech-to-text models.

![Models](images/models.png)

**History** — inspect transcription history with audio playback and per-segment timing.

![History](images/history.png)

## Installation

See [cortex-stt/DOCS.md](cortex-stt/DOCS.md) for full install, configuration, discovery, and troubleshooting instructions.

> **Heads-up for Proxmox VE / KVM users.** The pre-built binary's
> bundled ONNX Runtime requires **AVX + AVX2 + FMA + F16C + BMI2 +
> SSE 4.2** (Intel Haswell 2013+ / `x86-64-v3`). PVE's default
> `qemu64` / `kvm64` / `x86-64-v2-AES` CPU types mask AVX/AVX2/FMA
> from the guest — change the HAOS VM's CPU **Type** to `host` (or
> `x86-64-v3`) and **cold-boot** the VM (reboot is not enough). The
> addon's init oneshot detects the missing flags and prints a
> readable diagnostic instead of crash-looping. Full steps in
> [DOCS.md](cortex-stt/DOCS.md#system-requirements).

## Acknowledgements

- [transcribe-rs](https://github.com/cjpais/transcribe-rs) — the unified Rust library providing the multi-engine inference layer.
- [handy](https://github.com/cjpais/handy) — the desktop dictation app whose engine bridges + model catalog this project is built on.

## License

MIT — see [LICENSE.md](LICENSE.md).
