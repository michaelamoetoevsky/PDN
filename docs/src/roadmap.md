# Comprehensive Architectural Analysis & Development Roadmap for PDN (PRoot Distro Next)

**Date:** November 2026
**Baseline Status:** v0.1.7, stable master commit `c00809c`, 100% Green CI/CD
**Target Platform:** Termux on Android 16+ for aarch64, and Rootless Linux Environments for x86_64 and aarch64
**Related Documents:** `research/INTERNAL_CAPABILITY_TESTS_RUN2.md`, `research/DOCKER_RUNTIME_INTEGRATION_PLAN.md`, `docs/src/migration-guide.md`

---

## 1. Executive Summary & Status Quo

### 1.1 Validated Foundations
PDN has evolved from an exploratory Rust port into a production-grade container management and service orchestration platform:
1. **CI/CD Pipeline Integrity**: 15 GitHub Actions workflow jobs are **100% GREEN**. The jobs are Host Build, 133 Unit/Integration Tests, a Cross-Compile Matrix, Docs mdBook, Fuzzing, and Security Audit. The Cross-Compile Matrix covers aarch64/x86_64 across NDK r27c & r30.
2. **Defect Remediation & Security Hardening**:
   - *Child Process Exit Code*: The code propagates child exit codes accurately through `proot_core::ChildExit` downcasting. It forwards exit statuses 0..127 directly and does not collapse them to 1.
   - *Guest Path Traversal*: PDN closed the traversal vulnerability in `copy` subcommands. The fix uses strict token validation, refuses `..`, and enforces the canonical rootfs boundary.
   - *SELinux Hardlink Safe Unpacking*: Automated fallback to file copies when `hard_link` is denied by Android SELinux policies during OCI layer extraction.
   - *Shared Memory `/dev/shm` 1777*: Standalone POSIX shared memory store, enabling Chromium Headless and Playwright DOM extraction without crashes.
   - *Environment Variable Sanitization*: The guest environment is clean and isolated. Only 6 essential variables pass through, which eliminates the leakage of 163 host Termux variables.
   - *Dynamic Distribution Identification*: Real-time distribution identification parsed from guest `/etc/os-release` or `/usr/lib/os-release` for accurate `podnext list` reporting.
3. **Full Parity with proot-distro**:
   - `install`, `login`, `run`, `list`, `remove`, `rename`, `copy`, `sync` with sha256 and `--delete`, `reset`, `search`, and `info`.

### 1.2 Fundamental Android Kernel Constraints: Empirical Ground Truth
In-depth empirical testing documented in `research/INTERNAL_CAPABILITY_TESTS_RUN2.md` confirms the hard boundaries of pure PRoot on Android:
- Android Bionic's seccomp filter intercepts `mount(2)` and terminates it with `SIGSYS`, signal 31.
- `unshare(CLONE_NEWUSER)` returns `EINVAL`, because Google's Android GKI kernel disables unprivileged user namespaces by default.
- `unshare(CLONE_NEWPID | CLONE_NEWNET)` returns `EPERM`, because unprivileged process capabilities are in force with `CapEff=0`.
- `/sys/fs/cgroup` is mounted read-only for unprivileged Termux users.

**Logical Deduction:** A genuine Docker daemon, `dockerd`, cannot run directly inside pure PRoot without a real Linux kernel. The required architectural solution is **Runtime Backend Modularization**.

---

## 2. Forensic Source Code Comparative Mapping: PRoot C Engine & PRoot-distro v5.10.3

The goal is one single pass: the PDN architectural blueprint must be complete and executable in one pass. The project then ran forensic code audits on the upstream PRoot C codebase `source/proot-src` and the Termux `proot_distro` Python framework v5.10.3.

### 2.1 Anatomy & Internal Mechanics of the PRoot C Engine (`proot-src`)

| PRoot C Module | Source File | Internal Logic & Operational Mechanics | Architectural Implication for PDN |
|---|---|---|---|
| **Ptrace Event Loop & Signals** | `src/tracee/event.c`, `src/ptrace/wait.c` | PRoot installs `SIG_IGN` for almost all signals (`SIGINT`, `SIGTERM`, `SIGHUP`). Only select signals (`SIGQUIT`, `SIGSEGV`, `SIGABRT`) trigger `kill_all_tracees2()`. PRoot does not set `PTRACE_O_EXITKILL`. | **Root Cause of Zombie Processes**: Sending `kill -9` to PRoot leaves guest tracee processes orphaned and reparented to init. Controlled teardown requires signal escalation: sending `SIGQUIT` first to let PRoot terminate its children cleanly. |
| **VFS Syscall Interception** | `src/syscall/enter.c`, `src/syscall/exit.c` | `enter.c` (94KB) intercepts path-based syscalls (`openat`, `stat`, `execve`, `chdir`) and translates guest absolute paths to host rootfs paths. `exit.c` (23KB) patches return values (`uid=0`, `gid=0`, symlink sizes, and simulated mountinfo). | PDN Rust (`proot-core`) must maintain exact register alignment (`orig_x0` / `x8` on aarch64) and manipulate tracee memory via `process_vm_readv/writev`. |
| **Link2Symlink Extension** | `src/extension/link2symlink/` | Bypasses Android SELinux restrictions that prohibit `link(2)` on `/data`. Hard links are replaced by intermediate files prefixed with `.proot.l2s.` or `.l2s.`, while `stat` syscalls are patched to simulate genuine link counts (`st_nlink > 1`) and identical inodes. | When a container is renamed or copied, l2s symlinks containing absolute paths to the old rootfs break unless rewritten in-place. |
| **In-Memory SysV IPC** | `src/extension/sysvipc/` | Simulates System V IPC tables (`shmget`, `semget`, `msgget`) entirely in user-space memory inside the PRoot supervisor process. | Required by legacy software and enterprise databases that rely on SysV IPC rather than POSIX shared memory. |
| **Ashmem / Memfd** | `src/extension/ashmem_memfd/` | Emulates `memfd_create` using `/dev/ashmem` or anonymous temporary files when running on older kernels without native `memfd_create` support. | Prevents browser engines and modern runtimes (V8/Chromium/Electron) from crashing on Android. |
| **Kernel Version Spoofing (`kompat`)** | `src/extension/kompat/` | Spoofs the kernel release string (`--kernel-release=\\Linux\\...`) so that GNU C Library (glibc) does not abort with `FATAL: kernel too old`. | Essential for maintaining compatibility with modern distributions (Ubuntu 24.04/Debian 13) on Android. |
| **Android Seccomp Filter** | `patches/proot/seccomp-android.patch` | Bypasses modern syscalls blocked by Android 16 Bionic seccomp filters: `arch_prctl`, `rseq`, `membarrier`, `process_vm_readv/writev`, `clone3`, `openat2`, `mlock/munlock`. | Guarantees reliability for Node.js, Rust stdlib, and Chromium running inside guest containers. |

