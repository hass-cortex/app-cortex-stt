#!/usr/bin/with-contenv bashio
set -e

# Read log level from HA addon config
LOG_LEVEL="$(bashio::config 'log_level')"

# Map HA log levels to Rust RUST_LOG values
case "${LOG_LEVEL}" in
  trace)   export RUST_LOG="trace" ;;
  debug)   export RUST_LOG="debug" ;;
  info)    export RUST_LOG="info" ;;
  warning) export RUST_LOG="warn" ;;
  error)   export RUST_LOG="error" ;;
  fatal)   export RUST_LOG="error" ;;
  *)       export RUST_LOG="info" ;;
esac

# Data and model directories
# /data = addon persistent volume (database, audio)
# /share/cortex-stt/models = shared models (persist across rebuilds)
export DATA_DIR="/data"
export MODEL_DIR="/share/cortex-stt/models"
export STATIC_DIR="/app/web/dist"

mkdir -p "${MODEL_DIR}" "${DATA_DIR}/audio"

bashio::log.info "Starting Cortex STT Server"
bashio::log.info "Log level: ${RUST_LOG}"
bashio::log.info "Data directory: ${DATA_DIR}"
bashio::log.info "Model directory: ${MODEL_DIR}"

# Run as non-root user
chown -R cortex-stt:cortex-stt "${DATA_DIR}" 2>/dev/null || true
chown -R cortex-stt:cortex-stt "${MODEL_DIR}" 2>/dev/null || true

exec gosu cortex-stt /app/cortex-stt-server
