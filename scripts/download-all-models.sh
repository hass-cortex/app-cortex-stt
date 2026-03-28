#!/usr/bin/env bash
# Download all registry models for integration testing.
# Run once, models are cached in data/models/.
set -euo pipefail

cd "$(dirname "$0")/.."
MODEL_DIR="${MODEL_DIR:-./data/models}"
mkdir -p "${MODEL_DIR}"

echo "=== Building asr-cli ==="
cargo build --features "whisper onnx" --bin asr-cli --release 2>&1 | tail -1
CLI="./target/release/asr-cli"

echo ""
echo "=== Downloading all models ==="
# Get list of model IDs from registry
ALL_IDS=$(${CLI} --model-dir "${MODEL_DIR}" list 2>/dev/null | awk 'NR>2 {print $1}')

for id in ${ALL_IDS}; do
  ${CLI} --model-dir "${MODEL_DIR}" download "${id}" 2>&1 | grep -v "^$" || true
done

echo ""
${CLI} --model-dir "${MODEL_DIR}" list 2>/dev/null
