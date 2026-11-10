# R2-EXECUTION: the execution layer, re-read from raw source

Round-2 re-read of the three comparators that carry the *execution* model.
This round answers against `docs/src/roadmap.md` section 10, the v0.3.0 mobility target. The gaps are 10.1 to 10.5.
Every claim is a `file:line` in the raw tree. This round re-tests each claim and does not quote the earlier pass.

**Sources, read-only and verified this round:**

| tree | path | scale (measured) |
|---|---|---|
| runc v1.5.2 | `~/.hermes/cache/scratch/src/runc-src` | 930 `.go` files |
| shroot (mekhontsev/magicdesk @ `7ab99d6`) | `~/.hermes/cache/scratch/magicdesk/native/shroot` | 348 C files / 86,018 lines C |
| containerd v2.4.2 (shim side) | `~/.hermes/cache/scratch/src/containerd-src` | tree is a **subset** — see §7 |

**Markers.** **(R)** means read from source, with the line quoted. **(M)** means measured on this device.
**(I)** means inference from read code. **UNVERIFIED** means checked and unsettled. It names what will settle it.

**The four axes.** The roadmap section 10 gives this intent word for word:
**MOBILITY**, the same daily-driver reach as `proot-distro`.
**CAPABILITY**, what a guest can actually do. **STABILITY**, no orphans and an honest exit or errno.
**OPTIMIZATION**, the cost per operation.

**The competitive test, defined once.** Will PDN lose to the installed `proot-distro` on this
device without this mechanism?* If proot-distro does not have it, or has it and PDN already matches,
the answer is **no** and the mechanism is not a v0.3.0 obligation. It is recorded as optional or a
dead end, with the reason. "Competes with" means the four axes, not feature-count parity with an OCI
runtime PDN does not run.

---

## 0. Corrections to the earlier passes (re-tested, not quoted)

Three claims from `research/DISSECTION_SHROOT.md` did not survive re-reading:

| earlier claim | verdict | evidence |
|---|---|---|
| `--probe` checks *selective ptrace* (DISSECTION_SHROOT §1, from `kernel-compatibility.md`) | **REFUTED** | `grep -n ptrace native/shroot/src/bootstrap.c` → no match. The probe (`bootstrap.c:54-87`) never touches ptrace. "Selective ptrace" is a **baseline capability the runtime assumes** (`kernel-compatibility.md:144`), not something the probe asserts. |
| shroot's `docs/shroot.md` (cited repeatedly) | **REFUTED (path)** | no `docs/` dir exists; the shroot documents are `native/shroot/*.md`. The resource boundary is `README.md:214-216`, not `docs/shroot.md:75`. |
| "19 identity syscalls" (`interception.c:488-489`) | **REFUTED (count)** | the macro list spans `interception.c:487-491` and holds **18** syscalls; lines 488-489 hold 8 of them. |
| `supervisor.c:2956` sets `PTRACE_SETOPTIONS`; `supervisor.c:2624` svc-encoding mask | **CONFIRMED** | both exact; but `2956` is a *debugger-options* setter gated at `:2953`, unrelated to admission. |

All runc citations in `workbench/NEXT_DEV.md` §4 (container_linux.go:530, process_linux.go:984-1016,
standard_init_linux.go:152-156, hasInit 894-905) were re-tested and are **accurate**.

---

## 1. runc — the OCI lifecycle, and what PDN already has

### 1.1 The lifecycle verbs (R)

| verb | top-level | libcontainer |
|---|---|---|
| create | `create.go:63` `status, err := startContainer(context, CT_ACT_CREATE, nil)` | `utils_linux.go:277` → `container.Start` (`container_linux.go:205`) |
| start | `start.go:34-39` `case libcontainer.Created:` → `container.Exec()` | `container_linux.go:227` |
| exec | `exec.go:155-193` `execProcess` → `runner{action: CT_ACT_RUN, init:false}` | `container_linux.go:227` |
| kill | `kill.go:55` `err = container.Signal(signal)` | `container_linux.go:396` |
| delete | `delete.go:80-87` `Stopped → Destroy()`, `Created → killContainer()` | `state_linux.go` |
| state | `state.go:28-39` `container.Status()`, `.State()`, zero pid if stopped | `container_linux.go:874` `refreshState` |
| run | `run.go:76` `startContainer(context, CT_ACT_RUN, nil)` | `container_linux.go:214` |

**Status model (R).** libcontainer enum `Created, Running, Paused, Stopped`
`libcontainer/container.go:16-25`. OCI strings add `creating` for hook visibility.
The vendor path `vendor/.../specs-go/state.go:8-18` adds it. The runtime emits it transiently at `process_linux.go:972,1034`.
Status is **recomputed from `/proc`, never trusted from disk**: `refreshState`
`container_linux.go:874-891` treats `!hasInit()` as stopped and the presence of `exec.fifo`
as created. `state.json` is written atomically. The temp file is at `container_linux.go:838` and the rename at `:860`.
The write happens **inside the readiness handshake**. `process_linux.go:1007 updateState` fires on the `procReady`
message, and not before it.

**PDN's counterpart, by verb. All lines read this round:**