### 2.2 Anatomy & Internal Mechanics of the PRoot-distro Framework (`proot_distro` Python v5.10.3)

| Python Module | Source File | Internal Logic & Operational Mechanics | Architectural Implication for PDN |
|---|---|---|---|
| **Hardened Descriptor VFS** | `dirfd.py` (57KB) | Traverses filesystems using sequential `openat(2)` descriptors with `O_NOFOLLOW`. Protects against symlink swap attacks mounted by guest processes during host operations. Uses `O_NONBLOCK` on regular files (`open_regular_at`) to prevent hangs from malicious named pipes (FIFOs). Uses descriptor-pinned `_chmod_fd` because Linux `fchmodat` lacks `AT_SYMLINK_NOFOLLOW`. | PDN must replace recursive path string manipulation with a Rust `DirFd` abstraction wrapping `nix::fcntl::openat`. |
| **Inheritable POSIX Lock Manager** | `locking.py` | Employs non-blocking POSIX `flock(2)` across two namespaces: `ContainerLock` (Shared for `login`/`run`/`backup`; Exclusive for `install`/`remove`/`reset`/`rename`/`copy`) and `BuildLock` (Exclusive per image-tag). Clears `O_CLOEXEC` before `execvpe()` so descriptors remain held across the container session lifecycle. | Prevents race conditions and corruption when concurrent operations target the same container rootfs. |
| **Session Registry & Process Teardown** | `session.py`, `commands/kill.py` | Active sessions are recorded at `$RUNTIME_DIR/sessions/<pid>.json`. Liveness is probed via shared non-blocking `flock` (if acquired, the session is dead and cleaned up). `kill` scans `/proc/*/fd` to match the session file inode, then executes phased escalation: `SIGQUIT` to PRoot (graceful tracee cleanup) followed by `SIGKILL` to remaining orphans. | Enables accurate `podnext ps` process monitoring and clean `podnext kill` execution without residual zombies. |
| **Dynamic Sysdata Generator** | `sysdata.py` | Dynamically generates `/proc/stat` aligned with online CPUs extracted from host `/proc/cpuinfo`. Synthesizes `/proc/loadavg`, `/proc/version`, `/proc/uptime`, `/proc/sys/fs/overflowuid` (65534), `/proc/sys/fs/overflowgid`, and `/proc/sys/kernel/cap_last_cap`. | Eliminates crashes in `htop`, `top`, and Go runtimes caused by CPU topology mismatches on Android. |
| **Link2Symlink Tree Rewriter** | `l2s.py` | Scans and rewrites `.proot.l2s.` / `.l2s.` symlink targets in-place after container relocations (`rename`, `copy`, migration). Traversal is protected with signal blockers (`SIGINT`/`SIGQUIT`) to prevent half-rewritten states. | Preserves hard link integrity when containers are moved or cloned. |
| **OCI Dockerfile Builder & Layer Differ** | `commands/build.py`, `helpers/dockerfile.py`, `helpers/layer_diff.py`, `helpers/oci_writer.py` | Parses Dockerfile syntax (`FROM`, `RUN`, `COPY`, `ENV`), executes `RUN` directives in a temporary container sandbox, computes filesystem deltas with whiteout markers (`.wh.<file>`), and serializes standard OCI v1 manifests and tarballs. | Transforms PDN into a self-contained image builder that operates without a running Docker daemon. |

---

## 3. Maturity Gap Evaluation & Opportunities for PDN

The comparison of current PDN crates `proot-core`, `proot-distro-core` and `podnext` with upstream implementations finds the following concrete development opportunities:

1. **Session Registry & Inode-Based Process Teardown, `session` & `kill`**:
   - *Current State*: PDN lacks a centralized per-PID session registry with inherited `flock(2)` descriptors.
   - *Risk*: Abrupt termination of `podnext run` can leave background processes running as orphaned zombies in Termux.
2. **Hardened Descriptor VFS, `dirfd`**:
   - *Current State*: Some operations in `proot-distro-core` rely on standard `std::path::PathBuf` walking.
   - *Risk*: Susceptibility to symlink race conditions or FIFO deadlocks when operating on untrusted container filesystems.
3. **Link2Symlink Tree Rewriting on Container Relocation, `l2s`**:
   - *Current State*: `podnext rename` and `podnext copy` relocate rootfs directories but do not update embedded `.proot.l2s.` symlinks.
   - *Risk*: Internal hard links, such as `/usr/bin/perl` or package manager links, become broken symlinks after a rename.
4. **Custom Image Building, `build`**:
   - *Current State*: PDN can pull and unpack images from Docker Hub, but lacks a native `podnext build` subcommand for local Dockerfiles.
5. **Dynamic Host CPU Topology Alignment, `sysdata`**:
   - *Current State*: Synthesized `/proc/stat` entries in `sysdata.rs` use static rows rather than reading dynamically offline CPU cores from `/proc/cpuinfo`.

---

## 4. Infrastructure Evolution

### 4.1 Dual-Runtime Backend: The `RuntimeBackend` Trait
PDN decouples execution backends via a polymorphic trait interface:

```rust
#[async_trait]
pub trait RuntimeBackend: Send + Sync {
    async fn create_container(&self, spec: &ContainerSpec) -> Result<String>;
    async fn start_container(&self, id: &str) -> Result<()>;
    async fn exec_process(&self, id: &str, exec_spec: &ExecSpec) -> Result<ChildProcess>;
    async fn stop_container(&self, id: &str, timeout_s: u32) -> Result<()>;
    async fn inspect_container(&self, id: &str) -> Result<ContainerInspection>;
}
```

