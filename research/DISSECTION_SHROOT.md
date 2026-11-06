# Dissection: `shroot` (mekhontsev/magicdesk) - read for adoption into PDN

Source of truth: the repository at commit `7ab99d6` (2026-11-03), cloned read-only into
`~/.hermes/cache/scratch/magicdesk`. Everything below is quoted from that tree with file:line, and the size is
measured, not estimated: **86.018 lines of C**, 11.553 lines of headers, 11.121 lines of Markdown across 732 files
under `native/shroot/`, plus a kernel-side helper under `kernel/`.

The point of reading it: shroot solves, in one native ARM64 userspace runtime, several problems PDN currently
inherits from proot - it runs **without PRoot**, keeps one ptracer per process tree, intercepts only the syscalls
that need adaptation, and supplies logical Linux resources (inodes, credentials, namespaces, mounts, IPC) so that
real init systems and container engines work. PDN is a Rust reimplementation of proot-distro that still delegates
execution to proot; the question this document answers is what is worth taking, piece by piece, and what is not
(see `docs/shroot.md` and `native/shroot/README.md` for its own scope statement).


## 1. What it is, measured

- Size: `native/shroot/` holds **348 C files / 86.018 lines of C**, 343 headers, 11.121 lines of Markdown, 732 files.
  Largest single units: `src/supervisor.c` (11.089), `src/namespace_broker.c` (1.760), `src/fs_buffer_service.c`
  (1.307), `src/network_forward.c` (1.299), `src/cgroup.c` (1.239), `src/inode_db.c` (890), `src/ipc_credentials.c`
  (881), `src/fd_origins.c` (600). Plus a vendored `lwip/` and a kernel-side helper in `kernel/xr-resolution-fix/`.
- No PRoot and no Termux: the bootstrap maps an ARM64 ELF plus its interpreter, builds stack/auxv and enters the
  distribution's stock glibc/musl loader (`native/shroot/README.md`). "No PRoot process, external container manager
  or SELinux change is required."
- Executor: Android's authorised shell **UID 2000** as the unrooted baseline, or an explicitly chosen root
  (`docs/shroot.md`). Root is never a fallback: "A denied shell operation never causes an identity switch."
- Kernel floor: **Linux 5.15**, stated as a support boundary, with `shroot --probe` checking executable mappings,
  pidfds, subreapers, selective ptrace, seccomp notifications, descriptor publication and helper teardown *before*
  any guest program starts (`native/shroot/kernel-compatibility.md`, `docs/shroot.md`).

## 2. The interception mechanism, from the code

- **Selective seccomp, not ptrace-everything.** One supervisor per process tree is the sole ptracer
  (`src/supervisor.c:2956` sets `PTRACE_SETOPTIONS`); the guest is filtered so that only categories needing
  adaptation stop: `SECCOMP_RET_TRACE` with per-category cookies `MD_INTERCEPT_DISPATCH`, `_WATCH`, `_MEMORY`,
  `_PROC` (`src/fd_filter_program.c:66,73,80,107,117,139`; `src/interception.c:282,284`). Descriptor metadata uses
  `SECCOMP_RET_USER_NOTIF` with `SECCOMP_IOCTL_NOTIF_ADDFD` / `NOTIF_ID_VALID` (`src/fd_channel.c:66`,
  `src/fd_install.c:14,36,52,59`).
- Consequence, in the runtime's own words: "Ordinary unselected data IO, memory, futex and signal-mask operations
  retain native execution" (`native/shroot/README.md`).
- **Lazy machine-code entry.** Unmodified programs' AArch64 `svc` sites are admitted by scanning instruction words
  for the encoding `(word & 0xffe0001f) == 0xd4000001` (`src/supervisor.c:2624`), with SVC pools sized
  `MD_SVC_CODE_BYTES` / `MD_SVC_POOL_SIZE` (`src/interception_memory.c:319-321`). Unsupported sites fall back to
  "Base interception"; `SHROOT_MACHINE_ENTRY=0` selects it explicitly.
- **Register trap, documented with a kernel citation.** Policy and the retained typed-descriptor kind travel in
  **x9**, not x7, because the kernel restores x7 after an ARM64 syscall stop (`interception.md`, citing
  `arch/arm64/kernel/ptrace.c#L1694-L1726`). Anyone reimplementing a syscall gate on arm64 will meet this.
- **Identity is emulated through an explicit table**: 19 syscalls - `getuid geteuid getgid getegid getresuid
  getresgid setresuid setresgid setuid setgid setreuid setregid setfsuid setfsgid setgroups getgroups capget
  capset` (`src/interception.c:488-489`), backed by `src/guest_identity.c:233,251`.
- **Descriptor origins**: a sparse metadata table keyed by guest FD number, retaining anchors and flags but never
  read/write descriptions or offsets, following `CLONE_FILES` independently of `CLONE_FS`, and requiring a positive
  allocation-free proof (`md_fd_origins_native_regular`) before resuming an untouched kernel call. "An absent entry
  means unknown provenance, not EBADF" (`descriptor-origins.md`).
