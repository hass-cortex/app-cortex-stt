# Cortex STT Server

[![GitHub Release](https://img.shields.io/github/v/release/hass-cortex/app-cortex-stt)](https://github.com/hass-cortex/app-cortex-stt/releases)
[![HA Version](https://img.shields.io/badge/HA-2026.3.0+-green.svg)](https://www.home-assistant.io/)
[![GitHub License](https://img.shields.io/github/license/hass-cortex/app-cortex-stt)](https://github.com/hass-cortex/app-cortex-stt/blob/main/LICENSE)
[![DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/hass-cortex/app-cortex-stt)

Home Assistant app providing multi-engine speech-to-text with Whisper, Parakeet, SenseVoice, and more.

## Screenshots

**Models** — browse and download speech-to-text models.

![Models](images/models.png)

**History** — inspect transcription history with audio playback and per-segment timing.

![History](images/history.png)

## Installation

[![Open your Home Assistant instance and add this repository to the App Store.](https://my.home-assistant.io/badges/supervisor_add_addon_repository.svg)](https://my.home-assistant.io/redirect/supervisor_add_addon_repository/?repository_url=https%3A%2F%2Fgithub.com%2Fhass-cortex%2Frepository)

Click the button above, or manually: **Settings → Apps → App Store → ⋮ → Repositories → Add `https://github.com/hass-cortex/repository`**. After the repository loads, install **Cortex STT** from the list.

## Acknowledgements

Built on top of [transcribe-rs](https://github.com/cjpais/transcribe-rs) — a unified Rust library providing the multi-engine inference layer.

## License

MIT — see [LICENSE.md](LICENSE.md).
