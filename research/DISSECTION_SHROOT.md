# Dissection: `shroot` from mekhontsev/magicdesk, read for adoption into PDN

Source of truth: the repository at commit `7ab99d6`, dated 2026-11-03, cloned read-only into
`~/.hermes/cache/scratch/magicdesk`. Every item below is quoted from that tree with file:line, and the size is
measured, not estimated: **86.018 lines of C**, 11.553 lines of headers, 11.121 lines of Markdown across 732 files
under `native/shroot/`, plus a kernel-side helper under `kernel/`.

The reason to read it: shroot solves several problems in one native ARM64 userspace runtime. PDN currently
inherits these problems from proot. Shroot runs **without PRoot**. It keeps one ptracer for each process tree. It
intercepts only the syscalls that need adaptation. It supplies logical Linux resources: inodes, credentials,
namespaces, mounts, and IPC. Real init systems and container engines then work. PDN is a Rust reimplementation of
proot-distro that still delegates execution to proot. This document answers one question: what is worth taking,
piece by piece, and what is not. For the scope statement of shroot, see `docs/shroot.md` and `native/shroot/README.md`.


## 1. What it is, measured

- Size: `native/shroot/` holds **348 C files / 86.018 lines of C**, 343 headers, 11.121 lines of Markdown, 732 files.
  Largest single units: `src/supervisor.c` at 11.089, `src/namespace_broker.c` at 1.760, `src/fs_buffer_service.c`
  at 1.307, and `src/network_forward.c` at 1.299.
  Then `src/cgroup.c` at 1.239, `src/inode_db.c` at 890, `src/ipc_credentials.c` at 881, and `src/fd_origins.c` at 600.
  Plus a vendored `lwip/` and a kernel-side helper in `kernel/xr-resolution-fix/`.
- No PRoot and no Termux. The bootstrap maps an ARM64 ELF plus its interpreter. It builds the stack and auxv. It
  then enters the stock glibc or musl loader of the distribution. See `native/shroot/README.md`. A quote from that
  file: "No PRoot process, external container manager or SELinux change is required."
- Executor: the authorised Android shell, **UID 2000**, as the unrooted baseline. A chosen root is also possible.
  See `docs/shroot.md`. Root is never a fallback. A quote: "A denied shell operation never causes an identity switch."
- Kernel floor: **Linux 5.15**, given as a support boundary. The command `shroot --probe` checks executable mappings,
  pidfds, subreapers, selective ptrace, seccomp notifications, descriptor publication, and helper teardown. It runs
  that check *before* any guest program starts. See `native/shroot/kernel-compatibility.md` and `docs/shroot.md`.

## 2. The interception mechanism, from the code

- **Selective seccomp, not ptrace for every call.** One supervisor for each process tree is the only ptracer.
  The file `src/supervisor.c:2956` sets `PTRACE_SETOPTIONS`. The guest filter stops only the categories that
  need adaptation. These categories use `SECCOMP_RET_TRACE` with the per-category cookies `MD_INTERCEPT_DISPATCH`,
  `_WATCH`, `_MEMORY`, and `_PROC`. See `src/fd_filter_program.c:66,73,80,107,117,139` and `src/interception.c:282,284`.
  Descriptor metadata uses `SECCOMP_RET_USER_NOTIF` with `SECCOMP_IOCTL_NOTIF_ADDFD` or `NOTIF_ID_VALID`.
  See `src/fd_channel.c:66` and `src/fd_install.c:14,36,52,59`.
- Consequence, in the words of the runtime: "Ordinary unselected data IO, memory, futex and signal-mask operations
  retain native execution". The quote comes from `native/shroot/README.md`.
- **Lazy machine-code entry.** The code admits AArch64 `svc` sites of unmodified programs by a scan of instruction
  words for the encoding `(word & 0xffe0001f) == 0xd4000001`. See `src/supervisor.c:2624`. The SVC pools are sized
  `MD_SVC_CODE_BYTES` and `MD_SVC_POOL_SIZE`. See `src/interception_memory.c:319-321`. Unsupported sites fall back to
  "Base interception". The setting `SHROOT_MACHINE_ENTRY=0` selects it explicitly.
- **Register trap, with a kernel citation.** Policy and the retained typed-descriptor kind travel in
  **x9**, not x7. The kernel restores x7 after an ARM64 syscall stop. See `interception.md`, which cites
  `arch/arm64/kernel/ptrace.c#L1694-L1726`. Anyone who reimplements a syscall gate on arm64 will meet this.
- **Identity comes from an explicit table**: 19 syscalls. The table holds
  `getuid geteuid getgid getegid getresuid getresgid setresuid setresgid setuid setgid setreuid setregid setfsuid setfsgid setgroups getgroups capget capset`.
  See `src/interception.c:488-489`. The data comes from `src/guest_identity.c:233,251`.
- **Descriptor origins**: a sparse metadata table keyed by guest FD number. It keeps anchors and flags, but never
  read/write descriptions or offsets. It follows `CLONE_FILES` independently of `CLONE_FS`. It requires a positive
  allocation-free proof, `md_fd_origins_native_regular`, before it resumes an untouched kernel call. The rule is:
  "An absent entry means unknown provenance, not EBADF". See `descriptor-origins.md`.