1. **`ProotBackend`, Lightweight & Instant**:
   - Optimized for 80% of everyday workflows: Debian/Ubuntu/Alpine shells, compilers, Python/Node.js, Playwright, toolchain development.
   - Metrics: Cold-start 0.09s, zero virtualization memory overhead, direct native CPU instruction execution.
2. **`UmlBackend`, Kernel & Docker Full-Fidelity**:
   - Connects PDN to the User-Mode Linux daemon `uml4termuxd` through UNIX domain sockets.
   - Activated when users run `podnext docker` or launch containers requiring systemd, iptables, wireguard, overlayfs, and cgroup controllers.
   - Delivers real Linux kernel isolation and does not require Android root privileges.

### 4.2 Termux Packaging & Ecosystem Standardization: `termux-packages`
- **Binary Separation**: A clear separation between `podnext`, the CLI client, and `podnext-daemon`, the service supervisor.
- **Termux Build Recipe**: Up-to-date `termux-package/recipe/build.sh` aligned with NDK r27c/r30 for submission to official Termux repositories.
- **Runit Service Scripts**: Standard service templates at `$PREFIX/var/service/podnextd` enable supervisor management through `sv up podnextd` and `sv down podnextd`.

### 4.3 Android Sleep Mitigation & Phantom Process Killer Protection
Android 12+ enforces a *Phantom Process Killer* that limits background child processes to 32, and aggressive Doze Mode:
- **Android WakeLock Engine**: The engine acquires `PARTIAL_WAKE_LOCK` automatically while supervised services run.
- **Foreground Service Integration**: Integration with Termux:API and Termux:Boot to maintain persistent notifications, preventing Android OOM termination.

### 4.4 Image Store & Registry Transport — planned for the next major version

Planned infrastructure, not implemented. Full analysis, measurements and the reference architecture:
`research/IMAGE_STORE_AND_PUSH.md`. Tracked by #37 for the lookup and #39 for the layer store.

Today an image in PDN **is** its container directory: `build local` writes
`~/.podnext/containers/<store-key>/rootfs` and the layer blobs are discarded after extraction. That
model cannot serve `push`, because the model retains nothing to send. `image push` currently fails with
`Image '<ref>' not found locally` for an image that is present, and will refuse with *"PDN does not
retain layer blobs after extraction"* even once the lookup is fixed.

The next major version changes the model rather than patching around it:

- **Images become OCI artifacts**: manifest + config + layer blobs, digest-addressed. Containers
  become *derived* state, produced when the system applies layers to a rootfs, and not the image itself.
- **Layer cache**: one file per blob named by the digest of its content, behind a digest-grammar
  validator, so a crafted digest cannot escape the cache directory. That is the same trust boundary as #16.
- **Build emits one layer per instruction**: it snapshots the stage rootfs and diffs, with OCI
  whiteouts for deletions, and an image-config `history` whose non-empty entry count matches the
  layer count. Multi-stage support, with `COPY --from=<stage>` and per-stage `ENV`/`ARG`/`WORKDIR`/`USER`, is part of
  the same step.
- **Registry transport**, Docker Registry v2: a HEAD request skips blobs the registry already has,
  streams the missing ones, then PUTs the manifest. Credentials are scoped to the registry's own
  origin and stripped on cross-origin redirects. The 401/403 responses and rate limits get named errors. `pull` is
  the same path in reverse, and `install` works offline from a cached manifest.
- **Local OCI layout archives**: install from an image-layout archive, with blobs verified against
  the digests the archive names and bounded reads, because an archive is untrusted input.

Cost, measured on this device with Termux aarch64: the snapshot/diff work is in the 0.1 s to 0.25 s band per
build. The rootfs walk measured 105 ms / 575 entries, 229 ms / 6,188 and 214 ms / 11,641. This cost is paid at build
time. The alternative, no artifact store and a tar of the rootfs at push time, costs ≈0.35 s raw /
≈2.1 s gzip per 307 MB image and produces a single opaque layer with no build history. It is the
intermediate option, not the target.

Prerequisite: the licence metadata must be consistent before any code is compared against the
GPL-3.0 reference, issue #40.

---

## 5. Architectural Design

### 5.1 Decoupled Client-Daemon IPC Architecture
- **Thin Client**: The `podnext` binary translates CLI commands into structured RPC requests.
- **Robust Daemon `podnextd`**: It manages centralized state, lock coordination, cgroup limits, and network routing.
- **IPC Protocol**: UNIX Domain Sockets utilizing JSON-RPC 2.0 or D-Bus Wire Protocol at `$TMPDIR/podnext/podnextd.sock`, with automatic direct-execution fallback if the daemon is offline.

### 5.2 Event-Driven Reactive State Engine
- **State Store**: Replaces raw directory scanning with a robust embedded database engine, `sled` or `sqlite`, at `$PN_DATA_DIR/state.db`.
- **Atomic State Transitions**: Status changes `Creating` -> `Installing` -> `Stopped` -> `Running` -> `Failed` are recorded through Write-Ahead Logging, WAL. That record prevents orphaned state on ungraceful exits.
- **Event Broadcasting**: Tokio broadcast channels stream lifecycle events to the CLI `podnext log follow`, to D-Bus monitors, and to web dashboards in real time.

### 5.3 Container Runtime Interface, CRI, and OCI Compliance
- Finalizes `pn-next/src/runtime.rs`, the CRI gRPC layer built on `tonic`.
- Enables lightweight container orchestration for `k3s` or edge agent runners, and lets an Android Termux instance serve as an edge computing node.

---

## 6. Granular Module & Helper Specifications for Single-Pass Execution

The following technical specifications define the modules required for unified implementation:

### 6.1 Module `proot-distro-core::session` & `podnext kill` / `podnext ps`
- **Purpose**: Centralized container session registry and reliable process tree termination.
- **Implementation**:
  1. *Data Format*: Structured `$RUNTIME_DIR/sessions/<pid>.json` files storing `container_name`, `pid`, `started_at`, `login_uid`, and `cmdline`.
  2. *Inherited Locking*: Before the call to `execvpe` for PRoot launch, acquire `flock(fd, LOCK_EX)` on the session file and clear the `FD_CLOEXEC` flag.
  3. *Liveness Probing*: `podnext ps` attempts a non-blocking shared lock `flock(fd, LOCK_SH | LOCK_NB)` on each registry file. Success indicates the session has terminated and is safe to unlink. Refusal with `EWOULDBLOCK` confirms the session is alive.
  4. *Teardown Engine for `podnext kill <container> [--all]`*:
     - Inspect `/proc/[0-9]*/fd/*` symlinks to identify processes holding the session file inode.
     - Execute phased signal escalation:
       - Phase 1: Send `SIGQUIT` to the root PRoot PID, which triggers the internal `kill_all_tracees2` handler.
       - Phase 2: Wait 500ms, then verify whether guest processes remain.
       - Phase 3: Send `SIGTERM` followed by `SIGKILL` directly to any surviving orphaned tracees.

