# Wyoming ASR

Multi-engine speech-to-text service with [Wyoming protocol](https://github.com/rhasspy/wyoming) for Home Assistant.

Powered by [transcribe-rs](https://github.com/thewh1teagle/transcribe-rs), supporting Whisper, Parakeet, SenseVoice, GigaAM, Moonshine, and Canary engines.

## Features

- **Multi-engine support** - Multiple STT engines via transcribe-rs, each with different strengths
- **Model pooling** - Concurrent transcription with configurable pool size per model
- **Lazy loading** - Models load on first use, with LRU eviction and idle unloading
- **Wyoming protocol** - Native integration with Home Assistant voice pipelines
- **HTTP API** - REST API for transcription with sync, SSE streaming, and async job modes
- **Web Admin UI** - Model management, engine control, transcription history, and API key management
- **GPU acceleration** - CUDA support for faster inference
- **Four deployment modes** - Bare metal, Docker, Home Assistant App, Proxmox LXC

## Supported Models

| Model | Engine | Size | Languages | Notes |
|-------|--------|------|-----------|-------|
| whisper-tiny-int8 | Whisper | ~40MB | 99 | Lightest, runs on Raspberry Pi |
| whisper-base-int8 | Whisper | ~75MB | 99 | Good balance for low-power devices |
| whisper-small | Whisper | ~460MB | 99 | Recommended starting point |
| whisper-medium | Whisper | ~1.5GB | 99 | GPU recommended |
| whisper-large-v3-turbo | Whisper | ~1.5GB | 99 | Best accuracy, GPU recommended |
| breeze-asr | Whisper | ~1.1GB | 99 | Optimized for zh-TW |
| parakeet-v2-int8 | Parakeet | ~250MB | en | Fast English-only |
| parakeet-v3-int8 | Parakeet | ~250MB | 17 | Fast multilingual |
| sense-voice-small | SenseVoice | ~450MB | zh,en,ja,ko,yue | CJK languages |
| gigaam-v2-rnnt | GigaAM | ~200MB | ru,en | Russian + English |
| moonshine-base | Moonshine | ~250MB | en | Lightweight English |

Custom models (Whisper GGML `.bin` files and ONNX directories) can be added by placing them in the models directory.

## Quick Start

### Home Assistant App

1. Add the repository to Home Assistant:
   **Settings > Apps > App Store > Repositories** and add:
   ```
   https://github.com/hass-cortex/wyoming-asr
   ```
2. Install **Wyoming ASR** from the app store
3. Configure the default model and start the app
4. Wyoming ASR will be auto-discovered by Home Assistant for use in voice pipelines

### Docker

```bash
docker run -d \
  --name wyoming-asr \
  -p 10300:10300 \
  -p 10400:10400 \
  -v wyoming-asr-data:/data \
  ghcr.io/hass-cortex/wyoming-asr:latest-cpu
```

With CUDA GPU:

```bash
docker run -d \
  --name wyoming-asr \
  --gpus all \
  -p 10300:10300 \
  -p 10400:10400 \
  -v wyoming-asr-data:/data \
  ghcr.io/hass-cortex/wyoming-asr:latest-cuda
```

### Docker Compose

```bash
# CPU
docker compose -f deploy/docker-compose.yml up -d

# GPU (CUDA)
docker compose -f deploy/docker-compose.yml -f deploy/docker-compose.cuda.yml up -d
```

### Bare Metal

Download the binary from [Releases](https://github.com/hass-cortex/wyoming-asr/releases):

```bash
# Download and install
curl -fsSL -o /usr/local/bin/wyoming-asr \
  https://github.com/hass-cortex/wyoming-asr/releases/latest/download/wyoming-asr-x86_64-unknown-linux-gnu
chmod +x /usr/local/bin/wyoming-asr

# Run
wyoming-asr --default-model whisper-small

# Or install as systemd service
sudo cp deploy/wyoming-asr.service /etc/systemd/system/
sudo systemctl enable --now wyoming-asr
```

### Proxmox LXC

```bash
curl -fsSL https://raw.githubusercontent.com/hass-cortex/wyoming-asr/main/deploy/lxc/setup.sh | bash
```

See [deploy/lxc/gpu-passthrough.md](deploy/lxc/gpu-passthrough.md) for GPU passthrough instructions.

## Configuration

### CLI Arguments

| Argument | Env Var | Default | Description |
|----------|---------|---------|-------------|
| `--wyoming-host` | `WYOMING_HOST` | `0.0.0.0` | Wyoming TCP bind address |
| `--wyoming-port` | `WYOMING_PORT` | `10300` | Wyoming TCP port |
| `--http-host` | `HTTP_HOST` | `0.0.0.0` | HTTP server bind address |
| `--http-port` | `HTTP_PORT` | `10400` | HTTP server port |
| `--data-dir` | `DATA_DIR` | `./data` | Data directory |
| `--default-model` | `DEFAULT_MODEL` | `whisper-small` | Default model for Wyoming |
| `--pool-size` | `POOL_SIZE` | `1` | Instances per model |
| `--max-loaded-models` | `MAX_LOADED_MODELS` | `3` | Max concurrent models in memory |
| `--idle-timeout` | `IDLE_TIMEOUT` | `300` | Seconds before idle model unload |
| `--gpu-mode` | `GPU_MODE` | `auto` | GPU mode: `auto`, `cuda`, `cpu` |
| `--addon` | `ADDON_MODE` | `false` | HA App mode |

### Configuration Priority

```
CLI args > Environment variables > config.toml > config.json (runtime) > defaults
```

## Admin Web UI

Access the web admin interface at `http://<host>:10400` (or via HA sidebar in app mode).

Features:
- **Dashboard** - Hardware info, pool status, queue depth, metrics
- **Models** - Browse, download, delete models with hardware compatibility indicators
- **Engine** - Default model, pool controls, load/unload
- **History** - Transcription records with audio playback and segment timeline
- **API Keys** - Generate and manage keys for HTTP API access
- **Settings** - Retention policy, CORS, rate limiting

## Home Assistant Integration

Wyoming ASR integrates with Home Assistant via the [Wyoming protocol](https://www.home-assistant.io/integrations/wyoming/). When running as an app, it is auto-discovered. For standalone deployments, add the Wyoming integration manually:

**Settings > Devices & Services > Add Integration > Wyoming Protocol** and enter:
- Host: `<wyoming-asr-ip>`
- Port: `10300`

## License

[MIT](LICENSE)
