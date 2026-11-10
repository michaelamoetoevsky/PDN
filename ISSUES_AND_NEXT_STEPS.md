# PDN (PRoot Distro Next) - Issues & Next Steps Documentation

## Current Status: Phase 5 Complete ✅

All 10 Phase 5 tasks completed:
- [x] Security hardening: seccomp, AppArmor, SELinux, and capabilities
- [x] Comprehensive test coverage, with 57 tests passing
- [x] Fuzzing integration with cargo-fuzz and 3 targets
- [x] Release automation with GitHub Actions and cargo-release
- [x] Migration guide from PRoot-distro
- [x] Android NDK cross-compilation, with aarch64 verified
- [x] Documentation website with mdBook and 12 chapters
- [x] Performance profiling with Criterion benchmarks
- [ ] Termux package build as an APK. The binary works and needs a termux-packages submission.
- [ ] Long-running stability tests - needs dedicated infra

---

## Known Issues for v0.1.0

### 1. Output Capture / Stdio Inheritance
**Severity**: Medium  
**Location**: `crates/proot-core/src/lib.rs` - `execute()` method  
**Description**: `podnext run <container> -- <cmd>` executes successfully with exit code 0, but stdout and stderr are not visible in the terminal.  
**Root Cause**: The `execute()` method in `ProotEngine` uses `std::process::Stdio::inherit()`. The PTY and session management in `proot-distro-core` can fail to connect container stdio to the host terminal.  
**Workaround**: Direct execution mode works: `podnext -r /path/to/rootfs -- echo "hello"`

### 2. Container Listing Default Behavior
**Severity**: Low  
**Location**: `crates/podnext/src/main.rs` - `list` command  
**Description**: `podnext list` shows an empty table by default. You must use `--all` to see stopped containers.  
**Expected**: Show all containers by default, like `docker ps -a`

### 3. Termux APK Package
**Severity**: Low  
**Status**: The binary installs and runs on the device. It needs a formal termux-packages submission.  
**Blocker**: Requires termux-packages repo PR with proper build.sh, checksums, dependencies

### 4. Exit Code Propagation
**Severity**: Low  
**Location**: `crates/proot-core/src/lib.rs` - `execute()` returns `Result<i32>`  
**Description**: Non-zero exit codes from container commands are returned as `Err("Command exited with code X")` instead of clean exit code propagation.

### 5. Container Metadata Persistence
**Severity**: Low  
**Location**: `crates/proot-distro-core/src/container.rs` - `load_containers()`  
**Description**: The code loads containers from `metadata.json`. But it does not fully restore some fields: bind_mounts, env_vars, and security config.

---

## Next Steps Priority

### P0 - Critical, Before v1.0
- [ ] Fix stdio capture in container execution
- [ ] Submit Termux package to termux-packages
- [ ] Fix `podnext list` default to show all containers

### P1 - High, v1.1
- [ ] Complete container metadata restoration
- [ ] Improve exit code handling
- [ ] Add `podnext logs` command for container output history
- [ ] Add `podnext exec` for running commands in running containers

### P2 - Medium, v1.2
- [ ] Long-running stability tests, with a 72h soak
- [ ] GPU acceleration testing on physical devices
- [ ] Multi-arch image build support
- [ ] CRI/containerd integration testing

### P3 - Future, v2.0+
- [ ] Service supervisor and daemon architecture. See below.
- [ ] Kubernetes CRI plugin
- [ ] Web UI dashboard
- [ ] Multi-node clustering

---

## Architecture Discussion: Independent Service Supervisor

### Proposal
Add a **self-contained service supervisor** to PDN with:
- **Service Supervisor**, like runit/s6/daemontools
- **Service Manager** for dependency resolution and ordering
- **D-Bus Broker** for inter-process communication
- **Socket Activation** for on-demand service start
- **Daemon Runtime**, independent of the container runtime
- **Independent Lifecycle** that survives container restarts

### Comparison: Current vs Proposed

| Aspect | Current PDN | With Service Supervisor |
|--------|-------------|------------------------|
| Container Runtime | Direct (PRoot) | Via supervisor |
| Service Management | Manual (`podnext run`) | Declarative (service files) |
| Dependency Ordering | None | Automatic (DAG) |
| IPC | None / Ad-hoc | D-Bus |
| Socket Activation | None | systemd-style |
| Auto-restart | Manual | Policy-based |
| Logging | Stdout/stderr | Structured (journald-style) |
| Resource Limits | cgroups v2 | cgroups + supervision |

### Benefits for Termux/Android
1. **Boot-time services**: Start containers at Termux boot with Termux:Boot
2. **Background daemons**: Run databases, web servers, agents persistently
3. **Inter-container communication**: D-Bus for service discovery
4. **Socket activation**: Start a container on an incoming connection to save resources
5. **Watchdog integration**: Auto-restart failed containers
6. **Log management**: Centralized, rotated, queryable logs
7. **Resource isolation**: Supervisor enforces limits independently

### Implementation Approach

```
PDN Architecture with Supervisor:
┌─────────────────────────────────────────────────────────────┐
│                    podnext CLI                               │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                  PDN Supervisor (pid 1 in namespace)        │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐           │
│  │ Service     │ │ Socket      │ │ D-Bus       │           │
│  │ Manager     │ │ Activator   │ │ Broker      │           │
│  └─────────────┘ └─────────────┘ └─────────────┘           │
└─────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
        ┌──────────┐    ┌──────────┐    ┌──────────┐
        │Container │    │Container │    │Container │
        │ Service  │    │ Service  │    │ Service  │
        │ (PRoot)  │    │ (PRoot)  │    │ (PRoot)  │
        └──────────┘    └──────────┘    └──────────┘
```

### Service Definition Format in TOML
```toml
# ~/.config/pn-next/services/web-server.toml
[service]
name = "nginx"
container = "web-ubuntu"
command = ["nginx", "-g", "daemon off;"]
restart = "always"
restart_delay = "5s"

[resources]
memory = "512M"
cpu = "1.0"

[network]
ports = ["80:80", "443:443"]
socket_activation = true

[dependencies]
after = ["network-online.target"]
requires = ["redis.service"]

[logging]
driver = "journald"
max_size = "10M"
max_files = 5
```

### Integration Points
1. **Termux:Boot** - Supervisor starts at boot
2. **Wake locks** - Keep supervisor alive during doze mode
3. **PRoot integration** - Supervisor launches containers via existing runtime
4. **cgroups v2** - Supervisor manages resource limits
5. **D-Bus** - Use the existing Termux D-Bus or an embedded broker such as dbus-broker

### Risks & Mitigations
| Risk | Mitigation |
|------|------------|
| Added complexity | Modular design; supervisor optional |
| Memory overhead | Minimal (Rust, ~5MB) |
| Android restrictions | Use Termux APIs; fallback to direct mode |
| D-Bus on Android | Use dbus-broker (no system bus needed) |

### Recommendation
**Add this work as Phase 6 for v2.0.** This is a significant architectural change. It transforms PDN from a "container CLI" into a "container platform with service orchestration." The change aligns with the gateway pattern already in pn-next. It enables production workloads on Android.

---

*Document created: 2026-10-30*  
*PDN Version: 0.1.0*  
*Status: Phase 5 Complete*