### 6.2 Module `proot-distro-core::dirfd`, Hardened Descriptor VFS
- **Purpose**: Prevent symlink race attacks and FIFO blocking during host-guest file operations.
- **Implementation**:
  1. Rust `DirFd(RawFd)` wrapper with automated cleanup via `Drop`.
  2. Core methods:
     - `open_dir_at(&self, name: &str) -> Result<DirFd>` with `O_NOFOLLOW | O_DIRECTORY | O_CLOEXEC`.
     - `open_regular_at(&self, name: &str, flags: OFlag) -> Result<File>` with `O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC`, verifying `S_IFREG` via `fstat`.
     - `safe_chmod_at(&self, fd: RawFd, mode: u32) -> Result<()>` modifying permissions directly through open descriptors.
     - `atomic_write_at(&self, name: &str, content: &[u8]) -> Result<()>` writing to unique temporary inodes followed by `renameat(2)`.

### 6.3 Module `proot-distro-core::l2s`, Link2Symlink Tree Rewriter
- **Purpose**: Repair intermediate hard link symlinks when containers are renamed or relocated.
- **Implementation**:
  1. Function `rewrite_l2s_tree(rootfs: &Path, old_rootfs: &Path, new_rootfs: &Path) -> Result<usize>`.
  2. Recursive tree traversal:
     - Check symlink targets: if matching `.proot.l2s.` or `.l2s.` prefixed with `old_rootfs`, substitute with `new_rootfs`.
     - Use atomic link creation and replacement via `renameat`.
  3. Guard traversal with the signal blockers `SIGINT`/`SIGTERM` to ensure atomic completion.
  4. Automatically invoked at the conclusion of `podnext rename` and `podnext copy`.

### 6.4 Module `podnext build` (Native Dockerfile Builder & OCI Layer Differ)
- **Purpose**: Enable building custom container images from Dockerfiles directly in Termux.
- **Implementation**:
  1. *Dockerfile Parser*: Supports `FROM`, `RUN`, `COPY`, `ADD`, `ENV`, `WORKDIR`, `EXPOSE`, `ENTRYPOINT`, and `CMD`.
  2. *Build Execution Engine*:
     - Spawns a temporary container sandbox from the base `FROM` image.
     - Executes sequential `RUN` steps inside the isolated PRoot environment.
  3. *Layer Differ*:
     - Computes SHA-256 hashes of modified and newly created files.
     - Detects deleted files and injects standard OCI whiteout markers such as `.wh.<filename>`.
     - Archives delta layers into compressed `tar.gz` blobs.
  4. *OCI Serializer*: Generates standard OCI v1 manifests, image configuration JSON, and registers images with the local cache.

### 6.5 Module `proot-distro-core::sysdata`, Dynamic CPU Topology Generator
- **Purpose**: Prevent failures in monitoring utilities such as `htop`, `top` and Node.js, caused by dynamic CPU core throttling on Android.
- **Implementation**:
  1. Reads host `/proc/cpuinfo` to detect currently online CPU cores.
  2. Synthesizes `/proc/stat` containing matching `cpu0`, `cpu1`, ... entries aligned with host USER_HZ.
  3. Generates realistic default values for `/proc/loadavg`, `/proc/uptime`, `/proc/sys/fs/overflowuid` and `/proc/sys/kernel/cap_last_cap`. The values are `0.12 0.07 0.02 2/165 765`, `124.08`, and `65534`.
  4. Stores synthetic files outside the guest rootfs, at `$CONTAINER_DIR/sysdata/`, and bind-mounts them read-only.

### 6.6 Module `proot-distro-core::locking`, Inheritable POSIX Lock Manager
- **Purpose**: Prevent data corruption caused by concurrent operations on container filesystems.
- **Implementation**:
  1. `ContainerLock` manages `$RUNTIME_DIR/locks/<container>.lock`:
     - `Shared`: for `login`, `run`, `backup`, and `copy` as the source.
     - `Exclusive`: for `install`, `remove`, `reset`, `rename`, and `copy` as the destination.
  2. `BuildLock` manages `$RUNTIME_DIR/locks/build/<image_tag>.lock` and is always `Exclusive`.
  3. Re-entrancy tracking prevents deadlocks during nested operations, such as a `reset` that calls `install`.

### 6.7 Networking Module (Userspace Rootless TCP/IP & Dynamic DNS Helper)
- **Gvproxy / Slirp4netns Integration**:
  - Delivers complete user-space TCP/IP stack emulation.
  - Enables container port publishing with `podnext run -p 8080:80` and needs no iptables or host root privileges.
- **Adaptive Dynamic DNS Helper**:
  - Monitors network route changes as Android switches between Wi-Fi and mobile data.
  - Routes container `/etc/resolv.conf` to a local DNS forwarding resolver managed by `podnextd`.

### 6.8 Storage Module: OverlayFS Emulation and Layer Deduplication
- **OverlayFS Emulation / Reflink Deduplication**:
  - Leverages rootless `fuse-overlayfs` or symlink/hardlink tree branching to eliminate storage duplication across multiple containers sharing a common Debian base.
- **Garbage Collection & Prune Engine**:
  - Automated pruning of dangling image layers and unreferenced volumes through `podnext image prune` and `podnext volume prune`.

### 6.9 Supervisor, Watchdog & IPC Module
- **Adaptive Backoff & Flapping Detection**:
  - Enhances the circuit breaker: it detects rapid crash loops, flapping, and transitions services into a `Quarantine` state rather than consuming CPU with infinite restart loops.
- **Systemd Unit File Importer**:
  - Translates basic systemd `.service` unit files into native `podnext service` definitions.

