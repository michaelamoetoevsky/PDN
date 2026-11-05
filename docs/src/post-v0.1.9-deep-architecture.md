# Deep Architectural Forensics & Post-v0.1.9 Complex Subsystem Blueprint (v0.2.0 "Mobility")

**Author**: Hermes Agent Architecture Group  
**Target Milestone**: v0.2.0  
**Context**: Deep forensic analysis separating v0.1.9 (surgical stability & honesty) from complex, high-risk subsystem re-engineering (deferred to v0.2.0).  
**Hardware Baseline**: aarch64 Android 16 (Linux 6.6+ bionic, Termux prefix `/data/data/com.termux/files/usr`, no real root, no writable `/tmp`).

---

## 1. Subsystem A: OCI Content-Addressable Blob Store (`pn-image-store`)

### 1.1 The Failure Vector in v0.1.x
In v0.1.8, `podnext image pull <image>` was a deceptive façade:
```rust
// Old pseudocode in crates/podnext/src/main.rs (cmd_image_pull)
let mut container_mgr = ContainerManager::new(&config.storage.containers_dir)?;
container_mgr.create(&name, &image)?; // Silent no-op/fake success if image not local
println!("Image pulled successfully"); // Lie!
```
This was patched in v0.1.9 (Finding 01-1 / C2) with an honest hard rejection (`bail!`). However, v0.2.0 must implement real image pulling without running into Android sandbox limitations.

### 1.2 Anatomical Constraints on Android/Termux
1. **Unprivileged Extraction**:
   - `chown` / `lchown` fails with `EPERM` unless PRoot is intercepting syscalls or the extraction runs in unprivileged rootless mode where all files map to the current Termux UID (`u0_aXXX`).
   - Hard links: Android SELinux blocks cross-directory hard links or certain link creation types.
   - Special device nodes (`mknod` for `/dev/null`, `/dev/urandom` in rootfs layers): Always fails with `EPERM` on Android storage (`ext4`/`f2fs`).
2. **AUFS / OverlayFS Absence**:
   - Android kernels do not allow unprivileged user mounts of `overlayfs` or `fuse-overlayfs` without root.
   - PRoot cannot natively emulate dynamic layered union mounts without severe memory/CPU overhead (ptrace trap on every VFS resolution).
3. **Storage Quota & Deduplication**:
   - Flash storage on mobile devices is constrained. Unpacking complete duplicate distros wastes gigabytes.

### 1.3 Target Architecture: Two-Tier Storage Engine
```
~/.local/share/podnext/
├── storage/
│   ├── blobs/sha256/
│   │   ├── a1b2c3... (raw compressed OCI layer tarballs)
│   │   └── d4e5f6... (OCI image manifests & config JSON)
│   ├── index/
│   │   └── repositories.json (tags -> manifest digests)
│   └── cache/ (partial unpack cache)
└── containers/
    └── alpine-dev/
        ├── rootfs/ (flattened, sanitized rootfs)
        ├── config.json (OCI runtime spec / PDN profile)
        └── metadata.json (base image digest, created_at, layer_chain)
```

### 1.4 Unpack & Whiteout Sanitation Pipeline
When unpacking OCI layers into a flattened container `rootfs`:
- **Whiteouts (`.wh.<filename>`)**: Must be processed during sequential layer application:
  - Explicit file whiteout: `.wh.foo` deletes existing `foo`.
  - Opaque whiteout: `.wh..wh..opq` deletes all existing siblings in that directory.
- **Device Node Neutralization**: Intercept `mknod` entries in tar archives; replace them with dummy regular empty files or omit them, relying on PRoot's runtime bind-mounting (`-b /dev`).
- **Permission Normalization**: Force directory permissions to `0755` and executables to `0755`/`0644`. Mask out `setuid`/`setgid` bits (which are non-functional under Termux anyway and trigger audit warnings).

---

## 2. Subsystem B: Login Polygon & Proot-Distro Parity Engine

### 2.1 The 18-Flag Gap Analysis
Auditing Termux's official `proot-distro` (`commands/login/__init__.py`) against `podnext login` revealed a massive capability gap:

