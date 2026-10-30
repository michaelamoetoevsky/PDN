# PDN - PRoot Distro Next

**Next-generation container platform for Termux/Android**

PDN (PRoot Distro Next) is a unified Rust codebase that combines **PRoot**, **PRoot-distro**, and **proot-compat** into a single, cohesive container platform for Termux/Android. It provides a modern, safe, and performant alternative to traditional shell-based container management.

## Features

- **proot-core**: Core PRoot implementation with ptrace, syscall interception, and VFS
- **proot-distro-core**: Container lifecycle, image management, networking, and storage
- **proot-compat**: Termux/Android compatibility layer (platform shim, path translation, seccomp bypass, stub filesystem, mount helper, cgroup v2)
- **pn-gateway**: Unified gateway with fail-safe mechanisms, guardrails, health monitoring, and metrics
- **pn-next**: Advanced runtime, scheduler, security, API, workflow, and plugin system
- **podnext (CLI)**: Full-featured CLI for container/image/network/storage/volume management

### Advanced CLI Features

- **Native Docker Hub Integration**: Direct OCI image search/pull without shelling out
- **Multi-Format Output**: Table, JSON, YAML output formats
- **Tab Completion**: Shell completion for bash, zsh, fish
- **Parallel Operations**: Concurrent container operations where possible
- **Progress Indicators**: Real-time progress for image downloads and operations
- **Config Profiles**: Multiple configuration profiles for different environments
- **Remote Management**: Manage containers on remote hosts via SSH/TLS
- **Resource Templates**: Predefined resource limit templates (dev, prod, test)

### Mobility & Portability

- **Standalone Binary**: Single static binary with all dependencies (musl/glibc)
- **Android Cross-Compilation**: Native aarch64-linux-android target support
- **Termux Package Ready**: Designed for termux-packages repository integration
- **Offline Operation**: Full functionality without network after image install
- **Config Migration**: Automated migration from PRoot-distro configs
- **Rootless Execution**: No root privileges required (uses PRoot's ptrace)

## Quick Start

```bash
# Install from release
curl -L https://github.com/michaelamoetoevsky/PDN/releases/latest/download/podnext-aarch64-linux-android -o $PREFIX/bin/podnext
chmod +x $PREFIX/bin/podnext

# Or build from source
cargo install --path crates/podnext

# Search for images
podnext search ubuntu

# Install a container
podnext install ubuntu:22.04

# Run a command in the container
podnext run ubuntu -- apt update

# Start interactive shell
podnext login ubuntu
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        podnext (CLI)                         │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      pn-gateway                              │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐            │
│  │ Fail-Safe   │ │ Guardrails  │ │ Health/     │            │
│  │ Manager     │ │ Engine      │ │ Metrics     │            │
│  └─────────────┘ └─────────────┘ └─────────────┘            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                       pn-next                                │
│  ┌─────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐     │
│  │Runtime  │ │Scheduler │ │ Security │ │  Network/    │     │
│  │Manager  │ │          │ │ Manager  │ │  Storage     │     │
│  └─────────┘ └──────────┘ └──────────┘ └──────────────┘     │
│  ┌─────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐     │
│  │ Plugin  │ │ Checkpoint│ │  GPU    │ │   CRI gRPC   │     │
│  │ System  │ │ /Restore │ │ Support │ │              │     │
│  └─────────┘ └──────────┘ └──────────┘ └──────────────┘     │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                  proot-distro-core                           │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐    │
│  │Container │ │  Image   │ │ Network  │ │  Storage/    │    │
│  │ Manager  │ │ Manager  │ │ Manager  │ │  Volume Mgr  │    │
│  └──────────┘ └──────────┘ └──────────┘ └──────────────┘    │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     proot-core                               │
│  ┌─────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐     │
│  │ Ptrace  │ │ Syscall  │ │   VFS    │ │   Tracee     │     │
│  │ Controller│ │ Intercept│ │ Manager  │ │   Manager    │     │
│  └─────────┘ └──────────┘ └──────────┘ └──────────────┘     │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     proot-compat                             │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐    │
│  │ Platform │ │  Path    │ │ Seccomp  │ │   Stub FS    │    │
│  │   Shim   │ │Translation│ │  Bypass  │ │              │    │
│  └──────────┘ └──────────┘ └──────────┘ └──────────────┘    │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐    │
│  │  Mount   │ │  Cgroup  │ │CAP_SYS_  │ │   Security   │    │
│  │  Helper  │ │   v2     │ │ ADMIN    │ │  Hardening   │    │
│  └──────────┘ └──────────┘ └──────────┘ └──────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

## System Requirements

- **Android**: 10+ (API 29+)
- **Architecture**: aarch64 (ARM64)
- **Termux**: Latest version from F-Droid or GitHub releases
- **Storage**: ~500MB for binaries and containers
- **Memory**: 2GB+ recommended

## License

Licensed under GPL-2.0-or-later (compatible with PRoot's license).

## Links

- [GitHub Repository](https://github.com/michaelamoetoevsky/PDN)
- [Issue Tracker](https://github.com/michaelamoetoevsky/PDN/issues)
- [Releases](https://github.com/michaelamoetoevsky/PDN/releases)
- [Documentation](https://michaelamoetoevsky.github.io/PDN/)