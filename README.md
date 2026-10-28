# PRoot Distro Next (PDN) - Next Generation Container Management

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-GPL--2.0-or-later-blue.svg)](LICENSE)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)]()

## Overview

PRoot Distro Next (PDN) is a next-generation container management system built on top of PRoot, providing a modern, Rust-based architecture for managing containers on Termux Android 16+ and other Linux environments.

### Key Features

- **Rust Implementation**: Memory-safe, performant, and maintainable
- **Modular Architecture**: Clean separation of concerns across crates
- **PRoot Integration**: Seamless integration with PRoot for chroot-like functionality
- **Container Management**: Full container lifecycle management
- **Advanced Networking**: Bridge networks, port mapping, DNS
- **Storage Management**: Volumes, bind mounts, multiple drivers
- **Security First**: Seccomp, capabilities, AppArmor, SELinux, Landlock
- **Workflow Engine**: Declarative workflows with retries, conditions, parallel execution
- **Plugin System**: Extensible plugin architecture
- **REST API**: Full REST API for container management
- **Metrics & Monitoring**: Built-in metrics with Prometheus support
- **Health Checks**: Comprehensive health checking
- **Fail-Safe & Guardrails**: Circuit breakers, retries, guardrails

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
| `proot-distro-core` | Container management (images, containers, networks) |
| `proot-compat` | Termux/Android compatibility layer |
| `pn-gateway` | Gateway layer with fail-safe, guardrails, health, metrics |
| `pn-next` | Advanced architecture (runtime, scheduler, network, storage, security, API, workflow, plugin) |
| `pn-cli` | Command-line interface |

## Getting Started

### Prerequisites

- Rust 1.70+
- Termux Android 16+ (for Android deployment)
- Linux kernel 5.10+ (for full feature support)

### Building

```bash
# Clone the repository
git clone https://github.com/hermes-agent/PDN
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
# Install binary
cargo install --path crates/pn-cli

# Or copy binary
cp target/release/pn ~/.local/bin/
```

## Usage

### Basic Commands

```bash
# Initialize PN Next
pn init

# Start daemon
pn start

# Stop daemon
pn stop

# Container management
pn container create my-container --image ubuntu:22.04
pn container start my-container
pn container stop my-container
pn container logs my-container --follow
pn container exec my-container -- bash

# Image management
pn image pull ubuntu:22.04
pn image list
pn image remove ubuntu:22.04

# Network management
pn network create my-network --driver bridge --subnet 10.0.0.0/24
pn network connect my-network my-container

# Volume management
pn storage create my-volume
pn storage mount my-volume my-container /data

# Status and monitoring
pn status
pn logs my-container --follow
pn exec my-container -- bash -c "echo hello"
```

### REST API

Start the API server:

```bash
pn start --api
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
pn [GLOBAL_OPTIONS] <COMMAND>

Commands:
  init                    Initialize PN Next
  start                   Start PN Next daemon
  stop                    Stop PN Next daemon
  container               Container management
  image                   Image management
  network                 Network management
  storage                 Storage management
  status                  Show status
  logs                    Show container logs
  exec                    Execute command in container

Global Options:
  -c, --config <FILE>     Configuration file path
  -v, --verbose           Increase verbosity
  -h, --help              Print help
  -V, --version           Print version
```

### Configuration

Configuration file: `~/.pn-next/config.toml`

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
│   └── pn-cli/               # CLI interface
├── docs/                      # Documentation
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