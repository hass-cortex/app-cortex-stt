# GPU Passthrough for Proxmox LXC

This guide covers passing an NVIDIA GPU through to a Proxmox LXC container for Wyoming ASR CUDA acceleration.

## Prerequisites

- Proxmox VE 8.0+
- NVIDIA GPU installed on the host
- NVIDIA drivers installed on the host

## Host Setup

### 1. Install NVIDIA drivers on the Proxmox host

```bash
apt update
apt install pve-headers-$(uname -r)
apt install nvidia-driver nvidia-smi
```

### 2. Verify GPU is detected

```bash
nvidia-smi
```

### 3. Note the device files

```bash
ls -la /dev/nvidia*
# Typical output:
# /dev/nvidia0
# /dev/nvidiactl
# /dev/nvidia-modeset
# /dev/nvidia-uvm
# /dev/nvidia-uvm-tools
```

## LXC Configuration

### 1. Edit the LXC config file

```bash
nano /etc/pve/lxc/<VMID>.conf
```

Add the following lines:

```
# GPU passthrough
lxc.cgroup2.devices.allow: c 195:* rwm
lxc.cgroup2.devices.allow: c 509:* rwm
lxc.mount.entry: /dev/nvidia0 dev/nvidia0 none bind,optional,create=file
lxc.mount.entry: /dev/nvidiactl dev/nvidiactl none bind,optional,create=file
lxc.mount.entry: /dev/nvidia-uvm dev/nvidia-uvm none bind,optional,create=file
lxc.mount.entry: /dev/nvidia-uvm-tools dev/nvidia-uvm-tools none bind,optional,create=file
```

> **Note:** The cgroup major numbers (195, 509) may differ. Check with:
> `ls -la /dev/nvidia* | awk '{print $5}'`

### 2. Set the LXC to privileged (or use device passthrough)

For unprivileged containers, you need to map the device UIDs. The simplest approach is to use a privileged container:

```
unprivileged: 0
```

### 3. Restart the container

```bash
pct stop <VMID>
pct start <VMID>
```

## Inside the LXC Container

### 1. Install NVIDIA userspace libraries

The driver version inside the container must match the host driver version exactly.

```bash
# Check host driver version
nvidia-smi --query-gpu=driver_version --format=csv,noheader

# Install matching version in container
DRIVER_VERSION=550.127.05  # Replace with your version
curl -fsSL "https://us.download.nvidia.com/tesla/${DRIVER_VERSION}/NVIDIA-Linux-x86_64-${DRIVER_VERSION}.run" -o nvidia-installer.run
chmod +x nvidia-installer.run
./nvidia-installer.run --no-kernel-modules --silent
rm nvidia-installer.run
```

### 2. Verify GPU access

```bash
nvidia-smi
```

### 3. Install Wyoming ASR with GPU support

```bash
bash setup.sh --gpu
```

### 4. Configure Wyoming ASR for CUDA

Edit `/etc/wyoming-asr/config.toml`:

```toml
[engine]
gpu_mode = "cuda"
```

Restart the service:

```bash
systemctl restart wyoming-asr
```

## Troubleshooting

| Issue | Solution |
|-------|----------|
| `nvidia-smi: command not found` | Install NVIDIA userspace libraries in container |
| `CUDA error: no CUDA-capable device` | Check LXC device passthrough config |
| `Driver version mismatch` | Ensure container driver version matches host exactly |
| `Permission denied on /dev/nvidia*` | Check cgroup device allow rules |

## Resources

- [Proxmox Wiki: GPU Passthrough](https://pve.proxmox.com/wiki/PCI_Passthrough)
- [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/)