- **Pipes and FIFOs become metadata, not device nodes**: `pipe2` publishes both endpoints in one descriptor-table
  transaction. The object `inode_fifo` owns store-scoped kernel pipe pins. A quote: "the backing object is regular
  metadata, not a native `mknod` request: Android shell policy can prohibit native [mknod]". See `fifos.md`.
- **Guest FUSE without the host FUSE device**: protocol 7.31 over a typed guest `/dev/fuse`, sealed memfd snapshots,
  and O_TMPFILE read-only shared mappings. No `CONFIG_FUSE_FS` and no host dependency after 5.15.
  See `kernel-compatibility.md`.
- **Honest limits, stated by the authors**: "It is not a full Linux ABI or an isolation boundary for hostile code",
  from `README.md`. Also: "Inherited shared bytes are not isolation between mutually hostile guest processes",
  from `interception.md:499`. Paths that it cannot fake come back as explicit **unavailable** interfaces, not as
  approximations. See `interception.md:737-745`.

## 3. Its own benchmark, and the number that is worse

`docs/shroot-benchmarks.md`, Alpine on one Android phone, 2026-10-26, columns as published: chroot / PRoot / Shroot:

| workload | chroot | PRoot | Shroot | Shroot vs PRoot |
| --- | ---: | ---: | ---: | ---: |
| 16,384 `stat` | 8.11 ms | 510.31 ms | 41.46 ms | **12.3x faster** |
| 16,384 `open/close` | 18.32 ms | 877.82 ms | 133.61 ms | **6.6x faster** |
| 16,384 create/rename/unlink | 251.12 ms | 2,959.39 ms | 828.75 ms | **3.6x faster** |
| 128 `fork/exec/wait` | 131.37 ms | 718.45 ms | 1,100.13 ms | **1.53x SLOWER** |
| SQLite CLI + shared-library build | 37.87 s | 38.02 s | 39.74 s | about 5% slower |

The same design choice explains both the metadata wins and the process-creation loss. A stop for each
*selected syscall* is cheap. The supervisor and worker setup for each process is not. A reimplementation must take
the win and plan for the cost. The rule of this project also applies: publish the worse number too.

## 4. Correction after a challenge: cgroups are two different things

Someone raised a fair objection: if PDN runs Docker *inside* it, does it not need cgroups? A read of `cgroup.c` and
its own resource table separates the two halves. My first pass merged them by mistake.

**What the `cgroup.c` file of shroot really is, 1.239 lines:** a userspace **view** of cgroupfs, not enforcement. It owns a
private file enum. The enum is `PROCS, THREADS, EVENTS, CONTROLLERS, SUBTREE, TYPE, FREEZE, ALLOW, DENY, LIST, TASKS, NOTIFY, RELEASE, PIDS_MAX, PIDS_CURRENT, PIDS_EVENTS`.
See `src/cgroup.c:25-26`. The per-file modes are 0644 or 0444. See `:31-33`. The file answers `mkdir` and
`subtree_control` writes itself. See `md_cgroup_mkdir :755` and `subtree_write :872`. It produces its own errno
values: `-EBUSY`, `-EPERM`, `-EACCES`, and `-EROFS`. See `:781-986`. Nothing here writes to the host `/sys/fs/cgroup`.

**What it does not do, in its own words** from `docs/shroot.md:75`: the resource table lists "hierarchical cgroup
`pids.max`, membership, freezer and sampled CPU/RSS counters". Then it states the boundary.
"Android's physical FD/task budgets still apply." "**cgroup CPU, cpuset, memory and device policies are not enforced**."
"sampled RSS is not kernel page charging." "other resource limits remain native".

**So how does Docker run there at all?** The answer is in `docs/shroot.md:1130`, "Docker Engine", and in `:139` and
`:918`. The text at `:918` says "systemd runs as guest PID 1 in Shroot's private PID/mount/UTS/IPC/cgroup" namespaces.
The text at `:999` describes the routed network backend "for guests such as Docker/Podman that configure their own
network topology". The answer is not enforcement of limits. The guest gets a private cgroup **namespace view** plus
the real kernel operations that its executor can perform. That permission is the load-bearing part.
The executor of PDN, the Android shell UID 2000 inside Termux, does not have this permission for cgroup subtree creation.

**The adoption therefore splits. The earlier "not taken" note was too coarse:**

- **Take the view**, recorded as S8: a private cgroupfs hierarchy with `pids.max`, membership, freezer, and sampled
  counters. It refuses what it cannot honour with an explicit errno and does not pretend.
- **Do not port enforcement**, S9, conditional. CPU, cpuset, memory, and device policies need a rootful executor and
  a delegated subtree before anyone can enforce them. Record the prerequisite and probe for it. Do not
  reimplement 1.239 lines that the reference implementation itself does not enforce.
- The dockerd `Devices`-cgroup gate dissected earlier leads to the same conclusion from the other side.
  See `daemon/daemon.go:1130-1132`, WORKLIST B15. A view cannot satisfy an engine that needs device policy.
