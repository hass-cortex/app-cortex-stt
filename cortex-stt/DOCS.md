# Cortex STT

Multi-engine speech-to-text service for Home Assistant. Supports
[Whisper](https://github.com/openai/whisper) (multilingual),
[NVIDIA Parakeet](https://huggingface.co/nvidia/parakeet-tdt_ctc-1.1b)
(English low-latency) and
[SenseVoice](https://github.com/FunAudioLLM/SenseVoice) (Asian languages)
via a unified HTTP API, with a built-in admin UI for downloading and
managing models.

## Installation

Click the Home Assistant My button below to open the app on your Home Assistant instance.

[![Open this app in your Home Assistant instance.][app-badge]][app]

1. Click the "Install" button to install the app.
1. Start the "Cortex STT" app.
1. Check the logs of "Cortex STT" to see if everything went well.
1. Click on the "OPEN WEB UI" button to jump into Cortex STT.

## After Installing

### 1. Download a model

Open the **Cortex STT** panel from the sidebar → **Models** tab → pick a
model → **Download**. `SenseVoice (INT8)` is a good first choice. See
[MODELS.md](../MODELS.md) for the full catalog with sizes, supported
languages, and use-case notes.

### 2. Configure default model and lifecycle (Optional)

Switch to the **Settings** tab and configure:

- **Default Model** — select the model you just downloaded and click
  **Apply**. The companion integration uses this when no explicit model
  is specified.
- **Pre-load on startup** — check this so the default model is loaded
  into memory at server start, eliminating cold-start latency on the
  first request.
- **Keep models loaded (no auto-unload)** — leave this **checked**
  (the default). Models stay resident in RAM until manually unloaded or
  the addon restarts. Uncheck only if you are running multiple large
  models and need RAM back when idle.

### 3. Install the companion integration

[![Open your Home Assistant instance and add the Cortex STT integration via HACS.][hacs-badge]][hacs]

Press **Add**, then restart Home Assistant. (Requires
[HACS](https://hacs.xyz) installed.)

### 4. Pair with the running app

After restart, a **Cortex STT discovered** card appears under
**Settings → Devices & Services**. Click **Configure** on the card
(URL and API key are pre-filled), then **Submit**. Each downloaded model
becomes its own STT entity.

### 5. Assign to a voice pipeline

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

## Discovery

Cortex STT publishes itself to the Home Assistant Supervisor's
`/discovery` endpoint so the companion integration can pair with zero
manual configuration.

**What is announced.** Hostname, ingress port (`8769`), and a
system-managed API key (`home-assistant-discovery`). The integration
reads these to build the backend URL and authenticate — you never need
to copy the key yourself.

**When announcement happens.**

- **At app startup** — automatic, best-effort. If Supervisor is briefly
  unavailable the announce is logged as a warning and the app keeps
  serving requests; pairing simply requires a manual re-announce.
- **On demand** — open the admin UI → **Settings** → **Home Assistant**
  card → **Re-announce to Home Assistant**. Use this when:
  - You installed the integration **after** the app, so the original
    startup announce had no listener.
  - You dismissed the "Cortex STT discovered" card and want it back.
  - You changed `discovery_api_key` and need the integration to re-pair
    with the new value.

**Custom API keys.** The `discovery_api_key` addon option (see
**Configuration** above) lets you pin a specific key — useful if you've
already configured the integration with a known value and want to keep
it after a reinstall. Leave it empty to auto-generate.

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

**"Cortex STT discovered" card never appears.** The startup announce
runs once when the app first binds its HTTP listener — if the
integration was installed afterwards, Supervisor has no record. Open
the admin UI → **Settings** → **Home Assistant** card → **Re-announce
to Home Assistant** to trigger it again. See the **Discovery** section
above.

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

[app]: https://my.home-assistant.io/redirect/supervisor_addon/?addon=24127962_cortex_stt&repository_url=https%3A%2F%2Fgithub.com%2Fhass-cortex%2Frepository
[app-badge]: https://my.home-assistant.io/badges/supervisor_addon.svg
[hacs]: https://my.home-assistant.io/redirect/hacs_repository/?owner=hass-cortex&repository=cortex-stt&category=integration
[hacs-badge]: https://my.home-assistant.io/badges/hacs_repository.svg
[va]: https://my.home-assistant.io/redirect/voice_assistants/
[va-badge]: https://my.home-assistant.io/badges/voice_assistants.svg