| Capability / Flag | `proot-distro login` | `podnext login` (v0.1.8) | v0.2.0 Target Contract |
| :--- | :--- | :--- | :--- |
| **Interactive Shell** | Default (`/bin/bash` / `/bin/sh`) | Default (`/bin/sh`) | Retain |
| **User Switching** | `--user <name>` | `--user <name>` | Retain |
| **Isolation Mode** | `--isolated` (clean env) | `--isolated` | Retain |
| **Custom Bindings** | `--bind <host:guest>` | **ABSENT** | `--bind <host[:guest]>` (repeatable) |
| **Shared Home** | `--shared-home` (host `$HOME`) | **ABSENT** | `--shared-home` (binds `$HOME` to `/root` or `/home/$USER`) |
| **Shared /tmp** | `--shared-tmp` | **ABSENT** | `--shared-tmp` (binds `$PREFIX/tmp` to guest `/tmp`) |
| **Shared X11 / Wayland** | `--shared-x11` | **ABSENT** | `--shared-x11` (binds `$PREFIX/tmp/.X11-unix` & Wayland sockets) |
| **Termux Home Bind** | `--termux-home` | **ABSENT** | `--termux-home` (binds host home to `/data/data/...`) |
| **Working Directory** | `--work-dir <path>` | **ABSENT** | `--work-dir` / `-w <path>` |
| **Fix Low Ports (<1024)**| `--fix-low-ports` | **ABSENT** | PRoot `-p` low port redirect (e.g. 80 -> 8080) |
| **Disable SysV IPC** | `--no-sysvipc` | **ABSENT** | `--no-sysvipc` (PRoot flag passthrough) |
| **Link2Symlink Workaround**| `--no-link2symlink` | **ABSENT** | `--no-link2symlink` (for Android hardlink quirks) |
| **Kill On Exit** | `--no-kill-on-exit` | **ABSENT** | Default kill children; `--no-kill-on-exit` to leave background daemons |
| **Custom Environment** | `--env KEY=VAL` | **ABSENT** | `--env <K=V>` (repeatable) |
| **Custom Hostname** | `--hostname <name>` | **ABSENT** | Intercept `/proc/sys/kernel/hostname` |

### 2.2 Forensic Discovery: The Disconnected CLI-to-Engine Plumbing
A deep code inspection across `crates/podnext/src/main.rs`, `crates/podnext/src/native.rs`, and `crates/proot-distro-core/src/runtime.rs` reveals the exact mechanical breakdown:

1. **CLI Layer (`main.rs:2117`)**: `cmd_run` declares `_volume: Vec<String>`, `_publish: Vec<String>`, `_workdir: Option<String>`, `_user: Option<String>`, prepending underscores to silence compiler dead-code warnings. These values are never passed downstream.
2. **Native Dispatcher (`native.rs:497`)**: `login_container_native` accepts only `(container, user, isolated, cmd)`. Any bind mounts or environment variables passed on the command line cannot be received.
3. **Core Engine (`runtime.rs:113`)**: `ProotRuntimeBackend::execute` populates `engine_guard.config.bind` strictly from `container.config.bind_mounts` stored statically on disk at container installation time. Dynamic per-invocation bindings never reach the PRoot argument builder.

#### Required Plumbing Overhaul for v0.2.0:
- Define an `InvocationOverrides` struct in `proot-distro-core`:
  ```rust
  pub struct InvocationOverrides {
      pub dynamic_binds: Vec<BindMount>,
      pub dynamic_env: Vec<(String, String)>,
      pub working_dir: Option<PathBuf>,
      pub user: Option<String>,
      pub shared_home: bool,
      pub shared_tmp: bool,
      pub shared_x11: bool,
  }
  ```
- Pass `InvocationOverrides` through `cmd_login` / `cmd_run` -> `login_container_native` -> `distro_core.login_with_overrides()` -> `backend.execute_with_overrides()`.
- Merge dynamic binds with static binds in `runtime.rs:113` before constructing `-b <src>:<dst>`.

### 2.3 Security Matrix for Custom Binds (`--bind`)
In v0.1.8, arbitrary path traversal in volume creation was discovered (Finding 05-2). The `--bind` engine in v0.2.0 must enforce strict guardrails:
1. **Canonicalization**: Resolve host path with `std::fs::canonicalize()`.
2. **Deny List**:
   - Host `/proc` and `/sys` cannot be bound raw (causes kernel telemetry leak and potential PRoot hang).
   - Host `$PREFIX/etc/passwd` cannot be bound directly over guest authentication tables.
3. **Android Storage Isolation**:
   - Warn or guard against binding `/storage/emulated/0` (shared SD card) with execute bits unless `exec` emulation is explicitly requested.

---

## 3. Subsystem C: Guest Environment & Sysdata Emulation

### 3.1 The 6 Sysdata Vectors (Findings 11-6 through 11-11)
Swarm audit Subagent 11 isolated 6 subtle runtime breaks in guest processes (`systemd-detect-virt`, `htop`, `neofetch`, `apt`, `node`):

#### Vector 1: `/proc/mounts` Reality Drift (11-1)
- **Current State**: Empty or raw host mounts showing Termux internal private mount namespace (`/dev/block/dm-... on /data ...`).
- **Failure**: Package managers (e.g. `dpkg`, `pacman`) fail disk space checks (`df -P /`) because the root filesystem `/` is not represented as a writable mount point.
- **v0.2.0 Fix**: Synthesize a standard Linux VFS table:
  ```text
  rootfs / rootfs rw 0 0
  proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
  sysfs /sys sysfs rw,nosuid,nodev,noexec,relatime 0 0
  tmpfs /dev tmpfs rw,nosuid,relatime,mode=755 0 0
  devpts /dev/pts devpts rw,nosuid,noexec,relatime,gid=5,mode=620 0 0
  tmpfs /dev/shm tmpfs rw,nosuid,nodev,relatime,size=65536k 0 0
  tmpfs /tmp tmpfs rw,nosuid,nodev,relatime,size=65536k 0 0
  ```

