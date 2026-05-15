# Supported Models

Speech-to-text models that ship with Cortex STT. Open the **Models**
tab in the admin UI to download.

The source of truth is [`builtin_models()` in
`cortex-stt/src/engine/registry.rs`](cortex-stt/src/engine/registry.rs).
Update this file when adding entries there. All values below are
mechanically derived from registry fields:

- **Languages** — `supported_languages` (BCP-47 base codes).
- **Size** — `size_mb` (approximate; matches the downloaded artefact).
- **Requires** — `requires_avx`. ONNX engines need AVX; Whisper (ggml)
  works on any x86_64.

## Whisper (ggml)

OpenAI Whisper, packaged as quantised `.bin` files for whisper.cpp.

| ID                       | Name                   | Languages                                                                | Size   | Requires |
| ------------------------ | ---------------------- | ------------------------------------------------------------------------ | -----: | -------- |
| `whisper-tiny-int8`      | Whisper Tiny (INT8)    | en, zh, ja, ko, de, es, fr, pt, ru, ar, hi, it, nl, pl, tr, vi, th, uk   |  42 MB | —        |
| `whisper-small`          | Whisper Small          | en, zh, ja, ko, de, es, fr, pt, ru, ar, hi, it, nl, pl, tr, vi, th, uk   | 466 MB | —        |
| `whisper-medium-q4`      | Whisper Medium (Q4)    | en, zh, ja, ko, de, es, fr, pt, ru, ar, hi, it, nl, pl, tr, vi, th, uk   | 492 MB | —        |
| `whisper-large-v3-turbo` | Whisper Large V3 Turbo | en, zh, ja, ko, de, es, fr, pt, ru, ar, hi, it, nl, pl, tr, vi, th, uk   | 1.6 GB | —        |
| `whisper-large-v3-q5`    | Whisper Large V3 (Q5)  | en, zh, ja, ko, de, es, fr, pt, ru, ar, hi, it, nl, pl, tr, vi, th, uk   | 1.1 GB | —        |
| `breeze-asr`             | Breeze ASR (Q5K)       | en, zh, ja, ko, de, es, fr, pt, ru, ar, hi, it, nl, pl, tr, vi, th, uk   | 1.1 GB | —        |

`breeze-asr` is a Whisper checkpoint fine-tuned by MediaTek Research
for Traditional Chinese.

## Parakeet (ONNX, NVIDIA)

NVIDIA Parakeet TDT, packaged as ONNX directories.

| ID                 | Name                         | Languages                                                              | Size   | Requires |
| ------------------ | ---------------------------- | ---------------------------------------------------------------------- | -----: | -------- |
| `parakeet-v2-int8` | Parakeet TDT 0.6B V2 (INT8)  | en                                                                     | 473 MB | AVX      |
| `parakeet-v3-int8` | Parakeet TDT 0.6B V3 (INT8)  | en, es, fr, de, it, pt, nl, pl, ru, uk, ja, ko, zh, hi, ar, he, tr     | 478 MB | AVX      |

## SenseVoice / Fun-ASR (ONNX, FunAudioLLM)

FunAudioLLM SenseVoice family.

| ID                 | Name                | Languages           | Size   | Requires |
| ------------------ | ------------------- | ------------------- | -----: | -------- |
| `sense-voice-int8` | SenseVoice (INT8)   | zh, en, ja, ko, yue | 160 MB | AVX      |
| `funasr-nano-int8` | Fun-ASR-Nano (INT8) | zh, en, ja          | 179 MB | AVX      |

`funasr-nano-int8` runs in CTC-only mode (the SenseVoice family
includes CTC and AED heads).

## Moonshine (ONNX)

Useful Sensors Moonshine, packaged as ONNX.

| ID               | Name           | Languages | Size  | Requires |
| ---------------- | -------------- | --------- | ----: | -------- |
| `moonshine-base` | Moonshine Base | en        | 58 MB | AVX      |

## GigaAM (ONNX, Sber)

Sber GigaAM acoustic model, packaged as ONNX.

| ID               | Name             | Languages | Size   | Requires |
| ---------------- | ---------------- | --------- | -----: | -------- |
| `gigaam-v3-int8` | GigaAM V3 (INT8) | ru, en    | 152 MB | AVX      |

## Canary (ONNX, NVIDIA)

NVIDIA Canary, packaged as ONNX.

| ID                  | Name              | Languages      | Size   | Requires |
| ------------------- | ----------------- | -------------- | -----: | -------- |
| `canary-180m-flash` | Canary 180M Flash | en, de, es, fr | 146 MB | AVX      |
| `canary-1b-v2`      | Canary 1B V2      | en, de, es, fr | 692 MB | AVX      |

## Cohere Transcribe (ONNX)

Cohere Transcribe 2B, packaged as ONNX. Listed on the
[Open ASR Leaderboard](https://huggingface.co/spaces/hf-audio/open_asr_leaderboard).

| ID            | Name                          | Languages                                                | Size   | Requires |
| ------------- | ----------------------------- | -------------------------------------------------------- | -----: | -------- |
| `cohere-int8` | Cohere Transcribe 2B (INT8)   | en, de, fr, es, it, pt, nl, pl, el, ar, vi, zh, ja, ko   | 1.7 GB | AVX      |
| `cohere-int4` | Cohere Transcribe 2B (INT4)   | en, de, fr, es, it, pt, nl, pl, el, ar, vi, zh, ja, ko   | 1.1 GB | AVX      |

## Custom Models

Drop files into the model directory and click **Scan** in the admin UI:

- `.bin` files — detected as custom Whisper ggml models.
- Directories containing `model.onnx` — detected as custom Parakeet-style ONNX models.

## Hardware Notes

- ONNX engines (Parakeet, Moonshine, SenseVoice, Canary, GigaAM, Cohere)
  require **AVX** instructions. Whisper (ggml) does not.
- `--features cuda` build flag enables CUDA acceleration for both
  Whisper and ONNX engines (requires CUDA toolkit at compile time).
