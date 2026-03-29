# Cortex STT Server

Multi-engine speech-to-text HTTP service powered by [transcribe-rs](https://github.com/thewh1teagle/transcribe-rs), supporting Whisper, Parakeet, SenseVoice, GigaAM, Moonshine, and Canary engines.

## Features

- **Multi-engine support** - Multiple STT engines via transcribe-rs, each with different strengths
- **Model pooling** - Concurrent transcription with configurable pool size per model
- **Lazy loading** - Models load on first use, with LRU eviction and idle unloading
- **HTTP API** - REST API for transcription with sync, SSE streaming, and async job modes
- **Web Admin UI** - Model management, engine control, transcription history, and API key management
- **GPU acceleration** - CUDA support for faster inference
- **Three deployment modes** - Bare metal, Docker, Proxmox LXC

## Supported Models

| Model | Engine | Size | Languages | Notes |
|-------|--------|------|-----------|-------|
| whisper-tiny-int8 | Whisper | 42 MB | 18 | Smallest, fastest CPU inference |
| whisper-small | Whisper | 466 MB | 18 | Good balance of accuracy and speed |
| whisper-medium-q4 | Whisper | 492 MB | 18 | Q4_1 quantized, good accuracy/size tradeoff |
| whisper-large-v3-turbo | Whisper | 1.6 GB | 18 | Distilled large-v3, best practical Whisper model |
| whisper-large-v3-q5 | Whisper | 1.1 GB | 18 | Segfaults — see Known Limitations |
| breeze-asr | Whisper | 1.1 GB | 18 | Segfaults — see Known Limitations |
| parakeet-v2-int8 | Parakeet | 473 MB | en | NVIDIA, fast English-only, requires AVX |
| parakeet-v3-int8 | Parakeet | 478 MB | 17 | NVIDIA, fast multilingual, requires AVX |
| sense-voice-int8 | SenseVoice | 160 MB | zh,en,ja,ko,yue | FunAudioLLM, CJK + Cantonese, requires AVX |
| gigaam-v3-int8 | GigaAM | 152 MB | ru,en | Sber, Russian + English, requires AVX |
| moonshine-base | Moonshine | 58 MB | en | Lightweight English, requires AVX |
| canary-180m-flash | Canary | 146 MB | en,de,es,fr | NVIDIA, small and fast, requires AVX |
| canary-1b-v2 | Canary | 692 MB | en,de,es,fr | NVIDIA, high accuracy multilingual, requires AVX |

Custom models (Whisper GGML `.bin` files and ONNX directories) can be added by placing them in the models directory.

## Quick Start

### Docker

```bash
docker run -d \
  --name cortex-stt-server \
  -p 10400:10400 \
  -v cortex-stt-data:/data \
  ghcr.io/hass-cortex/cortex-stt-server:latest-cpu
```

With CUDA GPU:

```bash
docker run -d \
  --name cortex-stt-server \
  --gpus all \
  -p 10400:10400 \
  -v cortex-stt-data:/data \
  ghcr.io/hass-cortex/cortex-stt-server:latest-cuda
```

### Docker Compose

```bash
# CPU
docker compose -f deploy/docker-compose.yml up -d

# GPU (CUDA)
docker compose -f deploy/docker-compose.yml -f deploy/docker-compose.cuda.yml up -d
```

### Bare Metal

Download the binary from [Releases](https://github.com/hass-cortex/cortex-stt-server/releases):

```bash
# Download and install
curl -fsSL -o /usr/local/bin/cortex-stt-server \
  https://github.com/hass-cortex/cortex-stt-server/releases/latest/download/cortex-stt-server-x86_64-unknown-linux-gnu
chmod +x /usr/local/bin/cortex-stt-server

# Run
cortex-stt-server --default-model whisper-small

# Or install as systemd service
sudo cp deploy/cortex-stt-server.service /etc/systemd/system/
sudo systemctl enable --now cortex-stt-server
```

### Proxmox LXC

```bash
curl -fsSL https://raw.githubusercontent.com/hass-cortex/cortex-stt-server/main/deploy/lxc/setup.sh | bash
```

See [deploy/lxc/gpu-passthrough.md](deploy/lxc/gpu-passthrough.md) for GPU passthrough instructions.

## Configuration

### CLI Arguments

| Argument | Env Var | Default | Description |
|----------|---------|---------|-------------|
| `--http-host` | `HTTP_HOST` | `0.0.0.0` | HTTP server bind address |
| `--http-port` | `HTTP_PORT` | `10400` | HTTP server port |
| `--data-dir` | `DATA_DIR` | `./data` | Data directory |
| `--default-model` | `DEFAULT_MODEL` | `whisper-small` | Default model |
| `--pool-size` | `POOL_SIZE` | `1` | Instances per model |
| `--max-loaded-models` | `MAX_LOADED_MODELS` | `3` | Max concurrent models in memory |
| `--idle-timeout` | `IDLE_TIMEOUT` | `300` | Seconds before idle model unload |
| `--gpu-mode` | `GPU_MODE` | `auto` | GPU mode: `auto`, `cuda`, `cpu` |

### Configuration Priority

```
CLI args > Environment variables > config.toml > config.json (runtime) > defaults
```

## Admin Web UI

Access the web admin interface at `http://<host>:10400`.

Features:
- **Dashboard** - Hardware info, pool status, queue depth, metrics
- **Models** - Browse, download, delete models with hardware compatibility indicators
- **Engine** - Default model, pool controls, load/unload
- **History** - Transcription records with audio playback and segment timeline
- **API Keys** - Generate and manage keys for HTTP API access
- **Settings** - Retention policy, CORS, rate limiting

## Known Limitations

### whisper-large-v3-q5 and breeze-asr (Q5 Quantized Large Models)

These two models use the full Whisper large architecture (32 text decoder layers) with Q5 quantization. They **segfault** during inference due to an upstream bug in [whisper.cpp](https://github.com/ggerganov/whisper.cpp) (via `whisper-rs-sys` 0.15.0). This is not a memory issue or a CUDA issue — it reproduces on CPU with ample RAM.

- **Affected models:** `whisper-large-v3-q5` (Q5_0), `breeze-asr` (Q5_K)
- **Root cause:** whisper.cpp crashes on the 32-layer text decoder in full large models with Q5 quantization
- **Workaround:** Use `whisper-large-v3-turbo` (distilled 4-layer model, works fine) or `whisper-medium-q4` (Q4_1, 24 layers)
- **Status:** Waiting for upstream fix in whisper.cpp / whisper-rs

### ONNX Models Require AVX

All ONNX-based models (Parakeet, SenseVoice, GigaAM, Moonshine, Canary) require AVX instruction set support. Older CPUs without AVX cannot run these models.

## License

[MIT](LICENSE)