| runc verb | PDN today | file |
|---|---|---|
| create | `install` / container-dir creation + a metadata record | `crates/proot-distro-core/src/install.rs`, `.../container.rs` |
| start | `run` / `login` spawns proot | `crates/proot-core/src/lib.rs:486` (`c.spawn()`) |
| exec | `run <c> -- <cmd>` (argv table) | `crates/proot-core/src/lib.rs` |
| kill | `podnext kill` with a ladder | `crates/podnext/src/commands/container.rs:388` `KILL_LADDER=[SIGTERM,SIGQUIT,SIGKILL]` |
| delete | `podnext remove` | `crates/podnext/src/commands/container.rs` |
| state | `ps`/`list` + `metadata.json` (not `state.json`) | `crates/proot-distro-core/src/container.rs` |
| console/PTY | `openpty` + `setsid` + `TIOCSCTTY` + winsize | `crates/podnext/src/native/session.rs:505-522` |
| exit status | the full triple | `crates/proot-core/src/lib.rs:549-563` |
| init re-exec / reaping | **absent** — no `PR_SET_CHILD_SUBREAPER` anywhere | `grep -rn SUBREAPER crates/` → empty |
| detach/attach | supervisor + socket + log | `crates/podnext/src/native/session.rs:531-556`; `#57` |

**What buys what.** The OCI *verbs* are a format, not a capability: PDN keeps its own names and its
own state model, and the guest cannot tell the difference. The one part with teeth is the **status
recompute discipline**: `hasInit` plus FIFO probing at `container_linux.go:874-891`. PDN's
`process_is_alive(pid, start_time)` at `crates/proot-distro-core/src/session.rs:93` already applies
the *same* rule. It is `(pid, kernel start-time, not-a-zombie)`, which is the runc contract, arrived at
independently.

**Competitive test.** Does PDN lose to proot-distro by not exposing an OCI lifecycle? **No.**
proot-distro exposes none either. Both are `install/login/run/remove` front-ends. Roadmap 10.1-10.4
do not ask for one. **v0.3.0 obligation: none.** Keep the two cheap tests below and build nothing.

- **Acceptance test, create and status honesty.** `podnext` killed mid-`create` by SIGKILL between
  container-dir creation and metadata write) leaves **no orphan process and no half-written
  `metadata.json`**. Counts of both are taken before and after.
  This mirrors the failure mode that runc documents and guards against. runc writes state inside the readiness handshake.
- **Acceptance test, liveness.** A recycled PID is not reported as a live session, and a zombie is
  not reported running: `ps` after `kill -STOP` + upstream reap returns `stopped`, not `running`.

### 1.2 The mechanism PDN genuinely lacks: a subreaper (STABILITY)

runc's top-level process becomes a subreaper before it signals anything
`signals.go:24 system.SetSubreaper(1)` calls `unix.Prctl(PR_SET_CHILD_SUBREAPER,...)`, and reaps with
`Wait4(-1, &ws, WNOHANG, &rus)` in a loop at `signals.go:130-150`. `runc init` itself **does not
reap**. After `standard_init_linux.go:303 linux.Exec(...)` it is the user's binary. The parent is the
reaper precisely because nsexec forked with `CLONE_PARENT` at `nsenter/nsexec.c:322`.

PDN has **no subreaper at all**. proot-distro does (`run_step.py:_become_subreaper`, NEXT_DEV doctrine
7). Without it, a guest that double-forks, such as daemons, `apt` and `systemd --user`, reparents to Android's
init and PDN cannot find or kill it. The soak harness at `scripts/soak_sessions.py` will report
leftovers that `podnext kill` cannot reach.

**Competitive test.** Does PDN lose to proot-distro without it? **Yes**. proot-distro reaps its
build/run descendants. PDN's kill ladder can only reach processes it still owns. **v0.3.0 obligation:
YES.** Cost: one `prctl(PR_SET_CHILD_SUBREAPER,1)` at launcher start plus a `wait4(-1,WNOHANG)` drain.

- **Acceptance test.** A guest command that double-forks via `sh -c '(sleep 999 &) ; exit'` leaves
  **zero** surviving processes after the launcher exits. The proof counts `proot`/`podnext` rows in
  `/proc` before and after, with a negative control that goes red when the subreaper call is removed.

### 1.3 The one part of runc's create the competitive test does not ask for

`container_linux.go:530 exec.Command(exePath, "init")` (fork/exec of runc's own binary with argv[1]
== "init"), and the `exec.fifo` handshake. The parent reads at `container_linux.go:233-254`.
The child writes one byte at `standard_init_linux.go:264-269`. The nsexec `CLONE_PARENT` staging at `nsenter/nsexec.c:322,867` together
exist **only to enter namespaces and hand a container its PID 1**. PDN has no namespaces (roadmap
§1.2: `unshare(CLONE_NEW*)` → `EPERM`, `CapEff=0`) and does not re-exec an init. proot substitutes
its own loader for the guest program. Copying runc's bootstrap into PDN is **incompatible with the
proot tracee model** and is a dead end (§8). The FIFO idea alone (a one-byte readiness gate instead of
a sleep/poll) is *reusable* for PDN's own supervisor readiness, but that is an internal nicety, not a
v0.3.0 obligation.

---

## 2. runc — exit codes, signals without a PID namespace, stdio EOF

### 2.1 Exit status, the triple. STABILITY

**(R)** Exactly one place turns a wait into a process exit code:

```go
// libcontainer/utils/utils.go:12-23
const ( exitSignalOffset = 128 )
func ExitStatus(status unix.WaitStatus) int {
    if status.Signaled() { return exitSignalOffset + int(status.Signal()) }
    return status.ExitStatus()
}
```

