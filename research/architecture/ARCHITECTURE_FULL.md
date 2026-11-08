# PDN (podnext) — full architecture, components and mechanisms

Purpose: a complete map of the system as it *is*, so that no layer, module or mechanism can be
forgotten or mistaken for absent. Every number here is counted and every claim carries a file path or a
measured command; where something is a stub, unreachable or unverified, this document says so instead of
implying it works.

Companion artifacts, all generated from source rather than written from memory:

| artifact | contents | how to regenerate |
|---|---|---|
| `research/architecture/INVENTORY_RAW.md` | every `.rs` file with line counts, `pub fn` / `pub struct` / `pub use` counts and doc-comment counts | the python walker in this document, §9 |
| `research/architecture/MODULE_MAP.md` | every `//!` module doc, plus every public item with its first doc line (`-` where undocumented) | same walker |
| `research/architecture/DIGEST_ONE_LINE.txt` | one line per file: path, size, module doc first sentence | same walker |

Measured state at commit `48e86b7`: **6 crates + a root test crate, 188 `.rs` files, 72,257 lines**,
1,877 public items, 3 files with no module doc (§8).

---

## 1. The layer model

PDN is a stack, and each layer has exactly one job. Read top-down: each row may call the rows below it,
and nothing may call upward.

| # | layer | crate / binary | job | why the layer exists |
|---|---|---|---|---|
| 6 | user surface | `podnext` (bin), `podnext-daemon` (bin) | CLI parsing, command dispatch, rendering; the daemon process | the only layer a user sees |
| 5 | orchestration | `pn-next` | services, supervisor, watchdog, quotas, policy, network, volumes, D-Bus, socket activation, dashboard, cluster, GitOps, CRI, multi-arch build | everything that outlives a single container |
| 4 | bridge / fail-safe | `pn-gateway` | gateway config, guardrails, health, metrics, fail-safe manager | the layer that decides whether to keep going |
| 3 | container lifecycle | `proot-distro-core` | store layout, OCI install, extraction, sessions, locks, blob store, l2s, sysdata, refcount, dirfd | knows what a "container" is |
| 2 | Android/kernel compatibility | `proot-compat` | CAP_SYS_ADMIN detection, mount helpers, platform personality (uname), cgroup v2 shim, seccomp bypass list, stub files, path translation | bridges Linux assumptions to Android |
| 1 | execution engine | `proot-core` | ptrace controller, tracee state, syscall table, VFS, path translation, identity table, seccomp, selective interception | must make a foreign rootfs run |
| 0 | host kernel | Termux `proot` binary (external) | actual syscall interception today | see §7 — PDN *orchestrates* this binary rather than replacing it |

Dependency edges, read from each `Cargo.toml` (no crate depends on a crate above it):

```
proot-core          -> (no internal dependencies)
proot-compat        -> proot-core
proot-distro-core   -> proot-core, proot-compat
pn-gateway          -> proot-core, proot-distro-core, proot-compat
pn-next             -> proot-core, proot-distro-core, proot-compat, pn-gateway
podnext             -> pn-gateway, pn-next, proot-distro-core, proot-core, proot-compat
```

Two binaries come out of `crates/podnext`: `podnext` (`src/main.rs`) and `podnext-daemon`
(`src/daemon.rs`).

---

## 2. `proot-core` — the execution engine (18 files, 4,598 lines)

Module docs (verbatim first line) and what each one is for:

| file | lines | role |
|---|---|---|
| `lib.rs` | 679 | "PRoot Core - Rust Implementation. A ptrace-based chroot, mount --bind, and binfmt_misc implementation without …" — owns `ProotEngine`, `ProotConfig`, and both execution paths (§8) |
| `syscall.rs` | 1395 | "System call handler for PRoot" — 155 handler patterns (see §9.2: the bodies are placeholders) |
| `tracee.rs` | 327 | "Tracee management for PRoot" — per-tracee state, `read_syscall` / `read_retval` |
| `selective.rs` | 246 | "Selective seccomp interception backend (S1)" — `run_traced`, `run_selected_cmd`; the one interception path that is exercised by tests |
| `seccomp.rs` | 211 | "Seccomp filter for PRoot" — installs the BPF filter |
| `arch.rs` | 205 | "Architecture definitions for PRoot" — register layout per architecture (`x8`, `orig_x0` on aarch64) |
| `ptrace.rs` | 196 | "Ptrace controller for PRoot" — `PtraceController`, `PtraceEvent`, `run_loop` |
| `extension.rs` | 188 | "Extension system for PRoot" |
| `cli.rs` | 147 | "CLI configuration for PRoot" — mirrors the `proot` command line (`#[command(name = "proot")]`) |
| `tracee_state.rs` | 135 | "Tracee state management for PRoot" |
| `vfs.rs` | 133 | "Virtual File System (VFS) manager for PRoot" |
| `compat.rs` | 86 | "Compatibility layer for PRoot" |
| `identity.rs` | 84 | "The identity and credential syscall table (adoption S2)" |
| `path.rs` | 69 | "Path translation for PRoot" |

Tests that pin its facts: `tests/argv_facts.rs` (what actually reaches the `proot` argv),
`tests/identity_table.rs` (the identity table is "countable, complete and honest"),
`tests/seccomp_filter.rs` (A21: `install()` must deny syscalls or tell the caller it did not),
`tests/selective_seccomp.rs` (S1 backend measured against the PRoot path).

---

