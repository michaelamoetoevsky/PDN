# Architecture

## Overview

PDN follows a layered architecture where each layer has a single responsibility and communicates with adjacent layers through well-defined interfaces.

```
┌────────────────────────────────────────────────────────────────────┐
│                            podnext (CLI)                            │
│                  User-facing command-line interface                 │
└────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────┐
│                         pn-gateway                                  │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐               │
│  │ Fail-Safe    │ │ Guardrails   │ │ Health &     │               │
│  │ Manager      │ │ Engine       │ │ Metrics      │               │
│  └──────────────┘ └──────────────┘ └──────────────┘               │
│  Unified orchestration, fault tolerance, policy enforcement        │
└────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────┐
│                          pn-next                                    │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐              │
│  │ Runtime  │ │Scheduler │ │ Security │ │ Network/ │              │
│  │ Manager  │ │          │ │ Manager  │ │ Storage  │              │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘              │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐              │
│  │ Plugin   │ │Checkpoint│ │ GPU      │ │ CRI      │              │
│  │ System   │ │/Restore  │ │ Support  │ │ gRPC     │              │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘              │
│  Advanced orchestration, scheduling, security, APIs                │
└────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────┐
│                       proot-distro-core                             │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐              │
│  │Container │ │  Image   │ │ Network  │ │ Storage/ │              │
│  │ Manager  │ │ Manager  │ │ Manager  │ │ Volume   │              │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘              │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐              │
│  │ Session  │ │   GPU    │ │Signature │ │ Locking  │              │
│  │ Manager  │ │ Manager  │ │ Manager  │ │ Manager  │              │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘              │
│  Container lifecycle, image management, networking, storage        │
└────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────┐
│                         proot-core                                  │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐              │
│  │ Ptrace   │ │ Syscall  │ │   VFS    │ │ Tracee   │              │
│  │Controller│ │ Intercept│ │ Manager  │ │ Manager  │              │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘              │
│  Core PRoot functionality: ptrace, syscalls, virtual filesystem    │
└────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────┐
│                       proot-compat                                  │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐              │
│  │ Platform │ │  Path    │ │ Seccomp  │ │ Stub FS  │              │
│  │   Shim   │ │Translation│ │ Bypass   │ │          │              │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘              │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐              │
│  │  Mount   │ │  Cgroup  │ │CAP_SYS_  │ │ Security │              │
│  │  Helper  │ │   v2     │ │ ADMIN    │ │Hardening │              │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘              │
│  Termux/Android compatibility layer                                │
└────────────────────────────────────────────────────────────────────┘
```

## Core Crates

### proot-core

**Location:** `crates/proot-core`

Core PRoot implementation providing:
- **Ptrace Controller**: Process tracing and syscall interception
- **Syscall Interception**: Handle guest syscalls, translate to host
- **VFS Manager**: Virtual filesystem overlay for path translation
- **Tracee Manager**: Manage traced processes lifecycle

Key types:
```rust
pub struct ProotEngine {
    config: ProotConfig,
    ptrace: PtraceController,
    vfs: VfsManager,
}

pub struct ProotConfig {
    pub command: Vec<String>,
    pub bind: Vec<String>,
    pub env: Vec<String>,
    pub rootfs: PathBuf,
    // ...
}
```

### proot-distro-core

**Location:** `crates/proot-distro-core`

Container management layer providing:
- **ContainerManager**: Create, start, stop, remove containers
- **ImageManager**: Pull, push, list, remove OCI images
- **NetworkManager**: CNI-based networking
- **StorageManager**: Volume and storage pool management
- **SessionManager**: Exec sessions and PTY handling
- **GPUManager**: GPU device detection and passthrough
- **SignatureManager**: Image signing/verification
- **LockingManager**: Distributed locking for multi-process safety

### proot-compat

**Location:** `crates/proot-compat`

Termux/Android compatibility providing:
- **Platform Shim**: Android personality, syscall translation
- **Path Translation**: Symlink chain resolution, bind mount handling
- **Seccomp Bypass**: Android-compatible seccomp filter
- **Stub Filesystem**: Create /run/dbus, /tmp, /sys/fs/cgroup, etc.
- **Mount Helper**: Essential filesystem mounts (/proc, /sys, /dev)
- **Cgroup v2**: Resource controller setup
- **CAP_SYS_ADMIN**: Detection and fallback handling
- **Security Hardening**: Seccomp profiles, AppArmor, SELinux, capabilities

