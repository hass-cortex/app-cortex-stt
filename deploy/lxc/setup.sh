#!/usr/bin/env bash
# ==============================================================================
# Wyoming ASR - Proxmox LXC Installation Script
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/hass-cortex/wyoming-asr/main/deploy/lxc/setup.sh | bash
#
# Or with options:
#   bash setup.sh --version 0.1.0 --gpu
# ==============================================================================
set -euo pipefail

# Defaults
VERSION="${VERSION:-latest}"
INSTALL_DIR="/usr/local/bin"
DATA_DIR="/var/lib/wyoming-asr"
WEB_DIR="/usr/local/share/wyoming-asr/web"
CONFIG_DIR="/etc/wyoming-asr"
ENABLE_GPU=false
REPO="hass-cortex/wyoming-asr"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --version) VERSION="$2"; shift 2 ;;
        --gpu) ENABLE_GPU=true; shift ;;
        --data-dir) DATA_DIR="$2"; shift 2 ;;
        --help)
            echo "Usage: setup.sh [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --version VERSION  Version to install (default: latest)"
            echo "  --gpu              Install with CUDA GPU support"
            echo "  --data-dir PATH    Data directory (default: /var/lib/wyoming-asr)"
            echo "  --help             Show this help"
            exit 0
            ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

echo "========================================"
echo "  Wyoming ASR Installer"
echo "  Version: ${VERSION}"
echo "  GPU: ${ENABLE_GPU}"
echo "========================================"

# Resolve latest version
if [ "${VERSION}" = "latest" ]; then
    VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"v([^"]+)".*/\1/')
    echo "Resolved latest version: ${VERSION}"
fi

# Detect architecture
ARCH=$(uname -m)
case "${ARCH}" in
    x86_64) BINARY_ARCH="x86_64" ;;
    aarch64) BINARY_ARCH="aarch64" ;;
    *) echo "Unsupported architecture: ${ARCH}"; exit 1 ;;
esac

# Install runtime dependencies
echo "Installing dependencies..."
apt-get update
apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    libssl3 \
    libgomp1

# Download binary
BINARY_URL="https://github.com/${REPO}/releases/download/v${VERSION}/wyoming-asr-${BINARY_ARCH}-unknown-linux-gnu"
echo "Downloading binary from: ${BINARY_URL}"
curl -fsSL -o "${INSTALL_DIR}/wyoming-asr" "${BINARY_URL}"
chmod +x "${INSTALL_DIR}/wyoming-asr"

# Download and extract web UI
WEB_URL="https://github.com/${REPO}/releases/download/v${VERSION}/wyoming-asr-web.tar.gz"
echo "Downloading web UI from: ${WEB_URL}"
mkdir -p "${WEB_DIR}"
curl -fsSL "${WEB_URL}" | tar -xz -C "${WEB_DIR}"

# Create system user
if ! id -u wyoming-asr &>/dev/null; then
    useradd -r -s /bin/false -d "${DATA_DIR}" wyoming-asr
fi

# Create directories
mkdir -p "${DATA_DIR}/models" "${DATA_DIR}/audio" "${CONFIG_DIR}"
chown -R wyoming-asr:wyoming-asr "${DATA_DIR}"

# Create default config
if [ ! -f "${CONFIG_DIR}/config.toml" ]; then
    cat > "${CONFIG_DIR}/config.toml" <<'TOML'
# Wyoming ASR configuration
# See https://github.com/hass-cortex/wyoming-asr for documentation

[server]
wyoming_host = "0.0.0.0"
wyoming_port = 10300
http_host = "0.0.0.0"
http_port = 10400

[engine]
default_model = "whisper-small"
pool_size = 1
max_loaded_models = 3
idle_timeout_secs = 300
TOML
fi

# Install systemd service
UNIT_URL="https://raw.githubusercontent.com/${REPO}/v${VERSION}/deploy/wyoming-asr.service"
curl -fsSL -o /etc/systemd/system/wyoming-asr.service "${UNIT_URL}"
systemctl daemon-reload
systemctl enable wyoming-asr

# GPU setup hint
if [ "${ENABLE_GPU}" = true ]; then
    echo ""
    echo "GPU mode requested. Ensure NVIDIA drivers and CUDA are installed."
    echo "See: deploy/lxc/gpu-passthrough.md for Proxmox GPU passthrough setup."
    echo ""
    # Add GPU mode to config
    sed -i 's/^# \[gpu\]/[gpu]/' "${CONFIG_DIR}/config.toml" 2>/dev/null || true
fi

# Start service
systemctl start wyoming-asr

echo ""
echo "========================================"
echo "  Installation complete!"
echo ""
echo "  Binary:  ${INSTALL_DIR}/wyoming-asr"
echo "  Data:    ${DATA_DIR}"
echo "  Config:  ${CONFIG_DIR}/config.toml"
echo "  Web UI:  http://$(hostname -I | awk '{print $1}'):10400"
echo "  Wyoming: tcp://$(hostname -I | awk '{print $1}'):10300"
echo ""
echo "  Service: systemctl status wyoming-asr"
echo "  Logs:    journalctl -u wyoming-asr -f"
echo "========================================"
