#!/usr/bin/env bash
# Integration test: download models + transcribe test audio + verify output.
#
# Usage:
#   ./tests/integration/model_pipeline_test.sh
#
# Environment variables:
#   TEST_MODELS   Space-separated list of model IDs to test.
#                 Default: small/fast models under 200 MB.
#   TEST_AUDIO    Directory containing per-language WAV files (en.wav, zh.wav, ...).
#                 Default: data/test-audio
#   MODEL_DIR     Where models are stored.
#                 Default: data/models
#   ASR_CLI       Path to the asr-cli binary.
#                 Default: target/release/asr-cli (falls back to target/debug/asr-cli)
#
# Exit codes:
#   0  All tests passed.
#   1  One or more tests failed.

set -euo pipefail

# ── Colour helpers (no-op when not a tty) ────────────────────────────────────
if [ -t 1 ]; then
  RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; BOLD='\033[1m'; RESET='\033[0m'
else
  RED=''; GREEN=''; YELLOW=''; BOLD=''; RESET=''
fi

# ── Resolve project root (script lives in tests/integration/) ────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# ── Configuration ────────────────────────────────────────────────────────────
DEFAULT_FAST_MODELS="whisper-tiny-int8 moonshine-base sense-voice-int8 gigaam-v3-int8 canary-180m-flash"
TEST_MODELS="${TEST_MODELS:-$DEFAULT_FAST_MODELS}"
TEST_AUDIO="${TEST_AUDIO:-${PROJECT_ROOT}/data/test-audio}"
MODEL_DIR="${MODEL_DIR:-${PROJECT_ROOT}/data/models}"

# Locate asr-cli binary
if [ -n "${ASR_CLI:-}" ]; then
  : # user-specified
elif [ -x "${PROJECT_ROOT}/target/release/asr-cli" ]; then
  ASR_CLI="${PROJECT_ROOT}/target/release/asr-cli"
elif [ -x "${PROJECT_ROOT}/target/debug/asr-cli" ]; then
  ASR_CLI="${PROJECT_ROOT}/target/debug/asr-cli"
else
  ASR_CLI=""
fi

# ── Expected transcription fragments (lowercase, approximate match) ──────────
# Map: lang -> expected substring. Add more as test audio files are created.
declare -A EXPECTED_TEXT
EXPECTED_TEXT[zh]="打开"       # zh.wav says "打開大燈" (simplified: 打开大灯)
EXPECTED_TEXT[en]="light"      # en.wav says "turn on the living room light"

# Map: model -> primary test language (first supported language with a wav file)
model_test_lang() {
  local model_id="$1"
  # Query the CLI for the model's supported languages
  # shellcheck disable=SC2034
  local langs
  langs=$("${ASR_CLI}" --model-dir "${MODEL_DIR}" list 2>/dev/null \
    | grep "^${model_id} " | awk '{print $NF}' || true)

  # Fallback: try common languages in order
  for lang in en zh ja ko ru de es fr; do
    if [ -f "${TEST_AUDIO}/${lang}.wav" ]; then
      echo "${lang}"
      return
    fi
  done
  echo "en"
}

# ── Step 1: Build ────────────────────────────────────────────────────────────
echo -e "${BOLD}=== Step 1: Build ===${RESET}"

if [ -z "${ASR_CLI}" ] || [ "${REBUILD:-1}" = "1" ]; then
  echo "Building asr-cli with all engine features..."
  (cd "${PROJECT_ROOT}" && cargo build --features "whisper onnx" --bin asr-cli --release)
  ASR_CLI="${PROJECT_ROOT}/target/release/asr-cli"
  echo -e "${GREEN}Build succeeded.${RESET}"
else
  echo "Using existing binary: ${ASR_CLI}"
fi

mkdir -p "${MODEL_DIR}"

# ── Step 2: Verify URLs ─────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}=== Step 2: Verify URLs ===${RESET}"
if "${ASR_CLI}" --model-dir "${MODEL_DIR}" verify-urls; then
  echo -e "${GREEN}All URLs verified.${RESET}"
else
  echo -e "${YELLOW}Warning: some URLs could not be verified (non-fatal).${RESET}"
fi

# ── Step 3: Download + Transcribe each model ─────────────────────────────────
echo ""
echo -e "${BOLD}=== Step 3: Model Pipeline Tests ===${RESET}"

TOTAL=0
PASSED=0
FAILED=0
SKIPPED=0

# Result accumulators for summary table
declare -a RESULT_ROWS