### pn-gateway

**Location:** `crates/pn-gateway`

Gateway layer providing:
- **FailSafeManager**: Circuit breakers, retry policies, bulkheads
- **GuardrailsEngine**: Policy enforcement, resource quotas, rate limiting
- **HealthMonitor**: Liveness/readiness probes, dependency checks
- **MetricsCollector**: Prometheus metrics, structured logging

### pn-next

**Location:** `crates/pn-next`

Advanced orchestration providing:
- **RuntimeManager**: Sandbox creation, container lifecycle
- **Scheduler**: Pod scheduling, resource allocation
- **SecurityManager**: Security profiles, seccomp, capabilities
- **NetworkManager**: CNI integration, service mesh
- **StorageManager**: CSI integration, snapshot management
- **PluginManager**: Dynamic plugin loading (libloading)
- **WorkflowEngine**: DAG-based workflow execution
- **API Server**: gRPC + REST endpoints

### podnext (CLI)

**Location:** `crates/podnext`

Command-line interface providing:
- Container commands: install, list, login, run, ps, kill, remove
- Image commands: search, pull, push, list, remove
- Network commands: create, list, remove, inspect
- Storage commands: create, list, remove, inspect
- Volume commands: create, list, remove, inspect
- System commands: info, version, sessions, attach

## Gateway Layer

The gateway (`pn-gateway`) sits between the CLI and the runtime, providing:

### Fail-Safe Mechanisms

```rust
pub struct FailSafeConfig {
    pub circuit_breaker: CircuitBreakerConfig,
    pub retry_policy: RetryPolicyConfig,
    pub bulkhead: BulkheadConfig,
    pub timeout: TimeoutConfig,
}
```

### Guardrails

```rust
pub struct GuardrailsConfig {
    pub max_containers: usize,
    pub max_memory_per_container: ByteSize,
    pub max_cpu_per_container: f64,
    pub allowed_images: Vec<String>,
    pub blocked_syscalls: Vec<String>,
    pub required_security_profile: String,
}
```

### Health Monitoring

```rust
pub struct HealthCheck {
    pub name: String,
    pub check_type: HealthCheckType,
    pub interval: Duration,
    pub timeout: Duration,
    pub healthy_threshold: u32,
    pub unhealthy_threshold: u32,
}
```

## Data Flow

### Container Creation Flow

```
1. CLI: podnext install ubuntu:22.04
        │
        ▼
2. Gateway: Validate request, check guardrails
        │
        ▼
3. pn-next: RuntimeManager.create_container()
        │
        ▼
4. proot-distro-core: ImageManager.pull() → ContainerManager.create()
        │
        ▼
5. proot-compat: patch_rootfs() → Apply compatibility patches
        │
        ▼
6. proot-core: ProotEngine::new() → Ready for execution
```

### Command Execution Flow

```
1. CLI: podnext run ubuntu -- apt update
        │
        ▼
2. Gateway: Check health, apply rate limits
        │
        ▼
3. pn-next: RuntimeManager.exec()
        │
        ▼
4. proot-distro-core: SessionManager.create_session()
        │
        ▼
5. proot-compat: init_compat() → Setup environment
        │
        ▼
6. proot-core: ProotEngine::spawn() → Ptrace + execve
        │
        ▼
7. Guest process runs under PRoot
        │
        ▼
8. Results streamed back through gateway to CLI
```

## Key Design Principles

1. **Zero Guest Modification**: All compatibility handled host-side
2. **Memory Safety**: Rust ownership model prevents use-after-free, buffer overflows
3. **Fearless Concurrency**: Type system prevents data races in ptrace/seccomp handling
4. **Cargo Workspace**: Unified build, shared dependencies, atomic changes
5. **Gateway Pattern**: Single entry point for orchestration, observability, policy
6. **Layered Security**: Defense in depth (seccomp + capabilities + AppArmor + SELinux)
7. **Observability First**: Metrics, tracing, health checks built-in
8. **Extensibility**: Plugin system for runtime extensions