# Supported Models

Speech-to-text models that ship with Cortex STT. Open the **Models**
tab in the admin UI to download. `SenseVoice (INT8)` is a good first
choice for Chinese / mixed CJK + English; `Whisper Small` is the
recommended default for European languages.

The source of truth is [`builtin_models()` in
`src/engine/registry.rs`](cortex-stt/src/engine/registry.rs). Update
this file when adding entries there.

## Whisper (ggml)

Multilingual (en, zh, ja, ko, de, es, fr, pt, ru, ar, hi, it, nl, pl, tr, vi, th, uk).

| ID                        | Name                    |   Size | Notes                                                  |
| ------------------------- | ----------------------- | -----: | ------------------------------------------------------ |
| `whisper-tiny-int8`       | Whisper Tiny (INT8)     |  42 MB | Fastest; lower accuracy. Good for smoke tests.         |
| `whisper-small`           | Whisper Small           | 466 MB | Balanced. Recommended default for multilingual.        |
| `whisper-medium-q4`       | Whisper Medium (Q4)     | 492 MB | More accurate than Small; Q4_1 quantised.              |
| `whisper-large-v3-turbo`  | Whisper Large V3 Turbo  | 1.6 GB | Distilled large-v3, faster than Q5.                    |
| `whisper-large-v3-q5`     | Whisper Large V3 (Q5)   | 1.1 GB | Highest Whisper accuracy.                              |
| `breeze-asr`              | Breeze ASR (Q5K)        | 1.1 GB | Tuned for Traditional Chinese (zh-TW).                 |

## Parakeet (ONNX, NVIDIA)

Requires AVX.

| ID                  | Name                          | Languages | Size   | Notes                                                              |
| ------------------- | ----------------------------- | --------- | -----: | ------------------------------------------------------------------ |
| `parakeet-v2-int8`  | Parakeet TDT 0.6B V2 (INT8)   | en        | 473 MB | English only, high accuracy + low latency.                         |
| `parakeet-v3-int8`  | Parakeet TDT 0.6B V3 (INT8)   | 17 langs  | 478 MB | en, es, fr, de, it, pt, nl, pl, ru, uk, ja, ko, zh, hi, ar, he, tr |

## SenseVoice / Fun-ASR (ONNX, FunAudioLLM)

CJK + Cantonese focus. Requires AVX.

| ID                 | Name                | Languages           |   Size | Notes                                              |
| ------------------ | ------------------- | ------------------- | -----: | -------------------------------------------------- |
| `sense-voice-int8` | SenseVoice (INT8)   | zh, en, ja, ko, yue | 160 MB | Multilingual CJK + Cantonese.                      |
| `funasr-nano-int8` | Fun-ASR-Nano (INT8) | zh, en, ja          | 179 MB | CTC-only mode; slightly higher accuracy.           |

## Moonshine (ONNX)

| ID                | Name           |  Size | Notes                                                |
| ----------------- | -------------- | ----: | ---------------------------------------------------- |
| `moonshine-base`  | Moonshine Base | 58 MB | Lightweight English-only; smallest non-Whisper.      |

Requires AVX.

## GigaAM (ONNX, Sber)

| ID                | Name               | Languages |   Size | Notes               |
| ----------------- | ------------------ | --------- | -----: | ------------------- |
| `gigaam-v3-int8`  | GigaAM V3 (INT8)   | ru, en    | 152 MB | Russian + English.  |

Requires AVX.

## Canary (ONNX, NVIDIA)

Languages: en, de, es, fr. Requires AVX.

| ID                  | Name                |  Size  | Notes                            |
| ------------------- | ------------------- | -----: | -------------------------------- |
| `canary-180m-flash` | Canary 180M Flash   | 146 MB | Small + fast.                    |
| `canary-1b-v2`      | Canary 1B V2        | 692 MB | Larger, higher accuracy.         |

## Cohere Transcribe (ONNX)

#1 on Open ASR Leaderboard. Languages: en, de, fr, es, it, pt, nl, pl, el, ar, vi, zh, ja, ko. Requires AVX.

| ID            | Name                          |  Size  | Notes                                            |
| ------------- | ----------------------------- | -----: | ------------------------------------------------ |
| `cohere-int8` | Cohere Transcribe 2B (INT8)   | 1.7 GB | Highest accuracy in this catalog.                |
| `cohere-int4` | Cohere Transcribe 2B (INT4)   | 1.1 GB | Faster + smaller; slightly lower accuracy.       |

## Custom Models

Drop files into the model directory and click **Scan** in the admin UI:

- `.bin` files — detected as custom Whisper ggml models.
- Directories containing `model.onnx` — detected as custom Parakeet-style ONNX models.

## Hardware Notes

- All ONNX models (Parakeet, Moonshine, SenseVoice, Canary, GigaAM, Cohere) require **AVX** instructions.
- Whisper (ggml) works on any x86_64 CPU, including AVX-less hardware.
- `--features cuda` enables CUDA acceleration for Whisper and ONNX engines (requires CUDA toolkit at build time).