- signal death → `128 + N`. A clean exit → the child's own status. The call site is `signals.go:148
  status: utils.ExitStatus(ws)`.
- `255` is a **runtime failure**, from two origins. Init cannot report over the pipe
  at `libcontainer/init_linux.go:122 os.Exit(255)`, or an exec failure at `exec.go:114 fatalWithCode(...,255)`.
- there is **no symbol named `waitStatus`** in the tree. The R grep confirms this. The wait status is a local
  `ws unix.WaitStatus` at `signals.go:132`.

**PDN already implements the exact triple** at `crates/proot-core/src/lib.rs:549-563` and `exit_status_code_parts`.
The 255 branch is tested. The failure it closed is documented as A6.
The earlier collapse of a signal death to -1 made 137/139/143 indistinguishable from a runtime failure.
**v0.3.0 obligation: none. This work is done.** The acceptance test already exists
at `crates/podnext/tests/exit_codes.rs` with 7 cases. The remaining value is the *contract* being asserted
against a signal death, `kill -9` in the guest → 137, which the 255-and-signal branches cover.

### 2.2 Signals without a PID namespace. STABILITY

**(R)** `Container.Signal` at `container_linux.go:396-425` branches on the namespace.

```go
// container_linux.go:407
if s == unix.SIGKILL && !c.config.Namespaces.IsPrivate(configs.NEWPID) {
    if err := signalAllProcesses(c.cgroupManager, unix.SIGKILL); err != nil { ... }
    return nil
}
return c.signal(s)
```

- All calls except SIGKILL go to **init only** at `process_linux.go:139-141`
  `p.cmd.Process.Signal(sig)`. With a private pidns init==PID1, and the kernel broadcasts.
  Without one, only the init pid hears it, at `container_linux.go:400-406`, verbatim.
- The no-pidns SIGKILL path calls `signalAllProcesses` at `init_linux.go:684-720`. cgroup v2 writes
  `cgroup.kill=1` at `:691`, else freeze → `GetAllPids` → `kill` each at `:699-715` → thaw.
- **No `PidfdSendSignal` in runc core**. The R grep of non-vendor code confirms it. pidfds are used only for fd-passing
  at `process_linux.go:1108,1113` and `init_linux.go:727-732`.

**This is PDN's burned case**. Roadmap §2.1 says `kill -9` to PRoot orphans the tracees. proot ignores
SIGTERM, SIGINT and SIGHUP. PDN's answer is the kill ladder
`container.rs:388 [SIGTERM,SIGQUIT,SIGKILL]`, plus inode and `(pid,start_time)` session clearing
at `session.rs:93` and `wait_and_clear`, which is the proot-distro route, not runc's.

**The runc route is a dead end here.** `signalAllProcesses` needs a delegated cgroup subtree to write
read-only for the Termux uid and `CapEff=0`, as roadmap
§1.2 records. PDN cannot use cgroup-kill. The ladder is the only reachable path.
**Competitive test.** Does PDN lose to proot-distro here? **No**. PDN already has the ladder. The
gap is not the ladder but the *reaper behind it*, roadmap §1.2. The ladder can only signal processes PDN
still owns. **v0.3.0 obligation: the subreaper only. Keep the ladder.**
- **Acceptance test.** A guest that ignores `SIGTERM` is escalated `TERM→QUIT→KILL`, and
  `kill -s STOP` is delivered once and never escalated (`container.rs:455`, the
  `_NON_TERMINATING_SIGNALS` set). Coverage exists. Keep it green.

### 2.3 stdio EOF and PTY handling. STABILITY

**(R)** EOF propagation is `io.Copy` returning:

```go
// tty.go:56-59 (pipes)
go func() { _, _ = io.Copy(i.Stdin, os.Stdin); _ = i.Stdin.Close() }()
// tty.go:129-131 (pty)
go func() { _, _ = io.Copy(epollConsole, os.Stdin) }()
go t.copyIO(os.Stdout, epollConsole)
```

The `Close()` after `Copy` turns the host's stdin EOF into the container's EOF. There is no signal and no
out-of-band marker. PTY side: `console_linux.go:100` opens `/dev/pts/ptmx`, `:121
`console.NewPtyFromFile`, not `console.NewPty`, which is absent. `init_linux.go:410`
`cmsg.SendRawFd(socket, pty.Name(), pty.Fd())` passes the master over a unix socket, and
`dupStdio` at `init_linux.go:158-166` binds the peer to 0/1/2.