#### Vector 2: `/etc/resolv.conf` Desynchronization (11-2)
- **Current State**: Container creation copied host `resolv.conf` once. When the mobile device toggles between Wi-Fi and Cellular (LTE/5G), DNS nameservers change, rendering guest network dead.
- **v0.2.0 Fix**: Dynamic injector on every `login`/`run`:
  Inspect Android system DNS via `getprop net.dns1` or fallback to quad-eight (`8.8.8.8`, `1.1.1.1`), atomic write to `<guest>/etc/resolv.conf` before launching PRoot.

#### Vector 3: Android UID / Host Leak (11-3)
- **Current State**: `/etc/passwd` inside guest maps UID 0 to `root`, but Android SELinux context and `/proc/<pid>/status` leak `u0_aXXX` identity.
- **v0.2.0 Fix**: Bind a sanitized `/proc/stat` and `/proc/version` via PRoot `-b /path/to/fake_proc:/proc`.

#### Vector 4: Sysfs Skeleton (`/sys`) & IPC (11-6)
- **Current State**: Missing `/sys` causes `udev`, `mesa`, and system diagnostic tools to crash.
- **v0.2.0 Fix**: Provide a synthetic `sysfs` tree containing minimal nodes:
  - `/sys/class/net/`
  - `/sys/devices/system/cpu/`
  - `/sys/kernel/mm/`

---

## 4. Subsystem D: Automation Contract & Unified Exit Codes

### 4.1 Exit Code Taxonomy
To make PDN reliable for automated agents, CI, and shell scripts, exit codes must never be ambiguous:

| Exit Code | Meaning | Guarantee |
| :---: | :--- | :--- |
| **0** | Success | Operation completed exactly as requested; standard JSON emitted if `-f json`. |
| **1** | Domain / Application Error | Entity not found, guest process returned non-zero, or validation rejected. |
| **2** | CLI / Usage Error | Invalid syntax, missing required flag, clap validation error. |
| **3** | Configuration / State Corrupt | Config file invalid, permissions denied on PDN home, lock collision. |
| **4** | Runtime / PRoot Execution Error | PRoot binary missing, architecture unsupported, kernel refused ptrace. |
| **126** | Command Invoked Cannot Execute | Guest binary found but permission denied / format invalid (e.g. signal 4). |
| **127** | Command Not Found | Guest binary does not exist in `$PATH`. |
| **128+N**| Terminated by Signal | Guest killed by signal `N` (e.g. 137 = SIGKILL, 139 = SIGSEGV, 143 = SIGTERM). |

### 4.2 Stream Separation (stdout vs stderr)
In automated mode (`--format json` or `-f json`):
- `stdout`: **STRICTLY PURE JSON**. No progress bars, no human warnings, no colored text.
- `stderr`: Human telemetry, logging, spinners, warnings, and error diagnostics.

---

## 5. Diagnostic Toolkit: `podnext doctor` Blueprint

To eliminate user friction and debugging blindness on Termux, v0.2.0 will include `podnext doctor`:

```
$ podnext doctor
[✓] Architecture: aarch64 (supported)
[✓] Android OS: 16 (API 36) - Linux 6.6.58
[✓] Termux Prefix: /data/data/com.termux/files/usr (accessible, rwx)
[✓] Temp Directory: /data/data/com.termux/files/usr/tmp (compliant, rwx)
[✓] PRoot Engine: /data/data/com.termux/files/usr/bin/proot (v5.4.0+, functional)
[✓] ptrace Scope: permitted (PTRACE_ATTACH / PTRACE_TRACEME functional)
[✓] Storage Subsystem: ~/.local/share/podnext (healthy, 4.2 GB free)
[✓] DNS Resolution: 8.8.8.8, 1.1.1.1 (reachable)
[!] Seccomp Filter: BPF JIT active (Android 16 strict mode detected; link2symlink recommended)
Status: Operational (1 recommendation)
```

---

## 6. Execution Phasing & Dependency Graph

```
v0.1.9 (Immediate Target)
├── Critical C1: Argv non-UTF8 (#41) [SHIPPED]
├── Critical C2: Image pull honesty [SHIPPED]
├── Batch A: Storage & volume validation
├── Batch B: Automation contract (JSON stdout, TTY guard, exit codes)
├── Batch C: Guest DNS & mounts baseline
├── Batch D: Documentation cleanup (eradicate phantom flags/commands)
└── Ledger Update: 75 PASS verified

          │
          ▼ [Tag v0.1.9 Release]
          │

v0.2.0 "Mobility Target" (Deep Engineering)
├── Phase 1: `pn-image-store` crate (real OCI layer unpack + whiteouts)
├── Phase 2: Login polygon extension (18 flags, shared-home, custom binds)
├── Phase 3: Sysdata virtualization (/proc, /sys stubs, group masks)
├── Phase 4: `podnext doctor` and diagnostic engine
└── Phase 5: Export / Import tarball portability & `pd` shell wrapper
```