for model_id in ${TEST_MODELS}; do
  TOTAL=$((TOTAL + 1))
  echo ""
  echo -e "${BOLD}--- ${model_id} ---${RESET}"

  # 3a. Download
  echo "  Downloading ${model_id}..."
  if ! "${ASR_CLI}" --model-dir "${MODEL_DIR}" download "${model_id}" 2>&1; then
    echo -e "  ${RED}FAIL: download failed${RESET}"
    RESULT_ROWS+=("${model_id}|FAIL (download)|---|---")
    FAILED=$((FAILED + 1))
    continue
  fi

  # 3b. Pick the best test audio file for this model
  # Try each language the model supports
  test_lang=""
  test_wav=""
  for lang in en zh ja ko ru de es fr; do
    if [ -f "${TEST_AUDIO}/${lang}.wav" ]; then
      test_lang="${lang}"
      test_wav="${TEST_AUDIO}/${lang}.wav"
      break
    fi
  done

  if [ -z "${test_wav}" ]; then
    echo -e "  ${YELLOW}SKIP: no test audio found in ${TEST_AUDIO}${RESET}"
    RESULT_ROWS+=("${model_id}|SKIP|---|no test audio")
    SKIPPED=$((SKIPPED + 1))
    continue
  fi

  # 3c. Transcribe
  echo "  Transcribing ${test_wav} (lang=${test_lang})..."
  transcript=""
  if output=$("${ASR_CLI}" --model-dir "${MODEL_DIR}" transcribe "${model_id}" "${test_wav}" -l "${test_lang}" 2>&1); then
    # Extract the Text: line from the output
    transcript=$(echo "${output}" | grep '│ Text:' | sed 's/.*│ Text: //' | xargs)
    echo "  Transcript: ${transcript}"
  else
    echo -e "  ${RED}FAIL: transcription error${RESET}"
    echo "  Output: ${output}"
    RESULT_ROWS+=("${model_id}|FAIL (transcribe)|${test_lang}|---")
    FAILED=$((FAILED + 1))
    continue
  fi

  # 3d. Approximate match against expected text
  expected="${EXPECTED_TEXT[${test_lang}]:-}"
  if [ -z "${expected}" ]; then
    echo -e "  ${YELLOW}No expected text for lang=${test_lang}, accepting any non-empty output${RESET}"
    if [ -n "${transcript}" ]; then
      echo -e "  ${GREEN}PASS (non-empty transcript)${RESET}"
      RESULT_ROWS+=("${model_id}|PASS|${test_lang}|${transcript}")
      PASSED=$((PASSED + 1))
    else
      echo -e "  ${RED}FAIL: empty transcript${RESET}"
      RESULT_ROWS+=("${model_id}|FAIL (empty)|${test_lang}|---")
      FAILED=$((FAILED + 1))
    fi
  else
    # Case-insensitive substring match
    transcript_lower=$(echo "${transcript}" | tr '[:upper:]' '[:lower:]')
    expected_lower=$(echo "${expected}" | tr '[:upper:]' '[:lower:]')
    if echo "${transcript_lower}" | grep -q "${expected_lower}"; then
      echo -e "  ${GREEN}PASS (contains '${expected}')${RESET}"
      RESULT_ROWS+=("${model_id}|PASS|${test_lang}|${transcript}")
      PASSED=$((PASSED + 1))
    else
      echo -e "  ${RED}FAIL: expected '${expected}' not found in '${transcript}'${RESET}"
      RESULT_ROWS+=("${model_id}|FAIL (mismatch)|${test_lang}|${transcript}")
      FAILED=$((FAILED + 1))
    fi
  fi
done

# ── Step 4: Summary Table ───────────────────────────────────────────────────
echo ""
echo -e "${BOLD}=== Summary ===${RESET}"
printf "%-25s %-18s %-6s %s\n" "Model" "Result" "Lang" "Transcript"
printf "%-25s %-18s %-6s %s\n" "-------------------------" "------------------" "------" "--------------------"
for row in "${RESULT_ROWS[@]}"; do
  IFS='|' read -r m_id m_result m_lang m_text <<< "${row}"
  # Truncate transcript for display
  if [ "${#m_text}" -gt 40 ]; then
    m_text="${m_text:0:37}..."
  fi
  printf "%-25s %-18s %-6s %s\n" "${m_id}" "${m_result}" "${m_lang}" "${m_text}"
done

echo ""
echo "Total: ${TOTAL}  Passed: ${PASSED}  Failed: ${FAILED}  Skipped: ${SKIPPED}"

if [ "${FAILED}" -gt 0 ]; then
  echo -e "${RED}RESULT: FAIL${RESET}"
  exit 1
else
  echo -e "${GREEN}RESULT: PASS${RESET}"
  exit 0
fi
