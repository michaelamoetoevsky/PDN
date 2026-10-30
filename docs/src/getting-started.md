# Getting Started

## Installation

### From Release Binaries (Recommended)

```bash
# Download the latest release for Android aarch64
curl -L https://github.com/michaelamoetoevsky/PDN/releases/latest/download/podnext-aarch64-linux-android -o $PREFIX/bin/podnext
chmod +x $PREFIX/bin/podnext

# Verify installation
podnext --version
```

### From Source

```bash
# Prerequisites
pkg install rust git pkg-config libclang openssl

# Clone and build
git clone https://github.com/michaelamoetoevsky/PDN.git
cd PDN
cargo install --path crates/podnext

# Verify
podnext --version
```

### Android NDK Cross-Compilation

```bash
# Install NDK
pkg install android-ndk

# Build for Android
cargo build --release --target aarch64-linux-android
```

## Quick Start

### 1. Search for Container Images

```bash
# Search Docker Hub for Ubuntu images
podnext search ubuntu

# Search with limit
podnext search alpine --limit 5
```

### 2. Install a Container

```bash
# Install Ubuntu 22.04
podnext install ubuntu:22.04

# Install with custom name
podnext install alpine:latest --name my-alpine

# Install from OCI image reference
podnext install docker.io/library/debian:bookworm
```

### 3. Run Commands in Container

```bash
# Run single command
podnext run ubuntu -- apt update && apt upgrade -y

# Run with environment variables
podnext run ubuntu -e "DEBIAN_FRONTEND=noninteractive" -- apt install -y curl

# Run with bind mounts
podnext run ubuntu -b /sdcard:/mnt/sdcard -- ls /mnt/sdcard
```

### 4. Interactive Shell

```bash
# Start interactive shell (like proot-distro login)
podnext login ubuntu

# Start shell with custom command
podnext login ubuntu -- /bin/bash
```

### 5. Manage Containers

```bash
# List installed containers
podnext list

# List running sessions
podnext ps
podnext sessions

# Stop container
podnext kill ubuntu

# Remove container
podnext remove ubuntu
```

### 6. System Information

```bash
# Show system info
podnext info

# Show version
podnext version
```

## System Requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| Android Version | 10 (API 29) | 12+ (API 31+) |
| Architecture | aarch64 | aarch64 |
| RAM | 2 GB | 4 GB+ |
| Storage | 500 MB | 2 GB+ |
| Termux | Latest | Latest |

### Required Termux Packages

```bash
pkg install proot proot-distro qemu-user-static binfmt-support curl tar gzip xz-utils bzip2
```

### Kernel Requirements

- `CONFIG_CHECKPOINT_RESTORE=y` (for checkpoint/restore)
- `CONFIG_CGROUPS=y` and `CGROUP_V2` (for resource limits)
- `CONFIG_SECCOMP=y` (for seccomp profiles)
- `CONFIG_USER_NS=y` (for user namespace support)

## Configuration Files

PDN stores configuration in:

```
$HOME/.config/pn-next/
├── config.toml          # Main configuration
├── containers/          # Container definitions
├── images/              # Image cache
├── networks/            # Network configurations
├── storage/             # Storage pool definitions
└── security/            # Security profiles
```

## First Container Walkthrough

Let's create and use an Ubuntu container:

```bash
# 1. Search for Ubuntu
podnext search ubuntu

# 2. Install Ubuntu 22.04
podnext install ubuntu:22.04

# 3. Verify installation
podnext list

# 4. Run a command
podnext run ubuntu -- cat /etc/os-release

# 5. Start interactive session
podnext login ubuntu

# Inside the container:
# root@localhost:~# apt update && apt install -y neofetch
# root@localhost:~# neofetch

# 6. Exit and clean up
exit
podnext kill ubuntu
podnext remove ubuntu
```

## Next Steps

- [CLI Reference](cli-reference.md) - Complete command documentation
- [Configuration](configuration.md) - Advanced configuration options
- [Security](security.md) - Security hardening guide
- [Migration Guide](migration-guide.md) - Migrating from PRoot-distro