---

## 7. Security Hardening

### 7.1 Zero-Trust Guest Boundary
- Adheres to verified security practices: **all data received from guest environments is untrusted**.
- Symlinks that traverse outside the rootfs, such as `/data/data/com.termux` and `/storage/emulated/0`, are intercepted at the VFS layer and rejected unless `--bind` permits them explicitly.

### 7.2 Distro-Tailored Seccomp Profiling
- The filters return graceful `ENOSYS` or `EPERM` error codes for calls such as `sys_setxattr`, `keyctl`, or `bpf`. This prevents application crashes from blocked syscalls.

---

## 8. Unified Single-Pass Phased Master Plan

The implementation roadmap is structured for clean, sequential execution:

```
┌────────────────────────────────────────────────────────────────────────┐
│ PHASE 1: DISTRO CORE & PROCESS LIFECYCLE (Target: PRoot Parity Plus)   │
│ - Implement proot-distro-core::session (Registry JSON & flock probe)   │
│ - Implement proot-distro-core::dirfd (Hardened VFS & FIFO defense)     │
│ - Implement proot-distro-core::l2s (Symlink hardlink tree rewriter)    │
│ - Implement podnext ps & podnext kill (SIGQUIT signal escalation)      │
│ - Dynamic sysdata.rs aligned with host /proc/cpuinfo                   │
├────────────────────────────────────────────────────────────────────────┤
│ PHASE 2: CONTAINER IMAGE BUILDER & REPOSITORY (Target: Docker CLI Lite)│
│ - Implement Dockerfile parser (FROM, RUN, COPY, ENV, WORKDIR)          │
│ - Implement layer diffing engine and whiteout (.wh.*) generator        │
│ - Implement CLI command podnext build -t <tag> <dir>                   │
│ - Implement OCI blob deduplication and podnext image prune             │
├────────────────────────────────────────────────────────────────────────┤
│ PHASE 3: DUAL-BACKEND (PROOT + UML) & ROOTLESS NETWORKING              │
│ - Abstract RuntimeBackend trait (ProotBackend & UmlBackend)            │
│ - Implement socket client to uml4termuxd (Bionic kernel execution)     │
│ - Integrate userspace TCP/IP networking (gvproxy / slirp4netns)        │
│ - Dynamic DNS helper for Android Wi-Fi / Cellular mobility             │
├────────────────────────────────────────────────────────────────────────┤
│ PHASE 4: PRODUCTION PACKAGING, DAEMON & ORCHESTRATION                  │
│ - Stabilize CRI gRPC socket for edge k3s worker nodes                  │
│ - Submit official package recipe to termux-packages                    │
│ - Provide default Runit service script ($PREFIX/var/service/podnextd)  │
│ - Integrate WakeLock & Termux:Boot autostart supervisor                │
└────────────────────────────────────────────────────────────────────────┘
```

This roadmap serves as the definitive engineering blueprint for elevating container infrastructure on Android into a production-grade, secure, and developer-friendly runtime platform.

---

## 9. Roadmap to v0.2.0 — stability, simplicity, and the interactive shell

**Status, 5 Nov 2026: not complete.** Every figure below names the command that produced it. The
`v0.1.9` release was withdrawn before it was tagged at `5b479d1`, `Cargo.toml` still reads `0.1.8`,
and section 9.3 still has open work items. A one-line status per sub-section follows its heading.
Ledger: 80 PASS / 0 FAIL / 13 SKIPPED of 93 offline, and 93 PASS / 0 FAIL / 0 SKIPPED in CI at
`892549a`, from `python3 scripts/build_issue_ledger.py --offline | tail -1`.

Decisions already taken, so they are not reopened here:

- **Licence**: `GPL-2.0-or-later`, declared identically in `LICENSE`, `Cargo.toml` and `README.md`,
  pinned by four ledger predicates, issue #40, closed.
- **Image model**: unchanged in v0.2.0. Images are still container directories. The OCI artifact
  store, layers and registry transport are section 4.4 and belong to the next major version.
- **TUI**: not in v0.2.0. A TUI is a consumer. It is cheap only once a stable machine interface
  exists underneath. section 9.2 builds that interface first.

### 9.1 Stability and honesty — freeze breadth, deepen the paths people use daily

**Status, 5 Nov 2026: landed.** Every command carries a `stable` / `experimental` label, and every stub refuses with a named reason, issue #42 at `fcabc1c`. The exit-code contract is documented, and the CLI-reference contract test runs the built binary, issue #42 at `28c4594`. The audit findings were triaged into the tracker. Two halves of these criteria were read closely enough afterwards to notice that nothing had exercised them. Both are now closed. The **examples** in `docs/src/cli-reference.md` had no runner until **#64** built `scripts/check_doc_examples.py`. That script covers 50 blocks: it runs 10, excludes 41 with a printed reason, and it found two genuinely wrong examples. The exit-code classes are asserted by `crates/podnext/tests/exit_codes.rs`, `665b3ba`, now 7, as well as by the documentation. The ledger verifies this: **129 PASS / 0 FAIL / 0 SKIPPED of 129 in CI at `ad123ef`**, so no check is without a verdict. Offline the ledger shows 111 PASS / 18 SKIPPED, where the memory guard refuses the bench build.

The defect pattern of the last three rounds was consistent: a command existed, printed something
plausible, and did not do what it claimed. The examples are `image push`, benchmark reporting, `image inspect`
and `login --detach`. v0.2.0 removes the pattern rather than more instances of it.

| Work item | Acceptance criterion |
|---|---|
| Label every command `stable` / `experimental`, in `--help` and in the docs | no command is unlabelled |
| Replace every remaining stub with an explicit refusal that names the reason | zero occurrences of "not fully implemented" in user-facing output |
| Document the exit-code contract (0 ok, 1 usage/error, 2 invalid input) and enforce it | a test asserts the code for each failure class |
| Contract test: every command, flag and example in `docs/src/cli-reference.md` must exist and run | `podnext image list` vs `ls` cannot happen again |
| Triage the 13-agent audit findings into issues | every finding either fixed, filed, or explicitly declined with a reason |

### 9.2 Simplicity and performance — one source of truth, one module per domain

