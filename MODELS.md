# Supported Models

Speech-to-text models that ship with Cortex STT. Open the **Models**
tab in the admin UI to download.

The source of truth is [`builtin_models()` in
`cortex-stt/src/engine/registry.rs`](cortex-stt/src/engine/registry.rs).
Update this file when adding entries there. Every value below is
mechanically derivable from registry fields:

- **Languages** — `supported_languages` (BCP-47 base codes).
- **Size** — `size_mb`, approximate; matches the downloaded artefact.
- **Requires** — `requires_avx`. ONNX engines need AVX; Whisper (ggml)
  works on any x86_64. CUDA is a build-time flag (`--features cuda`),
  not a per-model requirement.

## Catalog

| Engine     | ID                       | Name                        | Languages                                                              |   Size | Requires |
| ---------- | ------------------------ | --------------------------- | ---------------------------------------------------------------------- | -----: | -------- |
| Whisper    | `whisper-tiny-int8`      | Whisper Tiny (INT8)         | en, zh, ja, ko, de, es, fr, pt, ru, ar, hi, it, nl, pl, tr, vi, th, uk |  42 MB | —        |
| Whisper    | `whisper-small`          | Whisper Small               | en, zh, ja, ko, de, es, fr, pt, ru, ar, hi, it, nl, pl, tr, vi, th, uk | 466 MB | —        |
| Whisper    | `whisper-medium-q4`      | Whisper Medium (Q4)         | en, zh, ja, ko, de, es, fr, pt, ru, ar, hi, it, nl, pl, tr, vi, th, uk | 492 MB | —        |
| Whisper    | `whisper-large-v3-turbo` | Whisper Large V3 Turbo      | en, zh, ja, ko, de, es, fr, pt, ru, ar, hi, it, nl, pl, tr, vi, th, uk | 1.6 GB | —        |
| Whisper    | `whisper-large-v3-q5`    | Whisper Large V3 (Q5)       | en, zh, ja, ko, de, es, fr, pt, ru, ar, hi, it, nl, pl, tr, vi, th, uk | 1.1 GB | —        |
| Whisper    | `breeze-asr`             | Breeze ASR (Q5K)            | en, zh, ja, ko, de, es, fr, pt, ru, ar, hi, it, nl, pl, tr, vi, th, uk | 1.1 GB | —        |
| Parakeet   | `parakeet-v2-int8`       | Parakeet TDT 0.6B V2 (INT8) | en                                                                     | 473 MB | AVX      |
| Parakeet   | `parakeet-v3-int8`       | Parakeet TDT 0.6B V3 (INT8) | en, es, fr, de, it, pt, nl, pl, ru, uk, ja, ko, zh, hi, ar, he, tr     | 478 MB | AVX      |
| Moonshine  | `moonshine-base`         | Moonshine Base              | en                                                                     |  58 MB | AVX      |
| SenseVoice | `sense-voice-int8`       | SenseVoice (INT8)           | zh, en, ja, ko, yue                                                    | 160 MB | AVX      |
| SenseVoice | `funasr-nano-int8`       | Fun-ASR-Nano (INT8)         | zh, en, ja                                                             | 179 MB | AVX      |
| GigaAM     | `gigaam-v3-int8`         | GigaAM V3 (INT8)            | ru, en                                                                 | 152 MB | AVX      |
| Canary     | `canary-180m-flash`      | Canary 180M Flash           | en, de, es, fr                                                         | 146 MB | AVX      |
| Canary     | `canary-1b-v2`           | Canary 1B V2                | en, de, es, fr                                                         | 692 MB | AVX      |
| Cohere     | `cohere-int8`            | Cohere Transcribe 2B (INT8) | en, de, fr, es, it, pt, nl, pl, el, ar, vi, zh, ja, ko                 | 1.7 GB | AVX      |
| Cohere     | `cohere-int4`            | Cohere Transcribe 2B (INT4) | en, de, fr, es, it, pt, nl, pl, el, ar, vi, zh, ja, ko                 | 1.1 GB | AVX      |

Notes:

- `breeze-asr` is a Whisper checkpoint fine-tuned by MediaTek Research
  for Traditional Chinese.
- `funasr-nano-int8` runs in CTC-only mode (the SenseVoice family
  includes CTC and AED heads).
- Cohere Transcribe is listed on the
  [Open ASR Leaderboard](https://huggingface.co/spaces/hf-audio/open_asr_leaderboard).

## Hardware Notes

- ONNX engines (Parakeet, Moonshine, SenseVoice, Canary, GigaAM, Cohere)
  require **AVX** instructions. Whisper (ggml) does not.
- `--features cuda` build flag enables CUDA acceleration for both
  Whisper and ONNX engines (requires CUDA toolkit at compile time).
