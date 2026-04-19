# Cortex STT Server

Home Assistant app providing multi-engine speech-to-text with Whisper, Parakeet, SenseVoice, and more.

## Installation

- **Stable**: `https://github.com/hass-cortex/ha-apps`
- **Beta**: `https://github.com/hass-cortex/ha-apps-beta`

## Structure

- `cortex-stt/` — addon files (Rust source, Dockerfile, rootfs, web UI)
- `.github/workflows/` — CI/deploy using `hassio-addons/workflows`

## Acknowledgements

Built on top of [transcribe-rs](https://github.com/cjpais/transcribe-rs) by [@cjpais](https://github.com/cjpais) — a unified Rust library providing the multi-engine inference layer (Whisper, Parakeet, SenseVoice, Silero VAD). Many thanks for the excellent foundation.

## License

MIT — see [LICENSE.md](LICENSE.md).
