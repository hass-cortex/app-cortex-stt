#!/usr/bin/env bash
# Download all registry models for integration testing.
set -euo pipefail
cd "$(dirname "$0")/.."

MODEL_DIR="${MODEL_DIR:-./data/models}"
mkdir -p "${MODEL_DIR}"

echo "=== Building asr-cli ==="
cargo build --features "whisper onnx" --bin asr-cli --release 2>&1 | tail -1
CLI="./target/release/asr-cli"

echo ""
echo "=== Downloading each model ==="

# Hardcoded list matching builtin_models() in registry.rs
MODELS=(
  whisper-tiny-int8
  whisper-small
  whisper-medium-q4
  whisper-large-v3-turbo
  whisper-large-v3-q5
  breeze-asr
  parakeet-v2-int8
  parakeet-v3-int8
  moonshine-base
  sense-voice-int8
  gigaam-v3-int8
  canary-180m-flash
  canary-1b-v2
)

for id in "${MODELS[@]}"; do
  echo "--- ${id} ---"
  ${CLI} --model-dir "${MODEL_DIR}" download "${id}" 2>&1 || echo "  FAILED: ${id}"
  echo ""
done

echo "=== Final Status ==="
${CLI} --model-dir "${MODEL_DIR}" list 2>/dev/null