## 3. `proot-compat` — the Android bridge (9 files, 2,002 lines)

| file | lines | role |
|---|---|---|
| `security.rs` | 977 | "Security hardening for PRoot containers" — seccomp profile management, AppArmor/SELinux integration |
| `cap_sys_admin.rs` | 569 | "CAP_SYS_ADMIN capability detection and filesystem mounting with fallbacks" |
| `path_translation.rs` | 115 | symlink-chain resolution (e.g. `usr/local/bin/chroot`) |
| `platform_shim.rs` | 64 | "Mengubah uname() untuk melaporkan `Linux` bukan `Android`" — makes `process.platform` and friends see Linux |
| `seccomp_bypass.rs` | 64 | Android-specific syscalls allowed through so they are not double-blocked |
| `stub_fs.rs` | 58 | stub files/dirs inside the guest for Chrome-headless-style compatibility |
| `mount_helper.rs` | 57 | automatic `/proc`, `/sys`, `/dev`, `/run`, `/tmp` inside the guest rootfs |
| `cgroup_v2.rs` | 42 | cgroup v2 setup inside the guest rootfs |
| `lib.rs` | 56 | crate root and re-exports |

---

## 4. `proot-distro-core` — container lifecycle (41 files, 13,969 lines)

Grouped by responsibility, with the file's own description:

**Store, layout and identity**

| file | lines | role |
|---|---|---|
| `container.rs` | 1081 | "Container management … lifecycle management with proot-core integration" — `Container`, `ContainerConfig`, `get_backend` (§8) |
| `paths.rs` | 396 | "Path management" — where the store, containers and runtime dirs live |
| `names.rs` | 398 | "Name validation and management" |
| `constants.rs` | 114 | "Constants" |
| `guestdirs.rs` | 110 | runtime directories beside a container's rootfs that are bound into the guest |
| `container_index.rs` | 289 | "On-disk cache of the container store listing (roadmap 9.2)" — the `list` fast path |
| `refcount.rs` | 471 | "Reference-counted eviction for the store (WORKLIST C6)" |
| `blobstore.rs` | 207 | "A content-addressed store for OCI layer blobs" |

**Install and extraction**

| file | lines | role |
|---|---|---|
| `untar.rs` | 642 | "Archive extraction that survives Android" — hard links become copies; special files are skipped rather than failing |
| `archive.rs` | 375 | install a rootfs from a local tarball or an OCI image-layout tar |
| `image.rs` | 573 | "Image management" — manifest and layer handling, `entrypoint`/`working_dir` fields |
| `layer_diff.rs` | 328 | "OCI layer differ for the PDN container image builder" |
| `dockerfile.rs` | 381 | "Dockerfile parser for PDN image builds" |
| `compress.rs` | 204 | compression utilities |
| `install.rs` | 66 | installer entry |
| `signature.rs` | 476 | "Image signature verification" — cosign/notary/simple (see §9.2: default disabled, verifier unused) |

**Execution, sessions and locks**

| file | lines | role |
|---|---|---|
| `runtime.rs` | 564 | "Unified Container Runtime Backend Abstraction" — `RuntimeBackend` trait, `ProotBackend`, `UmlBackend`, `RuntimeKind` |
| `session.rs` | 668 | "Session management" — the session record, liveness, exit rules |
| `locking.rs` | 398 | "Locking mechanism" — container locks (shared/exclusive) and build locks |
| `login.rs` | 84 | login entry |
| `bindings.rs` | 189 | bindings management |
| `android_binds.rs` | 653 | "The Android bind set a session is given — and the termux-type exclusion" |
| `shm.rs` | 210 | `/dev/shm` management (mode 1777 for V8/Chromium) |
| `sysdata.rs` | 745 | "Fake `/proc` entries for containers" — the synthetic `stat`, `loadavg`, `uptime`, `mounts` |
| `guestfile.rs` | 118 | guest file access |
| `dirfd.rs` | 329 | "Directory file descriptor management and hardened descriptor VFS" — `openat` with `O_NOFOLLOW`/`O_NONBLOCK` (see §9.2: not referenced outside its module) |
| `l2s.rs` | 167 | "Link2Symlink Tree Rewriter for PRoot containers" |
| `gpu.rs` | 312 | GPU device detection and passthrough |

**Declared stubs inside this crate** (their own words):
`network.rs` (349) "Network module - Stub for future implementation",
`storage.rs` (317) "Storage/Volume module - Stub for future implementation",
`security.rs` (208) "re-export from proot-compat".
`message.rs` (81) and `progress.rs` (140) are support utilities.

Tests: `tests/atomic_identity.rs` (B11), `tests/blob_store_dedup.rs`, `tests/deep_fs_recursion.rs`,
`tests/single_writer_races.rs` (A29), `tests/store_fifo_is_metadata.rs` (S3), `tests/store_integrity.rs`
(A1/A3a/A4).

---

## 5. `pn-gateway` — bridge and fail-safe (12 files, 3,555 lines)

| file | lines | role |
|---|---|---|
| `config.rs` | 771 | "Gateway configuration" |
| `guardrails.rs` | 411 | "Guardrail manager" — `Deny` is enforced; the other actions are not (§9.2) |
| `health.rs` | 370 | health monitors (TCP/HTTP) |
| `metrics.rs` | 300 | metrics collector (the fabricated figures live in `pn-next/src/web_dashboard/mod.rs:801`, §9.2) |
| `fail_safe.rs` | 201 | "Fail-safe manager" |
| `gateway.rs` | 178 | gateway module |
| `duration_fmt.rs` | 204 | human-readable durations in config files |
| `config_loader.rs` | 96 | TOML loading |
| `lib.rs` | 37 | "Bridge layer between PRoot, PRoot-distro, and proot-compat" |