**Status, 6 Nov 2026, after four rounds: the split is done, the latency criterion is recorded as an exception, and metadata is routed.** `main.rs` went from 4,679 to **1,040** lines. The whole Clap surface is in a new `cli.rs` at 1,445 lines. `commands/mod.rs` went from 3,719 to **813** lines across eight extractions. The extractions are `image` / `volume` / `network` / `sync` / `container`, then `policy` / `quota` / `log` / `dbus` / `socket`, then `service` / `watchdog` / `power`, then `build` / `gitops`. Every `--help` screen is byte-identical before and after. The 123-screen dump is the gate, and as of this round it is a **tracked fixture** at `research/cli-help-baseline.txt` with 123 screens and sha256 `8236a573...`. CI compares that fixture. So a help text change cannot land without an update to that fixture in the same commit. That comparison settled one question by measurement rather than assumption: the dump is **platform-independent**. The fixture generated on the device at aarch64 is byte-identical to the runner's dump at x86_64. So one fixture suffices, and it does not need a key per platform the way the warning ceiling does. The cached container index landed at `cf3f487` with mtime invalidation proved in both directions: in-process p50 **1.3-2.2 ms** with ten containers, against a criterion of under 10 ms. What that criterion cannot mean here is end-to-end: process start alone is p50 31-36 ms. So the 42-53 ms baseline was never the store scan.

**Daemon routing landed**, and the criterion attached to it is recorded as an exception rather than claimed. All six metadata queries, `list` / `info` / `ps` x json/table, produce an empty diff between the daemon path and the in-process fallback. Round trips are constant at one container and at ten containers: 1 with the daemon and 0 without. A container created after the daemon started appears in the next answer, and it is never stale. Latency independence is **not** met: with ten containers the daemon path is slower at in-process p50 5.20 ms against 1.48 ms. It still grows with the container count. The reason is that freshness revalidates each `metadata.json` over IPC. An O(1) freshness key serves in-place metadata rewrites stale, a regression against the index cache above. So the criterion is left unmet on purpose with the measurement beside it. `SIZE` honesty landed earlier at #44, and environment paths landed at #46.

The file-length row is **met**, in a fourth round, at every file in `crates/podnext/src`:
`cli.rs` 1,450 to **687** and `main.rs` 1,170 to **479**, with the surplus in `direct.rs` at 390,
`cli/system.rs` at 218, `cli/service.rs` at 219, `subcommands.rs` at 208, `dispatch.rs` at 120 and seven
smaller `cli/` modules - no file over 800 among them. The move was text-only and the help dump is
byte-identical to the tracked fixture at `sha256 8236a573be5952cdd983c5c8f6cd3833f8e6aa0b1782e1dc1cd08c638fd2b8c5` and
123 screens, and `scripts/local_gate.sh` is GATE GREEN with the `podnext` bin still at 26 warnings.
Measured and **not** claimed closed: `commands/mod.rs` is 813 lines and `native.rs` (a pre-existing
file the ticket did not name) is 1,605.

`native.rs` was the last one, and it was this project's own doing - 1,151 lines before the detach/attach
round, 1,605 after, so it was split rather than excused, issue #72 at `eaf3e2d`: 261 lines of module root behind
`native/image.rs` 579, `native/session.rs` 623 and `native/meta.rs` 184, along the OCI-install vs
container-session seam already present in the file. The guard there is deliberately **not** the help
fixture, which proves nothing about a file with no Clap surface: it is the 84 test targets,
`shell_fidelity.py`, `soak_sessions.py`, the quoting table, and the detach/attach round trip re-measured on
the split binary (same shell pid across two attaches, marker survives, no orphan after a supervisor
SIGKILL).

What remains above the guideline is one file, and it is stated rather than smoothed: `commands/mod.rs` is
**813** lines - 13 over a `~`800 guideline, and the module root, where the re-exports live by necessity.
The same is true of `cli.rs` at 687, whose 453-line `Commands` enum cannot leave the root without a change to
the rendered surface. Both are floors of their design, not places the split stopped.

Measured baseline: `image ls` 28.6 ms and `list -a` 42 to 53 ms for ten containers, dominated by process
start plus a full container load. `image ls` reports `SIZE` as a `stat` of the directory, 3.4 KB for a
307 MB image. Two monolithic files carry most of the CLI: `main.rs` 4,220 lines and
`commands/mod.rs` 3,604 lines.

| Work item | Acceptance criterion |
|---|---|
| Split the CLI by domain (`container`, `image`, `build`, `shell`, `service`, `network`, `policy`, `system`) | no file over ~800 lines; `--help` output byte-identical |
| Cache the container index with mtime invalidation | metadata query p50 < 10 ms with 10 containers |
| Stop printing a number that is not the image size | either a real size computed once and cached with a staleness marker, or an explicit `unknown` |
| Route read-only metadata queries through the daemon that already exists (`podnext-daemon`) | CLI latency independent of container count |
| Honour `TERMUX__HOME`, `TERMUX__PREFIX`, `TERMUX_APP__PACKAGE_NAME` and `XDG_*` instead of 12 hard-coded Termux paths | the suite passes with `HOME`, `PREFIX` and the `XDG_*` variables relocated (`TERMUX__PREFIX` excluded: it is owned by termux-exec, and relocating it makes even `/bin/sh` unresolvable - measured, see `known-limitations.md`) |

### 9.3 The interactive shell — the largest untapped value

**Status, 6 Nov 2026, fourth round: landed - six of six.** `podnext doctor` runs on the host and inside the guest. It reports 12 checks when a container is present, at `285720a` and `a6c4fa6`, issue #45. The `run ... -- <cmd>` **argv quoting table** is at `5c34115`. It covers fifteen arguments. The set includes quotes, a semicolon, a dollar-variable, and a command substitution that must not expand. It also includes a glob, an empty argument, a newline, a tab, a backslash, and multi-byte text. The table compares them against what the guest actually received, NUL-separated, so a newline inside one argument cannot read as two. A second test asserts that nothing expanded and that no host identity leaked. **Terminal fidelity and the soak are now measured and met.** `scripts/shell_fidelity.py` reports `stty size` at a fixed winsize. It reports a live resize that the guest sees from 24x80 to 40x120. It also reports `TERM` and `COLORTERM` inside the guest, a default `PS1`, and an interactive prompt. A UTF-8 round trip passes through both stdout and guest stdin. A negative control still goes red. `scripts/soak_sessions.py` runs twenty start/stop iterations and asserts that no container process survives, with an injected leak caught at the iteration where it happens.

