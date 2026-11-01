# PRoot Distro Next (PDN) - Next Generation Container Management

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-GPL--2.0--or--later-blue.svg)](LICENSE)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)](https://github.com/michaelamoetoevsky/PDN/actions)

## Overview

PRoot Distro Next (PDN) is a next-generation container management system built on top of **PRoot** and **PRoot-distro**, providing a modern, Rust-based architecture for managing containers on Termux Android 16+ and other Linux environments.

PDN unifies the PRoot ecosystem into a single cohesive codebase:
- **PRoot** (https://github.com/proot-me/proot) - The foundation: ptrace-based chroot alternative
- **PRoot-distro** (https://github.com/termux/proot-distro) - Termux's distro management wrapper
- **proot-compat** - Termux/Android compatibility layer (platform shim, path translation, seccomp bypass, stub FS, mount helper, cgroup v2)

### Key Features

- **Rust Implementation**: Memory-safe, performant, and maintainable
- **Modular Architecture**: Clean separation of concerns across 6 crates
- **PRoot Integration**: Seamless integration with PRoot for chroot-like functionality without root
- **Container Management**: Full container lifecycle management (create, start, stop, exec, logs)
- **Advanced Networking**: Bridge networks, port mapping, DNS, CNI plugin support
- **Storage Management**: Volumes, bind mounts, multiple drivers, backup/restore
- **Security First**: Seccomp profiles, capability dropping, AppArmor/SELinux/Landlock frameworks
- **Workflow Engine**: Declarative workflows with retries, conditions, parallel execution
- **Plugin System**: Extensible plugin architecture with dynamic loading
- **REST API**: Full REST API for container management with OpenAPI docs
- **Metrics & Monitoring**: Built-in metrics with Prometheus exporter support
- **Health Checks**: Comprehensive health checking (process, disk, memory, CPU, network)
- **Fail-Safe & Guardrails**: Circuit breakers, retries, fallback actions, container limits
- **Checkpoint/Restore**: CRIU integration for container checkpoint and restore
- **GPU Acceleration**: Vulkan/DRM GPU device pass-through
- **Multi-Arch Support**: OCI image manifest list handling
- **OCI Runtime Spec Compliance**: CRI gRPC with tonic

### Advanced CLI Features (podnext)

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

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        PN Next (pn-next)                     │
│  ┌─────────┐ ┌──────────┐ ┌────────┐ ┌────────┐ ┌────────┐  │
│  │ Runtime │ │Scheduler │ │Network │ │Storage │ │Security│  │
│  └────┬────┘ └────┬─────┘ └────┬───┘ └────┬───┘ └────┬───┘  │
│       │           │            │           │           │      │
│       ▼           ▼            ▼           ▼           ▼      │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │                    PN Gateway                            │  │
│  │  ┌──────────┐ ┌────────────┐ ┌─────────┐ ┌──────────┐  │  │
│  │  │Fail-Safe │ │ Guardrails │ │ Health  │ │ Metrics  │  │  │
│  │  └──────────┘ └────────────┘ └─────────┘ └──────────┘  │  │
│  └─────────────────────────────────────────────────────────┘  │
│       │           │            │           │           │      │
│       ▼           ▼            ▼           ▼           ▼      │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │                  PRoot Distro Core                       │  │
│  │  Containers │ Images │ Config │ Sessions │ Bindings    │  │
│  └─────────────────────────────────────────────────────────┘  │
│       │           │            │           │           │      │
│       ▼           ▼            ▼           ▼           ▼      │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │                    PRoot Core                             │  │
│  │  Ptrace │ Syscall │ Path │ VFS │ Seccomp │ Extensions  │  │
│  └─────────────────────────────────────────────────────────┘  │
│       │           │            │           │           │      │
│       ▼           ▼            ▼           ▼           ▼      │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │                   proot-compat                            │  │
│  │  Platform Shim │ Path Trans │ Seccomp │ Stub FS        │  │
│  │  Mount Helper │ Cgroup v2                                   │  │
│  └─────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Crates

| Crate | Description |
|-------|-------------|
| `proot-core` | Core PRoot implementation (ptrace, syscall, path, VFS) |
| `proot-distro-core` | Container management (images, containers, networks, storage, volumes) |
| `proot-compat` | Termux/Android compatibility layer |
| `pn-gateway` | Gateway layer with fail-safe, guardrails, health, metrics |
| `pn-next` | Advanced architecture (runtime, scheduler, network, storage, security, API, workflow, plugin, checkpoint/restore, GPU, CRI) |
| `podnext` | Command-line interface and daemon runtime (binaries: `podnext`, `podnext-daemon`) |

## Current Verification Status (v0.1.7 — 2026-11-01)

| Check / Metric | Verified State | Notes |
|---|---|---|
| **Workspace Version** | `0.1.7` | Centralized in root `Cargo.toml` across all 6 crates |
| **Release Binaries** | `podnext`, `podnext-daemon` | ELF 64-bit aarch64 (Android 24, NDK r30), `0.1.7` |
| **Workspace Tests** | `93 passed, 0 failed, 1 ignored` | `cargo test --workspace` |
| **CRI Tests** | `49 passed, 0 failed` | `cargo test --package pn-next --features cri` |
| **Code Formatting** | Clean | `cargo fmt --all -- --check` exit `0` |
| **Whitespace / Diff** | Clean | `git diff --check` exit `0` |
| **Daemon IPC** | Smoke verified | Unix socket `ping` -> `pong`, `list` -> `[]` |
| **Termux Packaging** | Aligned | `termux-package/recipe/build.sh` version `0.1.7` |

*Note on Implementation Scope*: CLI entrypoints and daemon supervisor are operational for baseline container/daemon workflows. Certain advanced subcommands (e.g. `image push/tag/inspect`, `network`, `volume`) and higher-tier orchestration features (clustering, GitOps sync) currently exist as compilation-verified or stub modules; see `PHASES_1_4_DOCUMENTATION.md` for the empirical breakdown.

## Getting Started

### Prerequisites

- Rust 1.70+
- Termux Android 16+ (for Android deployment)
- Linux kernel 5.10+ (for full feature support)

### Building

```bash
# Clone the repository
git clone https://github.com/michaelamoetoevsky/PDN
cd PDN

# Build all crates
cargo build --release

# Run tests
cargo test --workspace

# Run specific crate tests
cargo test --package proot-core
cargo test --package proot-distro-core
cargo test --package pn-next
```

### Installation

```bash
# Install binary (recommended)
cargo install --path crates/podnext

# Or copy binary directly
cp target/release/podnext ~/.local/bin/
```

## Usage

### Basic Commands (podnext)

```bash
# Search for images
podnext search ubuntu

# Install a container
podnext install ubuntu:22.04

# Run a command in container
podnext run ubuntu -- apt update && apt upgrade -y

# Start interactive shell
podnext login ubuntu

# List containers
podnext list

# List running sessions
podnext ps

# Stop container
podnext kill ubuntu

# Remove container
podnext remove ubuntu

# System info
podnext info
podnext version
```

### REST API

Start the API server:

```bash
podnext start --api
```

Then access the API at `http://localhost:8080/api/v1/`:

```bash
# List containers
curl http://localhost:8080/api/v1/containers

# Create container
curl -X POST http://localhost:8080/api/v1/containers \
  -H "Content-Type: application/json" \
  -d '{"name": "my-container", "image": "ubuntu:22.04"}'

# Get container details
curl http://localhost:8080/api/v1/containers/my-container

# Start container
curl -X POST http://localhost:8080/api/v1/containers/my-container/start
```

## Architecture Details

### Core Components

#### PRoot Core (`proot-core`)
- **Ptrace Controller**: Manages ptrace operations for syscall interception
- **Syscall Handler**: Handles 200+ syscalls with path translation
- **Path Translator**: Resolves symlink chains, translates guest/host paths
- **VFS Manager**: Virtual filesystem management
- **Seccomp Filter**: Android-compatible seccomp filtering
- **Extension System**: Pluggable extension architecture

#### PRoot Distro Core (`proot-distro-core`)
- **Container Manager**: Full container lifecycle
- **Image Manager**: Image pulling, verification, layer management
- **Config System**: Flexible configuration with validation
- **Session Management**: Track container sessions
- **Bindings**: Bind mount management
- **Image Management**: Pull, verify, layer extraction

#### Compatibility Layer (`proot-compat`)
- **Platform Shim**: Android → Linux personality
- **Path Translation**: Symlink chain resolution
- **Seccomp Bypass**: Android syscall allowlist
- **Stub Filesystem**: `/run/dbus`, `/tmp`, `/sys/fs/cgroup`, etc.
- **Mount Helper**: Essential filesystem mounts
- **Cgroup v2**: Resource control hierarchy

#### Gateway Layer (`pn-gateway`)
- **Fail-Safe Manager**: Retries, circuit breakers, fallback actions
- **Guardrails**: Container limits, resource limits, security policies
- **Health Monitoring**: Process, disk, memory, CPU, network checks
- **Metrics Collection**: Counters, gauges, histograms with exporters

#### Advanced Architecture (`pn-next`)
- **Runtime**: Container runtime management
- **Scheduler**: Priority-based task scheduling
- **Network Manager**: Bridge networks, IPAM, port mapping
- **Storage Manager**: Volumes, drivers, backup/restore
- **Security Manager**: Seccomp, capabilities, Landlock, policies
- **API Server**: REST API with OpenAPI documentation
- **Workflow Engine**: Declarative workflows with steps, conditions, retries
- **Plugin Manager**: Dynamic plugin loading with hooks

### CLI Reference

```bash
podnext [GLOBAL_OPTIONS] <COMMAND>

Commands:
  search                  Search Docker Hub for images
  install                 Install distribution from OCI image
  list                    List created containers
  run                     Run command in container
  login                   Start interactive shell in container
  ps                      List active sessions
  sessions                List all sessions
  kill                    Stop container
  remove                  Delete a container
  info                    Show system information
  version                 Print version
  start                   Start API server
  help                    Print help

Global Options:
  -c, --config <FILE>     Configuration file path
  -v, --verbose           Increase verbosity
  -h, --help              Print help
  -V, --version           Print version
  --output <FORMAT>       Output format (table, json, yaml)
  --profile <NAME>        Configuration profile to use
```

### Configuration

Configuration file: `~/.config/pn-next/config.toml`

```toml
[runtime]
max_containers = 100
auto_cleanup = true
cleanup_interval = 3600

[scheduler]
max_concurrent_tasks = 10
default_priority = "Normal"
task_timeout = 300

[network]
bridge_name = "pn0"
subnet = "10.0.0.0/24"
gateway = "10.0.0.1"
dns_servers = ["8.8.8.8", "1.1.1.1"]

[storage]
root_dir = "~/.pn-next"
default_driver = "local"
max_size = 53687091200  # 50 GB

[security]
default_seccomp_profile = "default"
no_new_privileges = true
read_only_rootfs = false
drop_capabilities = ["CAP_SYS_ADMIN", "CAP_SYS_RESOURCE"]

[api]
host = "0.0.0.0"
port = 8080
enable_cors = true
```

## Development

### Project Structure

```
PDN/
├── Cargo.toml                 # Workspace root
├── crates/
│   ├── proot-core/           # Core PRoot implementation
│   ├── proot-distro-core/    # Container management
│   ├── proot-compat/         # Compatibility layer
│   ├── pn-gateway/           # Gateway layer
│   ├── pn-next/              # Advanced architecture
│   └── podnext/              # CLI interface
├── docs/                      # Documentation
├── fuzz/                      # Fuzzing targets (cargo-fuzz)
├── tests/                     # Integration tests
└── .github/workflows/         # CI/CD
```

### Running Tests

```bash
# All tests
cargo test --workspace

# Unit tests
cargo test --lib

# Integration tests
cargo test --test integration

# Specific crate
cargo test --package proot-core
cargo test --package pn-next
```

### Code Style

```bash
# Format code
cargo fmt --all

# Lint
cargo clippy --all-targets --all-features -- -D warnings

# Check
cargo check --workspace
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
3. Run tests: `cargo test --workspace`
4. Format code: `cargo fmt --all`
5. Lint: `cargo clippy --all-targets --all-features -- -D warnings`
5. Submit a pull request

### Commit Convention

```
type(scope): description

Types: feat, fix, docs, test, refactor, perf, build, ci, chore
```

## License

GPL-2.0-or-later

## Acknowledgments

- [PRoot](https://github.com/proot-me/proot) - The foundation
- [Termux](https://termux.dev/) - Android terminal environment
- [Tokio](https://tokio.rs/) - Async runtime
- [Axum](https://github.com/tokio-rs/axum) - Web framework
- [Clap](https://github.com/clap-rs/clap) - CLI parser