Tests: `tests/config_loader.rs` (issue #27 follow-up) and `tests/doc_key_drift.rs` — a whole-file
documentation ↔ configuration drift guard, i.e. this project already treats doc/code drift as a defect
class worth a test.

---

## 6. `pn-next` — orchestration (39 files, 26,123 lines)

This is the largest crate and the one with the most hidden depth: **17 subsystem directories plus 12
top-level modules**. Nothing below is inferred from the specification — each row is the file's own
module doc.

### 6.1 Subsystem directories

| directory | files / lines | first doc line |
|---|---|---|
| `supervisor.rs` (top-level) | 2989 | "Service Supervisor - runit/s6-style supervisor for container services" |
| `dbus/` | `mod.rs` 1370, `services.rs` 507, `zbus_client.rs` 688 | "D-Bus Broker - Inter-container IPC and service discovery" · "D-Bus Service Definitions for PN Next Inter-Container Communication" · "ZBus-based D-Bus Client" |
| `backup_restore/` | 1617 | "Backup/Restore Module. Container snapshots and disaster recovery for PDN" |
| `api.rs` | 1422 | "API server for PN Next. Provides a RESTful API for container management with real integration to proot-distro-…" — dead code today (§9.2) |
| `daemon_runtime/` | 1139 | "Daemon Runtime - Independent lifecycle manager for services" |
| `multiarch_build/` | 1141 | "Multi-arch Build Service Module. Cross-compilation and multi-architecture build service" |
| `gitops/` | 1067 | "GitOps Integration Module. Flux/ArgoCD style GitOps integration for PDN" |
| `cluster/` | 889 | "Multi-node Clustering Module. Distributed scheduling and cluster management" |
| `structured_logging/` | 872 | "Structured Logging Module. Journald-style logging with rotation, querying, and filtering" |
| `policy_engine/` | 758 | "Policy Engine Module. Declarative policy evaluation for service management decisions" |
| `watchdog/` | 736 | "Watchdog Integration Module. Auto-restart failed containers and services with policy-based recovery" |
| `socket_activation/` | 756 | "Socket Activation - systemd-style on-demand container/service start" |
| `resource_quota/` | 838 | "Resource Quota Module. Enforces resource limits independent of container limits via supervisor" |
| `container_socket_activation/` | 612 | "Socket Activation for Containers Module. Extends socket activation to support on-demand container startup" |
| `wake_lock/` | 435 | "Wake Lock Management Module. Keeps services alive during Android doze mode" |
| `web_dashboard/` | 829 | "Web UI Dashboard Module. Real-time monitoring and management dashboard" |
| `cgroup_view.rs` | 369 | "A private cgroupfs view (adoption S8 of `research/DISSECTION_SHROOT.md`)" |
| `termux_boot/` | 278 | "Termux:Boot Integration Module" |
| `cri_plugin/` | 177 | "Kubernetes CRI Plugin Module (Stub Implementation)" — self-declared stub |
| `readiness.rs` | 186 | "Start coordination as one byte on a pipe (WORKLIST C10)" |
| `restart_policy.rs` | 133 | "B14 - the restart decision, as a pure module" |
| `runtime.rs` | 1701 | "Runtime management and container lifecycle service" |
| `network.rs` | 705 | "Network management … CNI (Container Network Interface) plugin supp…" |
| `plugin.rs` | 493 | "Plugin system … dynamic plugin loading via shared libraries (.so/.dylib/.dll)" |
| `storage.rs` | 180 | "Storage management" |
| `scheduler.rs` | 194 | "Scheduler" |
| `security.rs` | 151 | "Security management" |
| `workflow.rs` | 274 | "Workflow engine" |
| `lib.rs` | 351 | crate root; `pub struct PnNext` lives here and is never constructed outside this file (§9.2) |

### 6.2 pn-next test surface

`tests/supervisor_e2e.rs` (821), `tests/cgroup_view.rs` (336), `tests/runtime_cgroup_honesty.rs` (A27),
`tests/cgroup_honesty.rs` (A20), `tests/cgroup_throttle_honesty.rs` (A20 negative control),
`tests/readiness_handshake.rs` (C10), `tests/restart_policy.rs` (B14), plus two benchmarks.

---

## 7. `podnext` — CLI and daemon (65 files, 20,307 lines)

### 7.1 Command-line surface

| file | lines | role |
|---|---|---|
| `main.rs` | 479 | "podnext CLI - Advanced container and service management for Termux/Android" |
| `cli.rs` | 767 | "Clap command surface for the podnext CLI" |
| `cli/common.rs` | 78 | "Shared value enums for the CLI surface" |
| `cli/service.rs` | 219 | `service`, `daemon`, `watchdog`, `quota`, `log` families |
| `cli/system.rs` | 218 | "D-Bus and socket IPC, Termux boot and wakelock, the policy engine, gateway conf…" |
| `cli/image.rs` | 78 | local OCI image cache |
| `cli/backup.rs` | 51 | snapshot management |
| `cli/network.rs` | 62 | isolated bridge networks |
| `cli/build.rs` | 76 | local image builder |
| `cli/volume.rs` | 57 | persistent volumes |
| `dispatch.rs` | 125 | "one `match` from `Commands` to the command implementation that runs it" |
| `subcommands.rs` | 208 | "commands that print rather than orchestrate: host diagnostics, the version table, shell completions" |
| `utils.rs` | 69 | utility functions |

### 7.2 Command implementations (`commands/`)

| file | lines | role |
|---|---|---|
| `mod.rs` | 880 | "CLI command implementations for service orchestration" |
| `container.rs` | 757 | install, list, login, run, ps, sessions, attach, docker, kill, remove, info |
| `image.rs` | 757 | "Image and registry commands: `podnext image …` and `podnext search`" |
| `service.rs` | 739 | "The `service` and `daemon` supervision commands" |
| `sync.rs` | 586 | "Container filesystem commands: `podnext copy`, `sync`, `reset`, `rename` and the hole-aware copy machinery" — the file at the centre of the write-escape finding |
| `build.rs` | 732 | materialise a Dockerfile into the local image store |
| `gitops.rs` | 460 | "`podnext gitops` and `podnext dashboard` command implementations" |
| `quota.rs` | 318 | "Resource quota commands: set, get, usage, list" |
| `volume.rs` | 273 | volume commands |
| `network.rs` | 247 | network commands |
| `dbus.rs` | 245 | "`podnext dbus serve`, `list`, `call` and `monitor`" |
| `policy.rs` | 197 | "Policy-engine commands: eval, list, load, presets" |
| `log.rs` | 191 | "Structured log commands: query, tail, stats, flush" |
| `power.rs` | 153 | "`boot` and `wakelock` supervision commands" |
| `watchdog.rs` | 115 | watchdog commands |
| `socket.rs` | 44 | "Socket-activation commands: register, unregister, list" |
| `compose.rs` | 36 | "export PDN service configuration to a Docker Compose file" |

### 7.3 Native execution path (`native/`)

| file | lines | role |
|---|---|---|
| `image.rs` | 984 | "The OCI / Docker Hub install path: registry search, manifest and layer fetch, and `install_image_native`" — also spawns the external `proot` binary at `:131` |
| `session.rs` | 747 | "The container session path: foreground login and run, signal forwarding to the guest, and the detached-session…" |
| `meta.rs` | 299 | listing, the image metadata re-apply (`apply_image_runtime_config`), mount-spec helpers |
| `retry.rs` | 141 | "The transfer policy for the registry client (WORKLIST B2 and B3)" |
| `digest.rs` | 41 | "Digest validation before path composition (WORKLIST B1)" |

### 7.4 Daemon, direct mode, diagnostics

| file | lines | role |
|---|---|---|
| `daemon.rs` | 595 | "podnext-daemon - Persistent service supervisor daemon" (this is the `podnext-daemon` binary) |
| `direct.rs` | 412 | "Direct-execution mode: `podnext -r <rootfs> -- <cmd>`" — silently drops unknown flags (§9.2) |
| `doctor.rs` | 789 | "`podnext doctor` - a host and guest diagnostic that reports what it measured" |

### 7.5 podnext test surface (44 files under `tests/` plus examples)

The crate carries a contract suite, not just unit tests. Most relevant here:

- `cli_reference_contract.rs` (328) — every command and documented option in `docs/src/cli-reference.md` must exist.
- `exit_codes.rs` (229) and `quoting_table.rs` (165) — the exit-code and argv-quoting contracts.
- `stub_refusals.rs` (157) — "a command that cannot do its job must refuse, name the reason, and exit non-zero".
- `stdout_discipline.rs` (396) — "stdout is the data channel, and a page belongs to the command that owns it".
- `session_registry.rs` (768), `session_reattach_parity.rs` (279) — the session model and containerd-shim parity.
- `install_interrupt.rs` (865), `install_lock.rs` (176), `install_sources.rs` (511), `remove_atomic.rs` (290) — lifecycle and atomicity.
- `orphan_reaping.rs` (262) — "PDN is a real subreaper, so the descendants of a dead guest are adopted and reaped".
- `image_config_env.rs` (608) — the image's own config applied at `login` (issue #76) and the kill ladder (B8).
- `daemon_honesty.rs` (220) — "`daemon status` must not claim a running daemon when there is none".
- `pull_hygiene.rs` (234), `refcount_eviction.rs` (212), `relocated_paths.rs` (93), `doctor_capabilities.rs` (185), `doctor_contract.rs` (129), `inspect_missing.rs` (64), `argv_nonutf8.rs` (58), `build_copy_escape.rs` (149), `guest_identity_probe.rs` (49), `automation_contract.rs` (142, no module doc).
- Root crate `tests/`: `daemon_integration_test.rs` (375), `daemon_client_e2e.rs` (48), `integration/common.rs` (664), `integration/v2_criteria.rs` (616) — the v2.0 success criteria.

---

## 8. Cross-cutting mechanisms (what actually happens at runtime)

Each mechanism below names the file and line that implements it, plus the measurement that proved it.
Where a mechanism is declared but not reachable, that is stated here and again in §9.2.

### 8.1 The engine is an external `proot` binary — PDN orchestrates it

This is the single most consequential architectural fact, and it is invisible from the crate names.

* Resolution order lives in `crates/proot-core/src/lib.rs:79-91`: `PROOT_BIN` environment variable →
  `$dir/proot` next to the current executable → the literal name `proot` resolved from `PATH`.
* Spawn sites: `crates/proot-core/src/lib.rs:195` and `:337`, and `crates/podnext/src/native/image.rs:131`.
* The host binary at `/data/data/com.termux/files/usr/bin/proot` is 259,784 bytes, mode `0700`.
  `proot --version` prints only ASCII art and a 2015 copyright — **it prints no version number**; the
  version `5.1.107.101` appears only in a source comment at `crates/proot-core/src/lib.rs:187-195`.
* Measured process tree for `podnext run`: pid 4169 (`podnext`) → pid 4188
  (`/data/data/com.termux/files/usr/bin/proot --kill-on-exit --link2symlink --sysvipc -L --change-id=0:0:0 --rootfs=. --cwd=/`).
* Measured failure mode: `PROOT_BIN=/nonexistent/proot` → `os error 2`, no fallback to the internal engine.
* Consequence: `pkg install proot` is a hard prerequisite that cannot be pinned in any `Cargo.toml`.
  `podnext doctor` is the only place that reports the binary it found.

The six argv features PDN borrows from the external binary rather than implementing: `--rootfs`/`--cwd`,
`--bind`, `--change-id`, `--kill-on-exit`, `--link2symlink`, `--sysvipc`.

### 8.2 Guest environment construction

`LANG`, `LC_ALL` and `COLORTERM` are forwarded from the host unconditionally in both execution paths
(`crates/proot-core/src/lib.rs:260` in `start`, `:411` in `execute` — the same three-variable list in both). Measured consequence: host
`LANG=en_US.UTF-8` with an empty host `locale -a`; the guest rootfs ships only `C`, `C.utf8` and `POSIX`;
inside the guest every Perl-based tool prints `perl: warning: Setting locale failed … Falling back to the
standard locale ("C")` — reproduced under both `podnext login ubuntu` and `podnext login ubuntu --isolated`.
Negative control: `LANG=C.UTF-8` produces no warning, so the variable is the cause and not the guest image.
`--isolated` warns identically, i.e. the flag does not isolate the environment.

### 8.3 First entry (`login`) and identity

Measured: `login` lands in `/` because the argv carries `--cwd=/`; `USER` and `SHELL` are empty inside;
`PS1` is `"# "`; the argv has no `-l`, so `/etc/profile` is never read. The reference implementation
resolves the user from `/etc/passwd` and sets `login_wd = login_home`
(`/data/data/com.termux/files/usr/lib/python3.14/site-packages/proot_distro/commands/login/__init__.py:388`),
exporting `HOME`/`USER` per session and the identity variables `PD_CONTAINER`, `PD_IMAGE`, `PD_IMAGE_ID`
and `container=proot-distro` — none of which PDN sets.

### 8.4 Store layout, index cache, refcount and blob store

* Container rootfs: `~/.podnext/containers/<name>/rootfs`; per-container metadata `metadata.json`;
  runtime directories beside the rootfs are computed by `crates/proot-distro-core/src/guestdirs.rs`;
  the whole layout is centralised in `paths.rs`.
* `container_index.rs` is an on-disk cache of the store listing that makes `list` cheap
  (roadmap 9.2). Measured: `list` median 23.2–25.3 ms, while `podnext --version` alone costs 17–24 ms —
  i.e. the listing work itself is only ~1–3 ms.
* `refcount.rs` implements reference-counted eviction (WORKLIST C6) over the blob store
  `blobstore.rs`; `layer_diff.rs` computes OCI layer differences for the builder.
* Measured integrity behaviour: a FIFO planted at a container's `metadata.json` is classified as
  *metadata* (S3) and freezes `list`/`remove` (`rc=124` at the 20 s timeout, leaving orphan processes).

### 8.5 Locking

`crates/proot-distro-core/src/locking.rs` provides shared and exclusive container locks. Measured: an
exclusive `flock` attempt while another holder exists is refused with
`EX_REFUSED_shared_lock_is_held`; and `remove` deletes the container while leaving `fs-lock5.lock`
behind in the store.

### 8.6 Sessions

`crates/proot-distro-core/src/session.rs` holds the session record; `crates/podnext/src/native/session.rs`
implements foreground login/run, signal forwarding into the guest, and the detached-session path. One
reader feeds three renderings (`ps`, `sessions`, `ps -f json`) — covered by
`crates/podnext/tests/session_registry.rs`. Measured: a guest killed by a signal reports exit code
**255**, not 128+signal.

### 8.7 IPC surfaces and their scoping

* The daemon socket is `~/.local/run/pn-next/daemon.sock` **regardless of which store is in use** —
  a store relocation does not relocate the socket (finding `ASEC3-ORCH-005`).
* `PN_TEST_ROOT` selects the store but leaks into the parent shell's environment without warning
  (finding `ASEC3-UX-02`), which is how a test fixture can appear to have deleted a real container.
* D-Bus support is a broker plus service definitions plus a zbus client (`pn-next/src/dbus/`), with the
  CLI surface in `crates/podnext/src/commands/dbus.rs`; the CLI reports it as `[experimental]`.
* Socket activation exists twice: `pn-next/src/socket_activation/` (systemd-style) and
  `pn-next/src/container_socket_activation/` (on-demand *container* start).

### 8.8 The native engine: what exists, what does not

| piece | state | evidence |
|---|---|---|
| `ProotEngine::run()` | exists | `crates/proot-core/src/lib.rs` |
| any binary calling it | **none** | no `ProotEngine` construction outside `proot-core` |
| `read_syscall()` | stub | returns `number: 0, args: [0;6]` |
| `read_retval()` | stub | returns `0` |
| resume a tracee | **absent** | zero hits for `PTRACE_CONT`, `PTRACE_SYSCALL`, `PTRACE_GETREGSET` across all crates |
| syscall dispatch table | 155 patterns, placeholder bodies | `crates/proot-core/src/syscall.rs` (1,395 lines) |
| selective seccomp backend | **real and tested** | `crates/proot-core/src/selective.rs`, `crates/proot-core/tests/selective_seccomp.rs` |
| identity/credential table | real, countable | `crates/proot-core/src/identity.rs`, `crates/proot-core/tests/identity_table.rs` |

So the crate named `proot-core` is today a *library of intent* plus one working interception backend;
execution in production goes through the external binary in §8.1.

### 8.9 Guest `/proc`, binds and shared memory

`crates/proot-distro-core/src/sysdata.rs` synthesises `/proc` entries (stat, loadavg, uptime, mounts) for
the guest. Measured: the synthetic `/proc/mounts` advertises a store bind as `tmpfs` although it is not.
`android_binds.rs` defines the Android bind set and the `termux-type` exclusion. `shm.rs` provides
`/dev/shm` at mode 1777, which passes the V8/Chromium requirement.

### 8.10 Extraction and copy rules

* `untar.rs` ("extraction that survives Android"): hard links are materialised as independent copies
  (measured: distinct inode, `nlink=1`), and unsupported special files are skipped rather than failing
  the install. A *missing* link source is silently swallowed, which is a defect, not a feature
  (`ASEC3-FS2-*`).
* `l2s.rs` is the link2symlink tree rewriter; a `.proot.l2s` marker is **absent** in the Ubuntu rootfs,
  so the rewriter is not what produces the copy behaviour above.
* `crates/podnext/src/commands/sync.rs:350` (`copy_preserving_holes`) opens the destination without
  `O_NONBLOCK`, which is why a FIFO planted in the copy path hangs the command (`rc=124` at exactly 20 s
  with a 20 s timeout).
* Measured: `cp -a` of a container rootfs fails (`exit 1`) on bind-target directories that are mode `000`;
  `tar -cf - . | tar -xf -` succeeds (`rc=0`) — the recipe the fixture builder must use.

### 8.11 Enforcement subsystems, measured

| subsystem | declared | measured state |
|---|---|---|
| policy engine | `pn-next/src/policy_engine/mod.rs` (758) | `PolicyAction` is defined in `pn-next/src/security.rs:132,138` and re-exported; **no consumer matches on it outside the engine**, so a policy can be evaluated and not enforced (`ASEC2-ARCH-008`) |
| guardrails | `pn-gateway/src/guardrails.rs:57-81` | only `Deny` returns an error; `Throttle` logs `"Throttling due to guardrail"` and applies nothing; `Warn`/`Quarantine`/`Alert` log; `Allow` is empty. `check_condition` (`:89-95`) always returns `Ok(false)` |
| resource quota | `pn-next/src/resource_quota/` (838) + supervisor | measured: a guest at `VmHWM 564672 kB` against a 512 MiB limit stays `running`, and 40 children run against `pids_limit=10` (`ASEC3-ORCH-002`) |
| watchdog | `pn-next/src/watchdog/` (736) | the documented flags `-m 3 -w 60` do not exist (`exit 2`); what actually stops a loop is the TOML `max_restarts` (negative control: `max_restarts=1`) |
| wake lock | `pn-next/src/wake_lock/mod.rs:224-247` | `acquire_platform` logs `"Android: Would acquire … wake lock"` and returns `Ok(())`; `dumpsys power` shows no PDN lock — the command reports success for a lock never taken |
| cgroup view | `pn-next/src/cgroup_view.rs` (369, adoption S8) | a private cgroupfs *view*; honesty is pinned by `pn-next/tests/cgroup_honesty.rs`, `runtime_cgroup_honesty.rs`, `cgroup_throttle_honesty.rs` |

### 8.12 Observability

`structured_logging/` (872) is journald-style with rotation, query and filtering, surfaced by
`crates/podnext/src/commands/log.rs`. The dashboard (`pn-next/src/web_dashboard/mod.rs`, 829) serves
`/api/metrics` at `:275` and defaults to `bind_addr: "0.0.0.0"` with `auth_enabled: false` (`:50`, `:55`).
At `:801` the reported CPU percentage is `25.0 + rand::random::<f64>() * 30.0` — a fabricated figure
(§9.2). `pn-gateway/src/metrics.rs` is a separate collector.

### 8.13 Daemon and service supervision

`crates/podnext/src/daemon.rs` (595) is the `podnext-daemon` binary: a persistent supervisor over a Unix
socket. `pn-next/src/daemon_runtime/` (1139) is the independent lifecycle manager, and
`pn-next/src/supervisor.rs` (2989) is the runit/s6-style supervisor. `readiness.rs` (186) implements start
coordination as one byte on a pipe (the runc `fifoFile` pattern, WORKLIST C10) and
`restart_policy.rs` (133) isolates the restart decision as a pure function (B14).
Default state on a fresh install, measured: daemon `RUNNING=false`, 0 services, empty watchdog, one
policy (`quarantine-unhealthy`), no socket present, `service status`/`quota usage` demanding a name.

### 8.14 Boot integration

`pn-next/src/termux_boot/` (278) plus `crates/podnext/src/commands/power.rs` (153) implement Termux:Boot
auto-start; measured working entry: `~/.termux/boot/podnext-boot.sh` executing `podnext-daemon`.

### 8.15 Supply chain

`digest.rs` (B1) validates a digest before it becomes a path component; `retry.rs` (B2/B3) is the
transfer policy; `signature.rs` defaults to `enabled: false` (`:33`) and `verify_image` returns early
(`:132`), so signature verification is off unless explicitly enabled. Measured gap: the installer path
pipes `curl` output into `dpkg -i` with no checksum verification (`TC-SUPPLY-01`, issue #74), and the
release workflow's `sha256` step tolerates failure.

---

## 9. Honesty register

A layer that exists only as a declaration is worse than an absent one, because it is believed. This
section therefore separates what the system *does* from what it *says*.

### 9.1 Proven by test or by measurement

- The store, its index cache, locks, sessions and atomic install/remove behaviour: pinned by
  `proot-distro-core/tests/{store_integrity,atomic_identity,blob_store_dedup,single_writer_races,store_fifo_is_metadata}.rs`
  and `podnext/tests/{install_interrupt,install_lock,remove_atomic}.rs`.
- The exit-code contract, argv quoting, stdout discipline, stub refusal: `podnext/tests/{exit_codes,quoting_table,stdout_discipline,stub_refusals}.rs`.
- The CLI reference itself: `podnext/tests/cli_reference_contract.rs` requires every command and option in
  `docs/src/cli-reference.md` to exist, and `pn-gateway/tests/doc_key_drift.rs` guards the configuration
  ↔ document mapping. This project already treats documentation drift as a defect class.
- Orphan reaping (`podnext/tests/orphan_reaping.rs`), session reattach parity, ref-counted eviction
  against the real binary, `/dev/shm` mode 1777, the selective-seccomp interception backend.
- Start coordination and restart decisions as isolated, testable units (`readiness.rs`, `restart_policy.rs`).
- `proot` guest facts: the argv, the version comment, the locale leak, identity variables, the process
  tree (§8.1–8.3), all reproduced on this device.

### 9.2 Declared, documented, and yet not real — with evidence

| claim made by the system | measured state | evidence |
|---|---|---|
| PDN runs containers itself | it spawns the **external** `proot` binary | §8.1; `PROOT_BIN=/nonexistent/proot` → `os error 2` |
| `proot-core` implements a ptrace engine | `ProotEngine::run()` is never called by any binary; `read_syscall()` returns `number: 0, args: [0;6]`; `read_retval()` returns `0` | no `ProotEngine` construction outside `proot-core` |
| the engine can continue a tracee | there is no `PTRACE_CONT`, `PTRACE_SYSCALL` or `PTRACE_GETREGSET` anywhere in the tree | repository-wide grep, zero hits |
| 155 syscall handlers exist | 155 *patterns* with placeholder bodies | `proot-core/src/syscall.rs` |
| a policy action is enforced | `PolicyAction` is defined in `pn-next/src/security.rs:132,138` and re-exported; **no consumer matches on it outside the engine**, so a policy can be evaluated and not enforced (`ASEC2-ARCH-008`) | doc comment added on `PolicyAction` (`pn-next/src/policy_engine/mod.rs:52-58`) and `podnext policy eval` now prints `applied by : nothing - this command evaluates; no component enforces the action` next to the action (`podnext/src/commands/policy.rs:126-129`), 2026-11-08. Wiring an enforcement point is a product decision and is **not** done here |
| guardrails throttle, quarantine and alert | only `Deny` returns an error; the rest call `warn!`; `check_condition` always returns `Ok(false)` | `pn-gateway/src/guardrails.rs:57-81,89-95` — **messages made truthful + condition made fail-closed 2026-11-08**: the Throttle/Quarantine arms now say the action was *requested and NOT applied by this engine* instead of "Throttling due to guardrail", and `check_condition` refuses (it had no caller and answered `false` without evaluating). Two further facts recorded, not fixed: `GuardrailManager` is **never constructed outside its own tests** (measured: no call site in `podnext` or `pn-next`), and every metric the evaluators read is a **mock** (`get_container_count() -> Ok(0)`, `get_memory_usage_percent() -> Ok(0.0)`, `get_cpu_usage_percent() -> Ok(0.0)`, `get_disk_usage_percent() -> Ok(0.0)`, `check_privileged_containers() -> Ok(false)` at `:415-433`), so the engine cannot judge anything real yet |
| a wake lock is acquired | `acquire_platform` logged `"Android: Would acquire … wake lock"` and returned `Ok(())`, so `acquire()` recorded it as held and the CLI printed success for a lock nothing had taken (`dumpsys power` showed no PDN lock) | `pn-next/src/wake_lock/mod.rs:224-247` — **fixed 2026-11-08**: `acquire_platform` now runs `termux-wake-lock` and fails by name when the facility is missing or the command exits non-zero; `Full`/`DozeExemption` are refused by name because the facility offers a partial lock only; `release` asks the platform even when the in-process map is empty (the lock is app-level and outlives the process). Measured end to end on the device: count 0 → `wakelock acquire e1 partial` → 1 (`PARTIAL_WAKE_LOCK 'termux:service-wakelock'`) → `wakelock release e1` → 0 |
| quotas limit memory and pids | a guest at `VmHWM 564672 kB` under a 512 MiB limit stays `running`; 40 children under `pids_limit=10` | finding `ASEC3-ORCH-002` |
| the watchdog honours `-m 3 -w 60` | those flags do not exist (`exit 2`); what actually stops a loop is the TOML `max_restarts` | negative control `max_restarts=1`. **Fixed 2026-11-08**: the claim lived in the repo's own `README.md:246` (and in the stage spec, `SPEC_STAGE3.txt:128,305`), and the documented form was not loadable - `watchdog policy` is not a subcommand and `-m`/`-w` are not flags (`-w` is already the global `--pwd`). The README now shows `podnext watchdog setpolicy web-server on-failure 3 60`, and that form was run: `rc=0`, `Watchdog policy set for 'web-server': OnFailure (max restarts: 3, window: 60s)` |
| the dashboard reports CPU usage | `stats.cpu_percent = 25.0 + rand::random::<f64>() * 30.0`, plus a fixed `memory_total = 8 GiB` at 40% used and `disk_total = 100 GiB` at 30% used | `pn-next/src/web_dashboard/mod.rs:801-807` — **fixed 2026-11-08**: CPU comes from `/proc/stat` (delta between polls, since-boot on the first), memory from `/proc/meminfo`, disk from `statvfs` on the store root, and `SystemStats::measured` carries "not measured here" as a third status because Android denies `/proc/stat` to an app uid (measured: `answered here: memory=true disk=true cpu=false`) |
| the dashboard is safe by default | `bind_addr: "0.0.0.0"`, `auth_enabled: false` | `pn-next/src/web_dashboard/mod.rs:50,55` |
| images are signature-verified | `enabled: false` by default; `verify_image` returns early | `proot-distro-core/src/signature.rs:33,132` |
| the hardened descriptor VFS protects path traversal | `DirFd` has no consumer; the only reference outside its module is a re-export | `proot-distro-core/src/lib.rs:63` |
| a REST API serves container management | `pn-next/src/api.rs` (1,422 lines) has no reachable entry point | §6.1 |
| `/proc/mounts` inside the guest is true | it advertises a store bind as `tmpfs` although it is not | finding `ASEC2-*` mount honesty |
| a copy cannot write outside the rootfs | a symlink whose leaf does not yet exist is written through (`rc=0` outside the rootfs), via `copy` and via `sync` | findings `ASEC3-SEC-001`, `ASEC3-SEC-002`; `podnext/src/commands/sync.rs:107-215` |
| `kill` stops the guest | it prints success and deletes the record while the `proot` process and the guest keep running | finding `ASEC3-PROC2-01`; `podnext/src/commands/kill.rs` |
| a FIFO is not a hazard | a FIFO at a container's `metadata.json`, or in the copy path, hangs the command (`rc=124`) | `sync.rs:350`; store-integrity fixture |
| the installer verifies what it installs | `curl` output is piped into `dpkg -i` with no checksum | `TC-SUPPLY-01`, issue #74 |
| CRI is supported | the module's own doc says "(Stub Implementation)" | `pn-next/src/cri_plugin/mod.rs:1,45,65` |
| network and storage subsystems in `proot-distro-core` work | both are self-declared stubs | `proot-distro-core/src/network.rs:1`, `storage.rs:1` |
| extraction refuses a link it cannot honour | a member whose link source is absent extracts as `Ok` and is silently dropped | finding `ASEC3-FS2-*` |

Not established either way in this pass, and therefore neither claimed nor denied: reachability of
`pn-next/src/{scheduler,workflow,security,plugin}.rs`, whether `refcount.rs` eviction is invoked by any
command path, and whether the eval of `policy presets` reaches an enforcement site.

## 10. Limits of this document, and how to keep it true

What this document *is*: a counted inventory (§2–§7), a mechanism map with file:line anchors (§8), and an
evidence-backed honesty register (§9). What it is *not*: a line-by-line reading of all 72,257 lines. The
mechanism chapters cover the paths that were executed or read during the architectural assessment; the
per-file tables and `MODULE_MAP.md` cover every file, so an unread mechanism shows up as an
undocumented-in-prose file rather than as a silent gap.

The three files with no module doc, listed so they cannot hide:
`crates/podnext/examples/test_daemon_client.rs`, `crates/podnext/tests/automation_contract.rs`,
`crates/proot-distro-core/tests/deep_fs_recursion.rs`.

Regeneration (run from the repository root; the walker prints the tree it counted, so a stale figure is
visible rather than inherited):

```sh
python3 - <<'PY'
import pathlib, re
root = pathlib.Path(".")
dirs = sorted([p for p in root.glob("crates/*") if p.is_dir()] + [p for p in root.glob("tests") if p.is_dir()])
tf = tl = 0
for d in dirs:
    files = sorted(d.rglob("*.rs"))
    if not files: continue
    loc = sum(len(f.read_text(errors="replace").splitlines()) for f in files)
    tf += len(files); tl += loc
    print(f"{d}: {len(files)} files, {loc} lines")
print(f"TOTAL: {tf} files, {tl} lines")
PY
```

Every number in this document can be re-derived with that walker plus the `grep`/`sed` anchors quoted in
§8 and §9. When a number and the walker disagree, the walker is right and this document is the defect.

To publish it as part of the manual, move this file to `docs/src/architecture.md` and add the chapter to
`docs/src/SUMMARY.md`; it is kept under `research/architecture/` for now because it documents
unflattering facts (§9.2) that the release notes have not yet been reconciled with.