**Detach and attach is a real round trip, and the two items that were still open are closed.** #57
at `ff2e1e3` made `run --detach` start a supervised PTY session and `attach` reconnect to it: the same
shell pid answers two successive attaches, a marker written while detached is still there in the second,
the process census is `0 proot/podnext rows` before and after, and a SIGKILL of the supervisor leaves no
orphan. The supervisor is `setsid` with `PPid=1`, so it is not in the launcher's process group - the #68
lesson applied to a session that must outlive whoever started it. Two simplifications are **declared** in
`known-limitations.md` rather than hidden: a resize while attached is not re-forwarded, because `SIGWINCH` is not
re-emitted, and output produced while detached is appended to the session log rather than replayed.

**The bindings and the `USER` story are measured inside a guest, not assumed.** #66 at `523ae58` measured
what the documented paths actually are: `/sdcard`, `/storage/emulated/0` and `/storage/self/primary` exist
inside a guest as its **own empty rootfs directories**. The device id matches `/`, and `/proc/mounts`
carries no such bind. The `/tmp` and `/dev/shm` paths are per-container, and the host home is not mounted at all -
with a negative control that goes red when falsified. On `USER`: `build local` states the mapping,
`run` and `login` now print it on stderr with stdout untouched, and both `run --user` and `login --user`
refuse with the measurement - before this round `login --user 1000` was accepted and **silently ignored**,
which the same department found when it ran that command inside a guest.

The documented examples are executed too: `scripts/check_doc_examples.py` runs the 50 fenced blocks in
`cli-reference.md` - 10 executed, 41 excluded with a printed reason - and it runs in CI, where it found
two genuinely wrong examples: `dbus list --socket`, and a `dbus call` that misses a positional. #64.

The fidelity measurement found a **regression**, which is the useful part. A guest started with `login` had no prompt and was unable to read stdin. The reason is that `proot-core` gave the child its own process group while the tty foreground group was still the caller's. So the guest read stopped on `SIGTTIN` while writes and ioctls kept working. The installed v0.1.7 worked, and that is what proved it was a regression rather than a mystery, issue #68. The fix landed, with the harness as the oracle, and two pure functions and their unit tests in `crates/proot-core/src/lib.rs` guard it. `TERM` was also hardcoded while `COLORTERM` passed through. It is now forwarded. One open question is recorded rather than claimed: the harness proves that the guest sees the new winsize, not that the system delivers `SIGWINCH`.

**Landed: real `--detach` plus `attach`, issue #57.** `run --detach` starts the guest under a supervised PTY and prints the supervisor pid. `attach <container>` reconnects to the same live shell: the same guest pid across two attaches, a marker written while attached survives a detach, and no orphan after the guest exits. Two simplifications are declared rather than hidden. A resize of the attaching terminal is not re-forwarded while attached, and output produced while detached is written to the session log but not replayed. Both are recorded in `known-limitations.md`. The bindings and the `USER` story are documented in `constants.rs` for `/sdcard`, `/storage/emulated/0` and `/storage/self/primary`, and `--user` is refused with the mapped-root reason. The project has never measured them **inside** a guest. That is **#66**. `known-limitations.md` lists the unmeasured items.

The ability to `login` is not the same as daily use of the shell. Ordered by what blocks people:

