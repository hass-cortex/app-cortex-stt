# Test Audio Files

This directory is an alternative location for test audio used by `model_pipeline_test.sh`. By default, the script uses `data/test-audio/` at the project root.

## Adding Test Audio

Place WAV files named by language code (BCP-47 short form):

| File | Language | Suggested content |
|------|----------|-------------------|
| `en.wav` | English | "turn on the living room light" |
| `zh.wav` | Chinese | "打開大燈" |
| `ja.wav` | Japanese | "リビングの電気をつけて" |
| `ko.wav` | Korean | "거실 불 켜 줘" |
| `ru.wav` | Russian | "включи свет в гостиной" |
| `de.wav` | German | "schalte das Wohnzimmerlicht ein" |
| `es.wav` | Spanish | "enciende la luz del salon" |
| `fr.wav` | French | "allume la lumiere du salon" |

## Requirements

- Format: WAV (PCM)
- Sample rate: any (resampled to 16 kHz internally)
- Channels: mono preferred (stereo is downmixed)
- Duration: 1-5 seconds recommended
- Content: short voice commands work best for quick validation

## Generating with espeak

```bash
espeak-ng -v en -w en.wav "turn on the living room light"
espeak-ng -v cmn -w zh.wav "打開大燈"
espeak-ng -v ru -w ru.wav "включи свет в гостиной"
```

## Using with the test script

```bash
# Default (uses data/test-audio/)
./tests/integration/model_pipeline_test.sh

# Override audio directory
TEST_AUDIO=./tests/integration/test_audio ./tests/integration/model_pipeline_test.sh
```
