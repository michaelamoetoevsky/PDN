# PDN Deployment Guide

## Overview
PDN (PRoot Distro Next) is a next-generation container platform for Termux/Android that unifies PRoot, PRoot-distro, and proot-compat into a single Rust codebase.

## Prerequisites

### Host System (Build)
- Ubuntu 22.04+ / Debian 12+ / Fedora 38+
- Rust 1.75+ (stable)
- Android NDK r27c (for Android cross-compilation)
- Git
- CMake, pkg-config
- System dependencies: `libclang-dev libssl-dev libudev-dev libseccomp-dev libcap-dev`

### Target System (Termux/Android)
- Android 10+ (API 29+)
- Termux (latest from F-Droid or GitHub)
- PRoot (installed via Termux: `pkg install proot`)
- proot-distro (installed via Termux: `pkg install proot-distro`)
- aarch64 architecture (arm64)

## Building from Source

### 1. Clone Repository
```bash
git clone https://github.com/your-org/pdn.git
cd pdn
```

### 2. Build for Host (Linux x86_64)
```bash
# Install Rust if not present
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Install system dependencies (Ubuntu/Debian)
sudo apt-get update && sudo apt-get install -y \
    libclang-dev pkg-config libssl-dev libudev-dev \
    libseccomp-dev libcap-dev cmake

# Build workspace
cargo build --workspace --release

# Verify build
./target/release/pn-cli --version
./target/release/pn-cli system info
```

### 3. Cross-Compile for Android aarch64
```bash
# Install Android NDK
# Option A: Via Android SDK
sdkmanager "ndk;27.0.12077973"

# Option B: Direct download
wget https://dl.google.com/android/repository/android-ndk-r27c-linux.zip
unzip android-ndk-r27c-linux.zip
export ANDROID_NDK_HOME="$PWD/android-ndk-r27c"

# Add Rust target
rustup target add aarch64-linux-android

# Configure cargo for Android
mkdir -p .cargo
cat > .cargo/config.toml << 'EOF'
[target.aarch64-linux-android]
linker = "${ANDROID_NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android34-clang"
EOF

# Build for Android
export CC_aarch64_linux_android="${ANDROID_NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android34-clang"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="${ANDROID_NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android34-clang"
cargo build --workspace --release --target aarch64-linux-android

# Verify Android binary
file target/aarch64-linux-android/release/pn-cli
# Should show: ELF 64-bit LSB executable, ARM aarch64
```

## Installation on Termux

### Method 1: Pre-built Binary (Recommended)
```bash
# Download latest release
curl -L -o pn-cli https://github.com/your-org/pdn/releases/latest/download/pn-cli-android-aarch64
chmod +x pn-cli
mv pn-cli $PREFIX/bin/

# Verify
pn-cli --version
pn-cli system info
```

### Method 2: Build on Device (Termux)
```bash
# Install build dependencies
pkg install rust clang cmake libandroid-support libc++ openssl libseccomp

# Clone and build
git clone https://github.com/your-org/pdn.git
cd pdn
cargo build --workspace --release

# Install binary
cp target/release/pn-cli $PREFIX/bin/
```

### Method 3: Cargo Install (when published)
```bash
cargo install --locked pn-cli
```

## Configuration

### Default Configuration Location
```
$HOME/.config/pdn/config.toml
```

### Configuration File Structure
```toml
# PDN Configuration
[gateway]
name = "pdn-gateway"
version = "1.0.0"
data_dir = "/data/data/com.termux/files/home/.local/share/pdn"
log_level = "info"

[gateway.proot]
# PRoot-specific configuration
seccomp_enabled = true
seccomp_profile = "default"

[gateway.proot_distro]
# PRoot-distro compatibility
container_root = "/data/data/com.termux/files/usr/var/lib/proot-distro"
cache_dir = "/data/data/com.termux/files/usr/var/cache/proot-distro"

[gateway.compat]
# proot-compat settings
platform_shim_enabled = true
path_translation_enabled = true
seccomp_bypass_enabled = true
stub_fs_enabled = true
mount_helper_enabled = true
cgroup_v2_enabled = true

[gateway.fail_safe]
auto_recovery = true
max_retries = 3
retry_delay_ms = 1000
circuit_breaker_threshold = 5
circuit_breaker_timeout_sec = 30
health_check_interval_sec = 10

[gateway.guardrails]
resource_limits_enabled = true
syscall_filtering_enabled = true
path_containment_enabled = true
network_policy_enabled = true

[gateway.health]
enabled = true
interval_sec = 30
checks = ["gateway", "proot", "proot_distro", "resources", "network"]

[gateway.metrics]
enabled = true
export_interval_sec = 60
prometheus_port = 9090
```

## Running PDN

### 1. Initialize PDN
```bash
pn-cli init
```

This creates:
- Configuration directory: `~/.config/pdn/`
- Data directory: `~/.local/share/pdn/`
- Default networks: `bridge`, `host`, `none`
- Default storage pool: `default`

### 2. Start Gateway (Background Service)
```bash
# Start gateway daemon
pn-cli system start

# Check status
pn-cli system info

# View logs
pn-cli system logs -f
```

### 3. Container Operations

#### Create Container
```bash
# From image
pn-cli container create --name my-ubuntu --image ubuntu:22.04

# With custom resources
pn-cli container create \
    --name my-debian \
    --image debian:12 \
    --cpu-limit 2.0 \
    --memory-limit 2G \
    --disk-limit 10G
```