- **Pipes and FIFOs become metadata, not device nodes**: `pipe2` publishes both endpoints in one descriptor-table
  transaction; `inode_fifo` owns store-scoped kernel pipe pins, and "the backing object is regular metadata, not a
  native `mknod` request: Android shell policy can prohibit native [mknod]" (`fifos.md`).
- **Guest FUSE without the host FUSE device**: protocol 7.31 over a typed guest `/dev/fuse`, sealed memfd snapshots
  and O_TMPFILE read-only shared mappings; no `CONFIG_FUSE_FS` and no post-5.15 host dependency
  (`kernel-compatibility.md`).
- **Honest limits, stated by the authors**: "It is not a full Linux ABI or an isolation boundary for hostile code"
  (`README.md`), "Inherited shared bytes are not isolation between mutually hostile guest processes"
  (`interception.md:499`), and paths it cannot fake are returned as explicit **unavailable** interfaces rather than
  approximated (`interception.md:737-745`).

## 3. Its own benchmark, and the number that is worse

`docs/shroot-benchmarks.md` (Alpine, one Android phone, 2026-10-26, columns as published: chroot / PRoot / Shroot):

| workload | chroot | PRoot | Shroot | Shroot vs PRoot |
| --- | ---: | ---: | ---: | ---: |
| 16,384 `stat` | 8.11 ms | 510.31 ms | 41.46 ms | **12.3x faster** |
| 16,384 `open/close` | 18.32 ms | 877.82 ms | 133.61 ms | **6.6x faster** |
| 16,384 create/rename/unlink | 251.12 ms | 2,959.39 ms | 828.75 ms | **3.6x faster** |
| 128 `fork/exec/wait` | 131.37 ms | 718.45 ms | 1,100.13 ms | **1.53x SLOWER** |
| SQLite CLI + shared-library build | 37.87 s | 38.02 s | 39.74 s | about 5% slower |

The metadata wins and the process-creation loss are both explained by the same design choice: stopping per
*selected syscall* is cheap, while the supervisor/worker per-process setup is not. A reimplementation should adopt
the win and plan for the cost - and, following this project's own rule, publish the worse number too.

## 4. Correction after a challenge: cgroups are two different things

A fair objection was raised - if PDN is to run Docker *inside* it, are cgroups not required? Reading `cgroup.c` and
its own resource table separates the two halves, and shows that my first pass collapsed them wrongly.

**What shroot's `cgroup.c` actually is (1.239 lines):** a userspace **view** of cgroupfs, not enforcement. It owns a
private file enum (`PROCS, THREADS, EVENTS, CONTROLLERS, SUBTREE, TYPE, FREEZE, ALLOW, DENY, LIST, TASKS, NOTIFY,
RELEASE, PIDS_MAX, PIDS_CURRENT, PIDS_EVENTS`, `src/cgroup.c:25-26`) with per-file modes 0644/0444 (`:31-33`), answers
`mkdir`/`subtree_control` writes itself (`md_cgroup_mkdir :755`, `subtree_write :872`), and produces its own errno
values (`-EBUSY`, `-EPERM`, `-EACCES`, `-EROFS` at `:781-986`). Nothing here writes to the host's `/sys/fs/cgroup`.

**What it does not do, in its own words** (`docs/shroot.md:75`): the resource table lists "hierarchical cgroup
`pids.max`, membership, freezer and sampled CPU/RSS counters" and then states the boundary - "Android's physical
FD/task budgets still apply; **cgroup CPU, cpuset, memory and device policies are not enforced**; sampled RSS is not
kernel page charging; other resource limits remain native".

**So how does Docker run there at all** (`docs/shroot.md:1130` "Docker Engine", `:139`, `:918` "systemd runs as guest
PID 1 in Shroot's private PID/mount/UTS/IPC/cgroup" namespaces, `:999` the routed network backend "for guests such as
Docker/Podman that configure their own network topology")? Not by enforcing limits: by giving the guest a private
cgroup **namespace view** plus the real kernel operations that its executor is permitted to perform. That permission
is the load-bearing part, and it is exactly what PDN's executor - Android shell UID 2000 inside Termux - does not
have for cgroup subtree creation.

**Therefore the adoption splits, and the earlier "not taken" note was too coarse:**

- **Take the view** (recorded as S8): a private cgroupfs hierarchy with `pids.max`, membership, freezer and sampled
  counters, refusing what it cannot honour with an explicit errno instead of pretending.
- **Do not port enforcement** (S9, conditional): CPU, cpuset, memory and device policies need a rootful executor and
  a delegated subtree before they can be enforced by anyone. Record the prerequisite and probe for it; do not
  reimplement 1.239 lines that the reference implementation itself does not enforce.
- The dockerd `Devices`-cgroup gate dissected earlier (`daemon/daemon.go:1130-1132`, WORKLIST B15) is the same
  conclusion from the other side: an engine that needs device policy cannot be satisfied by a view.
