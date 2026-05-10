# Cortex STT

Multi-engine speech-to-text service for Home Assistant. Supports
[Whisper](https://github.com/openai/whisper) (multilingual),
[NVIDIA Parakeet](https://huggingface.co/nvidia/parakeet-tdt_ctc-1.1b)
(English low-latency) and
[SenseVoice](https://github.com/FunAudioLLM/SenseVoice) (Asian languages)
via a unified HTTP API, with a built-in admin UI for downloading and
managing models.

## Installation

Press **Install**, wait for the image to be pulled, then press **Start**.
The admin UI appears in the Home Assistant sidebar.

## After Installing

### 1. Download a model

Open the **Cortex STT** panel from the sidebar → **Models** tab → pick a
model → **Download**. `whisper-small` is a good first choice (≈480 MB,
multilingual, balanced speed/quality).

### 2. Install the companion integration

[![Open your Home Assistant instance and add the Cortex STT integration via HACS.][hacs-badge]][hacs]

Press **Add**, then restart Home Assistant. (Requires
[HACS](https://hacs.xyz) installed.)

### 3. Pair with the running app

After restart, a **Cortex STT discovered** card appears under
**Settings → Devices & Services**. Click **Configure** on the card
(URL and API key are pre-filled), then **Submit**. Each downloaded model
becomes its own STT entity.

### 4. Assign to a voice pipeline

[![Open your Home Assistant instance and manage your voice assistants.][va-badge]][va]

Pick (or create) a pipeline → **Speech-to-text** → select your Cortex STT
entity. Create one pipeline per model if you want to switch per language
or per use case.

## Configuration

| Option              | Default  | Description                                                                                                                                                       |
| ------------------- | -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `log_level`         | `info`   | One of `trace`, `debug`, `info`, `warning`, `error`, `fatal`.                                                                                                     |
| `discovery_api_key` | _(auto)_ | Leave empty and the app generates a key on first start. Override only if you've already configured the integration with a specific key and don't want to re-pair. |

The app exposes its HTTP API on port `8769`. Most runtime settings (GPU
mode, idle timeout, pre-load) live in the **admin UI → Settings**, not in
addon options.

## Troubleshooting

**The admin UI shows "Cannot connect" right after start.** First boot
initializes the database; wait ~10 seconds and reload.

**Model download is stuck or fails.** Check the addon **Logs** tab. Some
models are 1–2 GB — ensure enough disk and network bandwidth.

**Voice pipeline always returns "no speech".** The diagnostic sensors
created per model report the last audio duration. A zero-second result
usually means the wake-word/VAD is cutting audio too aggressively.

**Integration setup fails with `401 Unauthorized`.** The integration
relies on the auto-discovered API key. If you change `discovery_api_key`
manually after pairing, reload the integration.

**Wrong characters for Chinese device names?** Pair Cortex STT with
[STT Corrector](https://github.com/hass-cortex/stt-corrector) — it
wraps any STT entity with a language-normalization, custom-replacements,
and pinyin similarity-matching pipeline.

## Known Limitations

- Audio format is fixed at 16 kHz / 16-bit / mono PCM WAV. Home Assistant
  voice pipelines already produce this — only matters if you call the
  HTTP API directly.
- The model list seen by the integration is captured at setup time. New
  models downloaded server-side after pairing don't appear until the
  integration is reloaded.

## Support & Source

- Source: <https://github.com/hass-cortex/app-cortex-stt>
- Issues: <https://github.com/hass-cortex/app-cortex-stt/issues>
- Integration (HACS): <https://github.com/hass-cortex/cortex-stt>

[hacs]: https://my.home-assistant.io/redirect/hacs_repository/?owner=hass-cortex&repository=cortex-stt&category=integration
[hacs-badge]: https://my.home-assistant.io/badges/hacs_repository.svg
[va]: https://my.home-assistant.io/redirect/voice_assistants/
[va-badge]: https://my.home-assistant.io/badges/voice_assistants.svg