#### Start/Stop/Restart
```bash
pn-cli container start my-ubuntu
pn-cli container stop my-ubuntu
pn-cli container restart my-ubuntu
```

#### Execute Commands
```bash
# Interactive shell
pn-cli container exec my-ubuntu -- /bin/bash

# Single command
pn-cli container exec my-ubuntu -- apt update
```

#### View Logs
```bash
pn-cli container logs my-ubuntu -f
```

### 4. Image Management
```bash
# Pull image
pn-cli image pull ubuntu:22.04
pn-cli image pull debian:12

# List images
pn-cli image list

# Inspect image
pn-cli image inspect ubuntu:22.04

# Remove image
pn-cli image remove ubuntu:22.04
```

### 5. Network Management
```bash
# Create network
pn-cli network create --name mynet --driver bridge --subnet 10.0.1.0/24

# List networks
pn-cli network list

# Inspect network
pn-cli network inspect mynet

# Connect container
pn-cli network connect mynet my-ubuntu

# Disconnect container
pn-cli network disconnect mynet my-ubuntu

# Remove network
pn-cli network remove mynet
```

### 6. Storage Management
```bash
# Create storage pool
pn-cli storage create --name mypool --driver local --path /data/pdn/storage

# List storage
pn-cli storage list

# Inspect storage
pn-cli storage inspect mypool

# Remove storage
pn-cli storage remove mypool
```

### 7. Volume Management
```bash
# Create volume
pn-cli volume create --name mydata --storage mypool --size 5G

# List volumes
pn-cli volume list

# Inspect volume
pn-cli volume inspect mydata

# Mount volume to container
pn-cli container create --name my-ubuntu --image ubuntu:22.04 --volume mydata:/data

# Remove volume
pn-cli volume remove mydata
```

## Integration with proot-distro

PDN can use proot-distro as a backend for container management:

```bash
# List proot-distro containers
pn-cli container list --backend proot-distro

# Import existing proot-distro container
pn-cli container import --name imported-ubuntu --source proot-distro:ubuntu

# Export to proot-distro format
pn-cli container export --name my-ubuntu --format proot-distro
```

## System Diagnostics

```bash
# Full system diagnosis
pn-cli system diagnose

# Clean up unused resources
pn-cli system cleanup

# Prune everything (careful!)
pn-cli system prune --all

# View gateway metrics
curl http://localhost:9090/metrics
```

## Troubleshooting

### Common Issues

#### 1. "Permission denied" on container operations
```bash
# Ensure PRoot is installed
pkg install proot

# Check PRoot version
proot --version
```

#### 2. "Seccomp filter failed"
```bash
# Disable seccomp in config
# Edit ~/.config/pdn/config.toml:
[gateway.proot]
seccomp_enabled = false
```

#### 3. "Cannot allocate memory"
```bash
# Increase swap or reduce container memory limits
pn-cli container create --name test --image alpine --memory-limit 512M
```

#### 4. "Network not found"
```bash
# Recreate default networks
pn-cli network create --name bridge --driver bridge --subnet 172.17.0.0/16
pn-cli network create --name host --driver host
pn-cli network create --name none --driver null
```

### Debug Mode
```bash
# Enable debug logging
export RUST_LOG=debug
pn-cli system start --foreground

# Or for specific module
export RUST_LOG=pn_gateway=debug,pn_next=debug
pn-cli container start my-ubuntu
```

### Log Locations
```
Gateway logs: ~/.local/share/pdn/logs/gateway.log
Container logs: ~/.local/share/pdn/containers/<name>/logs/
Audit logs: ~/.local/share/pdn/audit.log
```

## Updating

### From Source
```bash
cd pdn
git pull
cargo build --workspace --release
cp target/release/pn-cli $PREFIX/bin/
```

### From Binary
```bash
curl -L -o pn-cli https://github.com/your-org/pdn/releases/latest/download/pn-cli-android-aarch64
chmod +x pn-cli
mv pn-cli $PREFIX/bin/
```

## Uninstallation

```bash
# Remove binary
rm $PREFIX/bin/pn-cli

# Remove configuration and data
rm -rf ~/.config/pdn
rm -rf ~/.local/share/pdn

# Remove proot-distro containers managed by PDN (optional)
# pn-cli container list --all | grep pdn | awk '{print $1}' | xargs pn-cli container remove
```

## Security Considerations

1. **Run as non-root**: PDN runs as the Termux user, not root
2. **Seccomp filtering**: Enabled by default for syscall filtering
3. **Path containment**: Containers restricted to designated paths
4. **Resource limits**: CPU, memory, disk, PIDs configurable per container
5. **Network policies**: Per-container network isolation
6. **Audit logging**: All operations logged for security review

## Performance Tuning

### For Low-End Devices
```toml
# ~/.config/pdn/config.toml
[gateway.proot]
seccomp_enabled = false  # Reduces overhead

[gateway.fail_safe]
health_check_interval_sec = 60  # Less frequent checks

[gateway.metrics]
enabled = false  # Disable metrics export
```

### For High-Performance
```toml
[gateway.proot]
seccomp_enabled = true

[gateway.fail_safe]
health_check_interval_sec = 10

[gateway.metrics]
enabled = true
export_interval_sec = 30
```

## Support

- **Issues**: GitHub Issues
- **Documentation**: https://github.com/your-org/pdn/wiki
- **Discussions**: GitHub Discussions

## License
Apache-2.0 / MIT dual license