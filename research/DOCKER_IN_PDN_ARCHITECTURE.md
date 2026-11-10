# Architecture: Docker Integration in PDN

Status: **Design specification**. Every claim about hardware limits and `uml4termux` comes from measured benchmarks and cited sources. No claim here is conjecture.

---

## 1. Measured Deterministic Constraints

PDN currently runs containers with **PRoot**. PRoot uses `ptrace` chroot emulation. PRoot is sufficient for basic filesystems and process execution. PRoot cannot run Docker. The measurements below come from this device. The user ID is 10396 and the user is unprivileged:

| Docker Requirement | Measured Outcome on Android | Evidence |
|---|---|---|
| `unshare(CLONE_NEWUSER)` | `EINVAL` | Direct syscall execution |
| `unshare(CLONE_NEWNS/NEWPID/NEWNET)` | `EPERM` | Direct syscall execution |
| `mount(2)` for overlayfs | **Process terminated with SIGSYS (31)** by Android seccomp | Direct syscall execution |
| `CAP_SYS_ADMIN` / `CAP_NET_ADMIN` | `CapEff = CapPrm = CapBnd = 0` | `/proc/self/status` inspection |
| Write access to `/sys/fs/cgroup` | Exists, but `touch` returns `Permission denied` | File write probe |
| Host `dockerd` binary | Absent; only client `docker` v24.0.6 exists | `command -v dockerd` |

PRoot **spoofs** `mount` and `unshare`. It returns `rc=0` and does no real VFS virtualization. The result is dangerous. `dockerd` appears to start inside a PRoot container, but the next container initialization fails completely. Direct Docker execution inside PRoot is not only difficult. It is **architecturally flawed**.

The only viable execution path on Android is a real Linux kernel in user space. That kernel is `uml4termux`, which is User-Mode Linux for arm64. The project is already validated on this identical hardware. It runs `dockerd` 20.10.24 with containerd and runc. The storage driver is `overlay2`, with `Native Overlay Diff: true` and `Backing Filesystem: extfs`. The cgroups v2 controllers are `cpu io memory pids`. The project supports `docker pull/run/exec/logs`, inter-container networking on `docker0`, port publishing with `gvproxy`, Compose v2.29.7, and BuildKit.

---

## 2. Architectural Decision

**PDN does not reimplement Docker. PDN introduces a polymorphic secondary runtime backend.**

```
                    ┌──────────────────────────────┐
   podnext CLI ────► │  RuntimeBackend (trait)      │
   podnext-daemon   │                              │
                    ├──────────────┬───────────────┤
                    │ ProotBackend │  UmlBackend   │
                    └──────┬───────┴───────┬───────┘
                           │               │
                   ptrace chroot     uml4termuxd (UNIX socket)
                   (no namespaces)    └─ UML kernel → dockerd → containers
```

Rationale:

- **Do NOT** reimplement Docker. That is impossible without kernel namespaces. Custom builders and overlay emulation on PRoot always crash at the first container start.
- **Do NOT** invoke `buildctl` or `podman` on the host. Both tools need kernel namespaces. Neither tool can run unprivileged on Android.
- **Do NOT** remove `build` from the CLI. Developers need local image builds. The correct solution is a backend that can execute them.

A backend is configured for each container. The `runtime` field in `metadata.json` holds the choice. A CLI flag can override it: `--runtime uml`. The default stays `proot` for low latency and zero virtualization overhead. You select `uml` only when a workload needs a real Linux kernel.

---

## 3. Daemon Wire Protocol for `uml4termuxd`

`uml4termuxd` uses **newline-delimited text** on a UNIX domain socket inside `$UML4TERMUX_RUNTIME`. The daemon sends one response and then closes the connection:

| Request | Response |
|---|---|
| `protocol` | `{"protocol":N,"runtime_contract":N,"commands":[...]}` |
| `status` | `{"state":"running\|idle\|completed","child_pid":N,"restarts":N,"kernel":"...","mem":"...","ncpus":"...","uptime_s":N}` |
| `exec <cmd>` | Output wrapped in `__EXEC_<seq>_BEGIN__ ... __EXEC_<seq>_DONE_$?` |
| `stop` | `{"state":"stopping"}` |