| Work item | Acceptance criterion |
|---|---|
| Real `--detach` plus `attach` back into the same session — **landed** (#57): `run --detach` → `attach` reconnects to the same shell | run → detach → attach returns to the same shell, no orphan session (measured, #57) |
| Terminal fidelity: `SIGWINCH`/resize, `TERM`/`COLORTERM` passthrough, UTF-8 and locale, a sane default `PS1` | `vim`/`htop` render and resize correctly inside the guest |
| Correct argv quoting for `podnext run … -- <cmd>` | a table of quoting cases passes, including embedded quotes and `;` |
| Predictable bindings (`/sdcard`, `/storage`, shared home/tmp) and an honest `USER` story | documented, tested, and the build output states USER is not enforced |
| Session hygiene: reuse, `ps`/`kill` consistency, no zombie guests after SIGKILL | a soak run leaves no processes behind |
| `podnext doctor` on the host and inside the guest | reports DNS, `/dev/shm`, `os-release`, cgroup, TMPDIR, proot argv with pass/fail |

### 9.4 Explicit non-goals for v0.2.0

**Status, 5 Nov 2026: held.** Only `doctor` was added beyond the existing surface, and section 9.3 requires that new top-level command. The `cluster`, `dbus`, `gitops` and `multiarch_build` commands were not expanded, and there is no TUI. `podnext --help` lists the same command set as before this round.

- No expansion of `cluster`, `dbus`, `gitops` or `multiarch_build`. `crates/pn-next/src` is 22,664 lines, the figure `scripts/check_non_goals.py` records as a ceiling: `find crates/pn-next/src -name '*.rs' | xargs wc -l`. An earlier draft of this section said 23,760, which no command reproduced.
  Complexity that is not exercised rots, and the project has the receipts: a benchmark target drifted
  through 26 compile errors behind a green check.
- No new top-level commands beyond `doctor`, which section 9.3 requires: that is the 38th, and it is
  already labelled and gated. Everything else new waits. v0.2.0 makes the existing 37 honest first.
- No TUI until section 9.2 lands, and no OCI store until the next major version in section 4.4.

The audit run that accompanies this section supplies the concrete defect list. Its findings are
triaged into the issue tracker and folded back here as they are confirmed.

---

## 10. v0.3.0: the mobility target

> **Numbering note, 6 Nov 2026.** This section was written as the `v0.2.0` target. The release that
> carried the section 9 stabilization - the audit, the module split, the detach/attach round trip, the
> gates - took the `v0.2.0` number instead, because `v0.1.9` was tagged and withdrawn and a tag cannot be
> reused. The mobility polygon below is therefore the NEXT release's target. Its acceptance tests are
> unchanged: they were never about a version number.

The goal for v0.3.0: podnext is usable as a daily driver with the same mobility as Termux's
`proot-distro`. Mobility means four things, each with an acceptance test, so "done" is not a matter
of opinion. Measured against the reference implementation installed on this device (the Python
`proot-distro` package) and against podnext `v0.1.8`.

### 10.1 Gap: bringing something in

| Capability | Reference | podnext today | Acceptance test for v0.3.0 |
|---|---|---|---|
| Install from a registry reference | yes, with alias derivation | partial: `install <image>` with a name, no alias derivation | `podnext install alpine:3.20` creates a container named by the derived alias, no extra flags |
| Install from a local rootfs tarball | yes | no | `podnext install ./rootfs.tar.gz` works offline |
| Install from a local OCI image-layout archive | yes (marker-detected, blobs verified against the declared digests) | no | `podnext install ./image.oci.tar` verifies digests and refuses a mismatch |
| Install from a URL | yes, but **no checksum** — it verifies the declared body length only (`grep -rn 'checksum' commands/install.py commands/install_local.py helpers/download.py` → 0 matches; `helpers/download.py:133 declared_length`, `:149 require_complete_body`) | no | `podnext install https://…/rootfs.tar.xz` works; a `--checksum sha256:…` option is a row where **PDN can beat the reference**, which cannot verify one |
| Multi-arch | `--architecture` plus emulator | `--qemu` only in a legacy pre-pass | an x86_64 rootfs runs on this aarch64 device through the emulator, documented |

### 10.2 Gap: living inside it

podnext's `login` accepts exactly two things today: `--isolated` and a trailing command. `-u/--user`
is refused, because the guest runs as the mapped root, see [Known limitations](known-limitations.md). It
is no longer part of the accepted surface. The reference polygon is far wider, and this is where
"mobility" is felt:

| Flag | Why it matters | Acceptance test |
|---|---|---|
| `--bind` (repeatable) | reaching the phone's storage without editing configs | `login c --bind /sdcard:/mnt/sd` works, pass-through mounts verified inside |
| `--shared-tmp`, `--shared-home`, `--termux-home` | sharing state with Termux deliberately | each is testable inside the guest |
| `--work-dir` | starting in the right place | `PWD` inside matches |
| `--hostname` | applications that care | `hostname` inside matches |
| `--fix-low-ports`, `--redirect-ports` | servers on privileged ports | a listener on 80 inside is reachable as documented |
| `--no-sysvipc`, `--no-link2symlink` | escape hatches for broken guests | documented and honoured |
| `--detach` + `attach` | leaving a long job running | detach, then reattach to the same live shell; **landed** as `run --detach` + `attach` (#57) — `login` itself still takes no `--detach` |
| `--env` (repeatable) | per-session configuration | visible inside |
| `--get-proot-cmd` | debuggability: print the exact proot invocation | prints a copy-pasteable command that reproduces the session |
| `--user` semantics stated honestly | GUI-less guests often need root | `--user` documented with what PRoot can and cannot enforce |

### 10.3 Gap: moving things out and between devices

| Capability | Reference | podnext today | Acceptance test |
|---|---|---|---|
| Portable export | no `export` subcommand; the reference exports by building an OCI image-layout tarball with `build -o FILE` (`parser.py:367-370` `-o/--output`; `helpers/oci_writer.py:155 write_oci_archive`) | no | `podnext export <c> ./c.oci.tar`, then `install` it on another device |
| Import an archive | `install <archive>`, not an `import` command (`commands/help/pages.py:1061 TOP_COMMANDS` lists 18 commands, none named `export` or `import`) | no | round-trip on the same device first, then across |
| Registry push/pull of built images | yes (section 4.4) | impossible: no layers retained (#39) | `build local` → `push` → `pull` on a second machine |
| Backup/restore | yes, with compression | yes | already passing; keep it |

### 10.4 Gap: ecosystem

| Capability | Reference | Acceptance test |
|---|---|---|
| Short alias | `pd` | `pd` resolves to `podnext` in a Termux package |
| Completions | bash + fish | `podnext completion` emits both and they load |
| Curated distro names | a name like `alpine` installs without a registry reference | `podnext search` lists installable names and `install <name>` uses them |
| Discoverable help | a help page per command | every command's `--help` states its stable/experimental status and one example |

### 10.5 Recycling map — what to take from the reference, and how

Reimplemented from the OCI/Docker specifications, **never transcribed**: the reference is GPL-3.0 and
this project is `GPL-2.0-or-later`. The boundary is described in `research/IMAGE_STORE_AND_PUSH.md`
section 5. What is taken is the *architecture*, module by module:

| Reference area | What it does | podnext counterpart to build |
|---|---|---|
| layer cache + manifest cache | digest-addressed blobs; the image is artifacts, the rootfs is derived | new `image-store` module (section 4.4) |
| pull / push | Docker Registry v2 with blob skipping, streaming, auth scoping | registry transport (section 4.4) |
| local archive install | raw rootfs tar and OCI layout, digests verified, reads bounded | `podnext install <archive>` |
| build engine + layer differ | one layer per instruction, whiteouts, multi-stage with per-stage scopes | replaces the current single-rootfs builder |
| login option polygon | isolated / shared / bindings / ports / detach / user | widen `login` to section 10.2 |
| session registry | recorded sessions, consistent `ps`/`kill`, teardown | extend what exists; detach/attach landed in v0.2.0 (#57) |
| sysdata, shm, l2s, dirfd | /proc stubs, shared memory, hardlink emulation, descriptor VFS | already ported in earlier rounds — verify parity, do not rewrite |
| state dir + paths | honours `TERMUX__HOME`, `TERMUX__PREFIX`, XDG | replace 12 hard-coded Termux paths |
| help pages + alias + completions | discoverability | section 10.4 |

### 10.6 Sequencing

- **v0.2.0**, shipped: stability, honesty, module split, cached index, env-var paths, and the crash
  class filed as #41. No new top-level commands and no TUI.
- **v0.3.0**: sections 10.1 to 10.4. The image store in section 4.4 is the spine, because import/export, push/pull and
  build-with-layers all sit on it. The `login` polygon is the other half and is independent of it, so
  the two can proceed in parallel.
- Performance is not a later phase: the cached container index and the daemon-owned state are v0.2.0
  work precisely because they make every v0.3.0 command cheaper by construction.

Acceptance for the whole section: the 13-agent audit findings triaged, the four tables above
satisfied, and a two-device round trip demonstrated end to end.