**PDN already has the PTY half** (`native/session.rs:505-522`: `openpty`, `setsid`, `TIOCSCTTY`,
`dup2` slave onto 0/1/2, and a socket plus an append log for attach at `:531-556`. What it has *not* wired is
runc's exact "Copy-returns-then-Close" EOF discipline on the *host* side: the session master is
drained non-blocking on the reactor at `:558-561`. The declared limitations are that a resize while
attached is not re-forwarded and detached output is appended rather than replayed
See `known-limitations.md` and roadmap §9.3.

**Competitive test.** Does PDN lose to proot-distro without runc's exact EOF path? **No**.
proot-distro's `login` has no detach/attach at all, so PDN is ahead. **v0.3.0 obligation: none**.
the remaining items are the two declared session limitations, not execution-model gaps.
- **Acceptance test.** An attached session. Sending EOF, Ctrl-D, at the client makes the guest shell
  exit and the supervisor reap it. The guest pid is the same and no orphan remains. A client drop without EOF leaves the
  guest running and reattachable, the #57 round trip.

### 2.4 The runc seccomp and setup order. Reference only

**(R)** `standard_init_linux.go` sets `no_new_privs` at `:152-156`. It sets **seccomp early** only when NNP is
false and the container is privileged, `:191-197`, before `finalizeNamespace`. It sets **seccomp late**, as
close to `execve` as possible, when NNP is true, `:239-245`, after `finalizeNamespace`. Then the
final `linux.Exec` at `:303`. This is the ordering a **tracee-owning** runtime follows. PDN does not
install a filter of its own (§5), so the order is reference knowledge, not a task.

---

## 3. shroot — the admission probe, in the order it runs

**(R)** Entry `src/bootstrap.c:54` `if (argc == 2 && md_equal(argv[1], "--probe")) {`. The probe is
**inline in `md_boot`**. There is no `probe*.c` file. The R check `ls src | grep probe` returns empty. Every step
fails with `md_die("<label>", rc)` → **binary pass/fail**, threshold = the call returns `>=0`, with a
single exact-value check, step 6. It runs **after** the identity gate `bootstrap.c:52-53`. The probe
only certifies what the *selected shell/root identity* can do. `md_launch_identity` returns `-EACCES` on
a mismatch. That is the unprivileged-reachable subset.

| # | line | call | label |
|---|---|---|---|
| 0 | `:52-53` | `md_launch_identity(getuid,geteuid,getgid,getegid)` | `guest runtime requires the selected shell or root identity` |
| 1 | `:55` | `pidfd_open(getpid(),0)` | `probe pidfd_open` |
| 2 | `:57` | `pidfd_send_signal(process, 0, 0, 0)` | `probe pidfd_send_signal` |
| 3 | `:60` | `prctl(PR_SET_CHILD_SUBREAPER, 1)` | `probe child subreaper` |
| 4 | `:62` | `faccessat2(AT_FDCWD, "/proc/self/exe", 1, AT_EACCESS)` | `probe faccessat2` |
| 5 | `:64-66` | `mmap(RW,PRIVATE\|ANON)` then `mprotect(...,PROT_READ\|PROT_EXEC)` | `probe executable mappings` |
| 6 | `:69` | `md_interception_install(0)` (seccomp filter + `SECCOMP_FILTER_FLAG_NEW_LISTENER`) | `probe syscall adapter` |
| 7 | `:71-75` | raw `svc #0` with `x8=SYS_fstat`, bad fd; requires `result == -EBADF` exactly | `probe external syscall dispatch` |
| 8 | `:76` | `md_adapter_buffer_install()` | `probe atomic descriptor publication` |
| 9 | `:78` | `md_adapter_buffer_take()` | `probe mapped request lease` |
| 10 | `:80-81` | `md_guest_buffer_control(MD_BUFFER_PROBE,0,0)` + `md_adapter_buffer_release()` | `probe private-table helper` / `probe mapped request release` |
| — | `:84-86` | `write(1,"...ready\nmachine-entry=1\n")`; `exit_group(0)` | success |

**Not probed:** ptrace and its refutation in §0, real cgroup authority, and mount or userns. The floor is Linux 5.15
The file `kernel-compatibility.md:9` sets that floor. Line `:144` names the assumed baseline:
"pidfds, selective ptrace, seccomp notifications, `openat2`, `faccessat2`, `close_range` and shared/sealed memfds".
Line `:152` requires `SECCOMP_ADDFD_FLAG_SEND` at 5.14 or higher for FD publication. So the probe certifies **only**
the unprivileged subset the shell uid can reach. The privileged primitives are assumed.

### The PDN equivalent, in Android order

PDN's natural home is `podnext doctor` at `crates/podnext/src/doctor.rs`. Today it checks the proot
binary, TMPDIR, `/dev/shm`, dns, host kernel, `/proc/self/cgroup` and the guest `os-release`/`shm`/
argv, and **probes no capability primitive**. The R check `grep -n 'unshare\|mount\|SIGSYS\|pidfd_open\|prctl\|faccessat2\|openat2' doctor.rs` returns none. A PDN admission list follows, in the order that fails cheapest-first:

1. **identity**: which uid the executor will run as, the Termux shell and not root. This gate comes first. Every later
   check depends on, exactly as `bootstrap.c:52-53`.
2. **`prctl(PR_SET_CHILD_SUBREAPER,1)`**: PDN's own missing reaper, roadmap §1.2. A failure here voids the soak
   guarantee.
3. **`pidfd_open` + `pidfd_send_signal`**: the race-free kill primitive the shim uses for escrow.
   It is worth a check of whether the kernel exposes it.
4. **`faccessat2` / `openat2`**: the descriptor-VFS primitives. Roadmap §6.2 calls these `dirfd`.
5. **`mmap(RW)` + `mprotect(RX)`**: executable mappings. Guest JITs such as V8 and .NET, and any future
   in-process gate need it, and Android SELinux can refuse `PROT_EXEC` on app data.
6. **seccomp filter install with `SECCOMP_FILTER_FLAG_NEW_LISTENER`**. This matters only if PDN ever owns its own
   gate, roadmap §5. On the proot path it is informational.
7. **external syscall dispatch**: a raw `svc` returning the expected errno. This asks whether a userspace trap
   even reachable under the current bionic filter.

**Competitive test.** proot-distro has **no** admission probe. Does PDN lose to it without one? **No.**
**v0.3.0 obligation: NO.** It is CAPABILITY insurance and a better `doctor`, not a mobility
requirement. It is cheap, and it is the honest way to say *why* a guest fails on a given kernel.
- **Acceptance test.** `podnext doctor --probe` prints one named pass/fail/fail-reason per check
  above, in that order, on this device. Each line carries the errno it saw. A check that cannot be
  attempted reports `unknown` rather than `ok`, like the existing Verdict model in `doctor.rs`.

---

## 4. shroot — the cgroupfs VIEW versus policy ENFORCEMENT

**(R) `src/cgroup.c`, 1,239 lines, is a userspace cgroupfs view and not enforcement.** It owns a private
file enum (`cgroup.c:25-27` `PROCS, THREADS, EVENTS, CONTROLLERS, SUBTREE, TYPE, FREEZE, ALLOW, DENY,
LIST, TASKS, NOTIFY, RELEASE, PIDS_MAX, PIDS_CURRENT, PIDS_EVENTS`), per-file modes
at `:32-36`, for example `[PROCS]={"cgroup.procs",0644,...}` and `[PIDS_CURRENT]={"pids.current",0444,...}`.
answers `mkdir`/`subtree_control` itself (`:755 md_cgroup_mkdir` → real `mkdirat` of a backing dir at
`:766`. It runs `:872 subtree_write` and `:878` returns `-EOPNOTSUPP` for an unsupported control. It backs control
files with real files at `:485 openat(...,O_CREAT|O_EXCL,...)`, and produces its own errno
at `:781-986`. The check `grep -rn 'sys/fs/cgroup' src/*.c src/*.h` returns no matches. The host cgroupfs is never
touched.

**Its own boundary statement (R), `README.md:214-216`:**

> Cgroups enforce hierarchical pids limits and freezer state, and provide sampled CPU/RSS accounting.
> CPU quotas, cpuset affinity, memory and device policies remain compatibility state, not enforced
> limits.

**Precise split, confirmed from source:**

- **Enforced in userspace by the supervisor:** a hierarchical **pids limit**. The charge check walks
  ancestors at `cgroup.c:279 for (g=group;g;g=g->parent) if (g->pids_charged>=g->pids_max) return 1;`,
  with a write at `:1062-1068`, and a counter reconciled at `:980`. The **freezer** does a walk at `:301`, a read at `:972`
  read, `:1072-1076` write + wakeup, backed by owned `PTRACE_INTERRUPT` stops
  at `kernel-compatibility.md:723`. It also covers **membership**, `procs`/`tasks`/`events`, and **sampled CPU/RSS
  counters** at `:983-986`, `cpu.stat` and `memory.current/peak/stat`.
- **View only, stored and never enforced:** all of `cpu.weight/cpu.max/.burst`, all `cpuset.*`, all
  write-side `memory.*`, `min/low/high/max/swap.max/oom.group`, and `devices.allow/deny`. These are declared in
  `cgroup_resources.c:12-33` and bounded by `README.md:215`.

**How Docker still runs there**, per roadmap §4.4 and DISSECTION_SHROOT §4. Not by enforcement, but by a
private cgroup *namespace view* plus the real kernel operations the executor is permitted to perform.
That permission is load-bearing and is exactly what PDN's executor lacks.

**PDN's current behaviour is worse than either half.** `crates/pn-next/src/supervisor.rs:1694-1724`
`setup_cgroup` creates `runtime_dir/cgroups/<name>/` and **writes `cgroup.procs`, `cpu.max`,
`memory.max`, `pids.max` into ordinary files**, returning `Ok(())`. Those writes succeed on a plain
directory and **enforce nothing**. There is no host cgroupfs, no errno and no signal to the operator. shroot at
least refuses what it cannot honour with `-EOPNOTSUPP`. proot-distro simply does not claim limits.

**Competitive test.** Does PDN lose to proot-distro on cgroup *enforcement*? **No**. proot-distro
enforces nothing and `/sys/fs/cgroup` is read-only here, per roadmap §1.2. Does PDN lose on *honesty*?
**Yes, mildly**: it writes numbers that mean nothing. **v0.3.0 obligation: make the view honest, do
not build enforcement.** Concretely: either refuse with a named `-EOPNOTSUPP`-style error, or label the
values "compatibility state, not enforced" in `doctor`/`known-limitations.md`, mirroring
`README.md:214-216`.
- **Acceptance test.** With `pids_limit` set, the supervisor either reports `unavailable: no delegated
  cgroup subtree`, and `doctor` says so on both host and guest, or the limit is *shown to bind*. A
  guest that exceeds `pids.max` is refused. A silent `Ok(())` with no effect fails the test.
- **Enforcement itself, for CPU, cpuset, memory and devices, is a DEAD END for PDN**, roadmap §8. It needs a rootful
  executor and a delegated subtree. `Signal`/`setup_cgroup` cannot supply either on Android.

---

## 5. shroot — selective seccomp interception, and the MINIMAL PDN version

### What it is (R)

Two seccomp return paths, chosen per syscall category:

- **`SECCOMP_RET_TRACE` with a per-category cookie** for calls needing adaptation:
  `src/fd_filter_program.c:66` maps `ADAPT` → `MD_INTERCEPT_DISPATCH` for `dup`/`dup3`, `:80`
  `CONTROL(...,MD_INTERCEPT_PROC)`, and `:107` applies `SECCOMP_RET_TRACE|MD_INTERCEPT_WATCH` for
  read/readv/ioctl/splice/tee/sendfile on a selected fd, `:117` applies
  `MD_INTERCEPT_MEMORY` for `mprotect(PROT_WRITE)`, and `:139` applies `MD_INTERCEPT_PROC` for `fcntl`
  seal/getfl on the selected fd. The global filter at `src/interception.c:282,284` mirrors this.
  Categories **not** selected run native. `NATIVE(n) → SECCOMP_RET_ALLOW` applies at `interception.c:286`.
- **`SECCOMP_RET_USER_NOTIF`** for descriptor publication, installed with
  `SECCOMP_FILTER_FLAG_NEW_LISTENER` (`interception.c:521-522`: `prctl(PR_SET_NO_NEW_PRIVS,1,...)`
  then `seccomp(SECCOMP_SET_MODE_FILTER, ...NEW_LISTENER, &program)`), and served with
  `SECCOMP_IOCTL_NOTIF_ADDFD` + `SECCOMP_ADDFD_FLAG_SEND` (`fd_channel.c:64-66`,
  fd_install.c:50-52`), validated by `SECCOMP_IOCTL_NOTIF_ID_VALID` (`fd_install.c:14,36,59`).
- **arm64 register detail (R)** `interception.md:27-28`: the dispatch gate receives policy and the
  retained descriptor kind in **x9**, *not* x7, because the kernel restores x7 at an ARM64 syscall
  stop, per `arch/arm64/kernel/ptrace.c#L1694-L1726`. x10 can carry a leased input-buffer address
  Line `:36`.
- **The gate is one ptracer per tree, not ptrace-everything**. `supervisor.c:2956` is the
  `PTRACE_SETOPTIONS` call, a debugger-options setter from §0. `supervisor.c:2624` is the SVC#0
  The encoding test `(word & 0xffe0001f) == 0xd4000001` comes at `:2623`, after the registry rewrite.

### PDN's state today

**PDN has no seccomp gate. It has a stub that looks like one.** `crates/proot-core/src/seccomp.rs`
holds a `SeccompFilter` with an `allowed_syscalls` vector of raw numbers at `:28-58`, and an
`install()` that sets `PR_SET_NO_NEW_PRIVS` and then **does nothing**. Its own comment reads: *"In a full
implementation, we intend to build and install a BPF filter ... For now, we just document the intent"*
at `seccomp.rs:72-76`. `filter_program` stays empty. The struct is constructed at `lib.rs:158` but never
gates a syscall. On the proot path this is correct: **proot owns interception** (roadmap §2.1,
`proot_sysnums[]` static filter, `PROOT_NO_SECCOMP` gate). What PDN must stop claiming is that it
installs a filter. It does not.

### The minimal version, if PDN ever needs it

Only two futures need it: (a) a proot-free backend, or (b) returning a *graceful* errno for a
syscall Android refuses rather than letting proot's blanket handling decide (roadmap §7.2
"Distro-Tailored Seccomp Profiling"). Minimal = **both halves tiny, neither re-implemented here**:

1. A BPF program of a few hundred instructions: `SECCOMP_RET_TRACE|cookie` for a short selected set
   the numbers PDN already lists, and `SECCOMP_RET_ALLOW` for everything else. There are no per-fd selector
   masks (shroot's `fd_filter_program.c` machinery exists to shadow *descriptor metadata*, which PDN
   does not model).
2. `NEW_LISTENER` + a notifier loop using `SECCOMP_IOCTL_NOTIF_ADDFD` **only** for the one job it is
   irreplaceable at: publishing a fd to the guest without a ptrace stop. This is the descriptor-publication
   primitive. Everything else can return `-ENOSYS`/`-EPERM` from a plain `TRACE` handler.

**Competitive test.** proot-distro does not own a seccomp gate either. It relies on proot. Does PDN
lose without one for v0.3.0? **No.** **v0.3.0 obligation: NO. The deliverable is honesty plus one test.**
- **Acceptance test.** `doctor` reports seccomp as `unknown`/`not-owned` on the proot path rather than
  `ok`, since it never installs one. The number of BPF instructions PDN actually installs is **zero**,
  asserted. If a future gate lands, the test flips to: a guest calling an unselected syscall runs it
  natively with no ptrace stop, a selected one is intercepted, and the selected set is a subset of the
  documented numbers.

---

## 6. shroot — the descriptor-origin table, and the MINIMAL PDN version

### What it is (R)

A **sparse metadata table keyed by guest FD number** that retains *anchors and flags*, never
read/write descriptions or offsets, per `descriptor-origins.md:11-12`. The structure is
`fd_origins.c:25-32` with `object`, `channel`, `loan`, `fd`, `flags` and `uncertain`, and no rdwr or offset fields.
It is sparse via `enum { BUCKETS=61 }` at `:13`. It **follows `CLONE_FILES` independently of `CLONE_FS` and
mount-view ownership**, per `descriptor-origins.md:13-15`. `fd_origins.c:134-136 md_fd_origins_fork(t,
shared)` returns one shared table when `CLONE_FILES` is set, else a deep copy. The caller keys only on
that flag, at `task_resources.c:40 out->files=md_fd_origins_fork(r->files,!!(flags & CLONE_FILES));`.
`CLONE_FS` is tracked separately on the fs-context owner. The **positive, allocation-free IO proof**
`md_fd_origins_native_regular` at `fd_origins.c:199-202` requires the *ordinary anchor object* and
`native.kind==S_IFREG`, declining on loan/uncertain/channel/selector cases
at `:193-195`. The supervisor uses it to resume an untouched kernel call only when it holds sole task
ownership, at `supervisor.c:3754`. The contract **"an absent entry means unknown provenance, not
EBADF"** is code, not just prose: a missing fd yields `e=NULL` → returns the selectors of NULL with no
error at `:175-177`, whereas an *uncertain* entry yields `-ESTALE` at `:176`.

### The minimal version for PDN

**None exists and none is needed while proot owns interception.** The table only pays off for a
runtime that intercepts *some* syscalls and must decide, per fd, whether the syscall can bypass the
gate. PDN's analogous discipline already lives at the *filesystem* layer, descriptor-first and
TOCTOU-safe: `crates/proot-distro-core::dirfd` from roadmap §6.2, identity-checked atomic writes
in `tests/atomic_identity.rs`, the `ESTALE` refusal, and the session file held open with `FD_CLOEXEC`
cleared at `session.rs:248-260`, so liveness is "some process holds the inode", never a PID
at `session.rs:93`. That is the PDN-shaped version of "an anchor identifies the object, a bare number
does not." A guest-fd provenance table is redundant under proot.

Minimal, **if** a proot-free gate arrives: a `HashMap<RawFd, FdOrigin{ kind, flags, dev, ino }>`,
propagated on a `CLONE_FILES` copy, **never storing offsets**, with absent = unknown, not an error, and
an `uncertain` bit that yields `ESTALE`. That is ~100 lines, not shroot's 600.

**Competitive test.** proot-distro has nothing of the kind. Does PDN lose without it for v0.3.0?
**No.** **v0.3.0 obligation: NO.** It is future R&D tied to §5, and naming its acceptance test now
without the build is the honest position.
- **Acceptance test, for a later build.** An fd with no entry is treated as unknown provenance and the
  syscall runs natively and never `EBADF`. An `uncertain` entry fails with `ESTALE`. A `CLONE_FILES`
  fork shares one table while a plain fork copies it. These are three assertions and all pure functions.

---

## 7. containerd-shim — the detach supervisor seen from the other side

**(R) Caveat first: the tree is a subset.** `cmd/containerd-shim-runc-v2/`, `pkg/shim`, `pkg/cio`,
`plugins/runtime/v2/` and `vendor/.../ttrpc` are **absent**. Every claim about *what the shim binary
does internally* is therefore **UNVERIFIED by source**. Only the containerd-side counterpart is
readable. `github.com/containerd/ttrpc` is the library that will settle the fd-passing question.

What *is* readable, and what it means for PDN:

- **A shim survives containerd by design.** `core/runtime/v2/command_unix.go:23-27` returns
  `&syscall.SysProcAttr{Setpgid: true}` with **no death signal**. The only `Pdeathsig` in non-vendor
  `core/` is a userns mount helper at `core/mount/mount_idmapped_utils_linux.go:46`. Killing containerd
  therefore does not signal the shim, which is *exactly* PDN's `run --detach` supervisor. It uses `setsid`,
  `PPid=1`, #68 and #57. **Confirmed, R.**
- **stdio crosses as a path, not an fd, on the wire**: `CreateTaskRequest` carries `Stdin, Stdout,
  Stderr string`, per `api/runtime/task/v3/shim.pb.go:52-54`. These are **FIFO paths**, or v3 streaming
  URLs at `internal/cri/io/helpers.go:79-109,168-184`. Whether ttrpc *also* passes fds is
  **UNVERIFIED**, since `github.com/containerd/ttrpc` is not vendored. PDN's unix-socket and append-log session
  is the same shape: a named rendezvous plus a log, not an inherited descriptor.
- **Reconnect is by discovery**: `core/runtime/v2/shim_load.go:39-64 LoadExistingShims` scans the
  state dir per namespace, `:66-126 loadShims` per task id, and the connection record is cached at
  `bundle/bootstrap.json` at `shim_manager.go:392-420 restoreBootstrapParams`. PDN's session files are
  keyed by pid in a scanned `sessions/` dir with a `.sock` beside each, at `session.rs:45-63`. The same
  `"find it by looking, not by remembering"` rule.
- **Exit status is cached across restart**: `shim.go:576-593 recordTaskDeleteResult` stores the last
  exit. A retried Delete returns it even when the shim answers `NotFound`, at `:702-710`. When the shim
  is dead, the shim binary is invoked with action `delete` and its `task.DeleteResponse.ExitStatus` is
  parsed at `core/runtime/v2/binary.go:204-215`, with `255` as the no-response fallback
  at `shim.go:174-181`. PDN reports exit via the same triple, §2.1, and clears the session on the
  ladder's `wait_and_clear`.

**Competitive test.** Does PDN lose to proot-distro on session survival? **No**. proot-distro has no
detach/attach. PDN's #57 round trip is ahead. **v0.3.0 obligation: none beyond keeping it green.**
- **Acceptance test, cheap to adopt.** Kill the *launcher/daemon* while a `--detach` session runs. The
  session survives. `podnext ps` finds it by a scan of the state dir. A second `attach` reaches the
  same guest pid, the discovery rule from `shim_load.go`, already PDN's design.

---

## 8. DEAD ENDS for PDN, with the reason

1. **runc's cgroup SIGKILL path**, `signalAllProcesses` → `cgroup.kill`, `init_linux.go:691`. It needs a
   delegated cgroup subtree. `/sys/fs/cgroup` is read-only for the Termux uid and `CapEff=0`, per roadmap
   §1.2. PDN's ladder is the only reachable kill path.
2. **runc's namespace entry**. `unshare(CLONE_NEWPID|NEWNET)` gives `EPERM`, `CLONE_NEWUSER` gives `EINVAL`, per roadmap
   §1.2. The OCI lifecycle's whole reason to exist is namespaces. Without them it is a verb table PDN
   does not need, per §1.1.
3. **runc's `init` re-exec + `exec.fifo` bootstrap** (`container_linux.go:530`, `standard_init_linux.go:264-269`,
   `nsenter/nsexec.c:322,867`). Incompatible with the proot tracee model: proot substitutes its loader
   and traces the guest. A re-exec'd init will fight it. Take only the FIFO *idea* for supervisor
   readiness, if anything.
4. **runc's real seccomp/capability order**, `standard_init_linux.go:152-303`. It presupposes owning
   the child's uid/caps/namespaces. PDN under proot owns none of that.
5. **shroot's cgroup *enforcement***, for CPU, cpuset, memory and devices, per `README.md:215`. shroot does not
   enforce it. It needs a rootful executor. PDN cannot and must not reimplement 1,239 lines the
   reference refuses to enforce. Take the *view-or-refuse* posture only (§4).
6. **shroot's `--probe` as a hard gate.** It is a diagnostic. To make PDN refuse to start without it
   will be stricter than proot-distro and pure capability-loss. Adopt it as `doctor` output, §3.
7. **shroot's SVC-code scanning / lazy machine-entry**, `supervisor.c:2623-2624`. Requires owning and
   a rewrite of the guest's instruction pages. Under proot the svc sites belong to proot's loader.
8. **shroot's `SECCOMP_RET_USER_NOTIF` descriptor publication + the descriptor-origin table** for
   v0.3.0, §5 and §6. A second syscall gate *inside a proot guest* is double interception. The
   denormalization only pays in a proot-free runtime.
9. **runc's detach-keys.** There is no `detach-keys`/`DetachKeys` symbol anywhere in runc. The R grep confirms this.
   detach handling lives in the *consumer*, Docker or containerd. To copy it from runc is pointless.
   It copies nothing.
10. **A full OCI runtime surface for v0.3.0**: create, start, exec, kill, delete and state. Roadmap 10.1-10.4
    never asks for it. proot-distro does not have one. The UML backend, §5.1, is where a real OCI
    runtime belongs, and it is a later release.

---

## 9. Verdict table for v0.3.0

| # | mechanism | file:line anchor | axis | PDN needs it? (competitive test) | acceptance test |
|---|---|---|---|---|---|
| 1 | subreaper + reap loop | `signals.go:24,130-150`; `run_step.py:_become_subreaper` | STABILITY | **YES** — proot-distro reaps, PDN does not | double-fork guest leaves zero leftovers; control goes red |
| 2 | exit-status triple | `utils.go:12-23`; PDN `lib.rs:549-563` | STABILITY | **no (done)** — keep the contract test | signal death → 137; clean → own code; 255 reserved |
| 3 | kill ladder w/o pidns | `container_linux.go:407`; PDN `container.rs:388` | STABILITY | **no (done)**; runc's cgroup path is a dead end | guest ignoring TERM escalates TERM→QUIT→KILL; `-s STOP` not escalated |
| 4 | EOF via Copy+Close | `tty.go:56-59,129-131`; PDN `native/session.rs:505-522` | STABILITY | **no** — detach/attach already ahead of proot-distro | Ctrl-D exits guest + reaps; client drop reattaches |
| 5 | honest cgroup view | `README.md:214-216`; PDN `supervisor.rs:1694-1724` | STABILITY | **YES (honesty only)** — PDN writes limits that bind nothing | either a named `unavailable` or a limit shown to bind; silent `Ok()` fails |
| 6 | admission probe | `bootstrap.c:54-87`; PDN `doctor.rs` | CAPABILITY | **NO** — proot-distro has none | `doctor --probe` prints named pass/fail/errno per check, in order; un-attemptable = `unknown` |
| 7 | selective seccomp gate | `interception.c:521-522`, `fd_filter_program.c:66-139`; PDN `seccomp.rs:72-76` (stub) | CAPABILITY | **NO** — proot owns it | PDN installs **zero** BPF instructions today; `doctor` says `not-owned` |
| 8 | descriptor-origin table | `descriptor-origins.md:11-16`, `fd_origins.c:199-202` | CAPABILITY | **NO** — future R&D only | (whenever built) absent=unknown, uncertain=ESTALE, CLONE_FILES shares / fork copies |
| 9 | session survives restart | `command_unix.go:23-27`, `shim_load.go:39-126`; PDN `native/session.rs:531-556` | STABILITY | **no (done)** — adopt the discovery test | kill the launcher; `ps` finds the session; attach reaches same pid |
| 10 | OCI lifecycle verbs | `create.go:63` … `run.go:76` | — | **NO** for v0.3.0 | create-killed-midway leaves no orphan/half state (the only take) |

**One-line answer to section 10:** the execution model PDN needs for v0.3.0 is **not a new runtime**.
It is two small STABILITY fixes, a subreaper and a stop to writing cgroup limits that bind nothing, one
honesty fix, a stop to claiming a seccomp filter PDN does not install, and tests that lock the contracts
PDN already meets, the exit triple, the kill ladder and detach/attach. The heavy machinery in all three
comparators, namespaces, cgroup enforcement, SVC rewriting, a descriptor-origin table and an OCI
lifecycle, is either unreachable on Android or belongs to the proot-free/UML backend, and is
recorded as a dead end with its reason rather than scheduled.