Protocol constraints:
- `exec` is **rate-limited** and **length-bounded**. It answers `{"error":"rate-limited","min_interval_ms":N}` for a rate limit. It answers `{"error":"command too long"}` for an overlong command. Both conditions give explicit errors.
- A socket has mode `0600`. The daemon checks it with `SO_PEERCRED`. Every request goes into `audit.log`.
- A client must complete the `protocol` handshake first. Then it can send commands.

---

## 4. Subcommand Mapping to Backends

| PDN Subcommand | Proot Backend Behavior | UML Backend Behavior |
|---|---|---|
| `login` / `run` | PRoot ptrace emulation | `exec` into UML guest shell |
| `build` | Rejects `RUN` directives | Executes `DOCKER_BUILDKIT=1 docker build`, then extracts layers via `docker save` |
| `docker` (passthrough) | Rejected with explanatory notice | Direct execution: `exec "docker <args>"` |
| `network connect` | Writes to guest `/etc/resolv.conf` | Native `docker network connect` inside UML guest |
| `volume` | Host directory bind mount | Native `docker volume` with Docker reference counts |
| `quota` | JSON state + `collect_usage` | Mapped to `--cpus`/`--memory` (`cpu.max`/`memory.max` verified: OOM 137 on `--memory 64m`) |

---

## 5. Measured Physical Limits in UML Mode Too

The constraints below are hardware and architecture invariants:
- `io.max` cgroup. The guest kernel cgroup v2 exposes only `io.stat`. It cannot enforce `--device-*-bps`.
- Swap accounting. This feature is disabled in the guest kernel.
- The memory cgroup charge for tmpfs or page-cache. `CONFIG_MEMCG_KMEM` is disabled.
- Multi-CPU operation in `ptrace` mode. The mode gives one CPU only. It needs `seccomp=on`.
- Inbound forwarding with `passt` or `umnet`. These tools give outbound traffic only. The supported inbound path is `gvproxy` plus `POST /services/forwarder/expose`.
- Native i386 or armv7 execution. `CONFIG_COMPAT` is disabled. This mode needs `qemu-user-static` inside the guest.

---

## 6. Target Code Structure

```rust
// crates/proot-distro-core/src/runtime.rs
pub enum RuntimeKind { Proot, Uml }

pub trait RuntimeBackend: Send + Sync {
    fn kind(&self) -> RuntimeKind;
    fn supports_kernel_features(&self) -> bool;
    async fn exec(&self, container: &str, argv: &[String]) -> Result<ExitStatus>;
    fn status(&self) -> Result<RuntimeStatus>;
}
```

- `ProotBackend`: A direct migration of the current `container.rs::execute`.
- `UmlBackend`: A thin client to `uml4termuxd.sock`. It completes the `protocol` handshake first. Then it sends `exec` and retries after a rate limit.
- Selection: the file `metadata.json` holds `"runtime": "proot" | "uml"`. The flag `--runtime` overrides the value.
- On the `proot` backend, `podnext docker ...` refuses the request with a clear message: `"requires the uml runtime: docker needs namespaces; see 'podnext runtime info'"`.

---

## 6a. Safe Dockerfile Subsets That Exclude `RUN`

A study of the Docker image build pipeline shows one point. Only **command execution** needs namespaces. Artifact assembly does not. `RUN` needs `fork` plus `exec` inside a mount namespace. For `unshare` the kernel returns EPERM. For `mount(2)` it returns SIGSYS. The other directives are tar operations and JSON changes only. These directives are `FROM`, `COPY`, `ADD`, `ENV`, `LABEL`, `WORKDIR`, `USER`, `EXPOSE`, `CMD`, and `ENTRYPOINT`.

**ImageManager Fixes Applied**:
1. `diff_ids` now holds uncompressed tar SHA-256 hashes, not compressed layer digests.
2. Config serialization sets the real `size` and computes the SHA-256 `digest`.
3. Push operations reject missing blobs before they send manifests to registries.

---

## 7. Phased Implementation Plan

1. **`RuntimeBackend` + `ProotBackend`**: A pure refactoring. All 128 tests pass with unchanged CLI semantics.
2. **Minimal `UmlBackend`**: It implements `protocol`, `status`, and `exec`. It proves that `uname -a` returns the UML kernel.
3. **`podnext docker` Passthrough**: This path is enabled on the UML backend only. Example: `podnext docker run --rm hello-world`.
4. **Production `build`**: It runs `docker build` inside the UML guest and exports real OCI layers.
5. **Mapping Subsystems**: It bridges `network`, `volume`, and `quota` to native Docker resources when `backend = UML`.
