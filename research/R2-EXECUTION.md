# R2-EXECUTION — the execution layer, re-dissected from raw source

Round-2 re-read of the three comparators that carry the *execution* model, answering against
`docs/src/roadmap.md` section 10 (the v0.3.0 mobility target, gaps 10.1–10.5). Every claim is a
`file:line` in the raw tree, re-tested this round rather than quoted from the earlier pass.

**Sources (read-only, verified this round):**

| tree | path | scale (measured) |
|---|---|---|
| runc v1.5.2 | `~/.hermes/cache/scratch/src/runc-src` | 930 `.go` files |
| shroot (mekhontsev/magicdesk @ `7ab99d6`) | `~/.hermes/cache/scratch/magicdesk/native/shroot` | 348 C files / 86,018 lines C |
| containerd v2.4.2 (shim side) | `~/.hermes/cache/scratch/src/containerd-src` | tree is a **subset** — see §7 |

**Markers.** **(R)** read from source, line quoted. **(M)** measured on this device. **(I)** inference
from read code. **UNVERIFIED** checked and unsettled, names what would settle it.

**The four axes** (roadmap §10, verbatim intent): **MOBILITY** (same daily-driver reach as
`proot-distro`), **CAPABILITY** (what a guest can actually do), **STABILITY** (no orphans, honest
exit/errno), **OPTIMIZATION** (cost per operation).

**The competitive test, defined once.** *Would PDN lose to the installed `proot-distro` on this
device without this mechanism?* If proot-distro does not have it, or has it and PDN already matches,
the answer is **no** and the mechanism is not a v0.3.0 obligation — it is recorded as optional or a
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
(`libcontainer/container.go:16-25`); OCI strings add `creating` for hook visibility
(`vendor/.../specs-go/state.go:8-18`, emitted transiently at `process_linux.go:972,1034`).
Status is **recomputed from `/proc`, never trusted from disk**: `refreshState`
(`container_linux.go:874-891`) treats `!hasInit()` as stopped and the presence of `exec.fifo`
as created. `state.json` is written atomically (`container_linux.go:838` temp → `:860` rename) and
**inside the readiness handshake** — `process_linux.go:1007 updateState` fires on the `procReady`
message, not before it.

**PDN's counterpart, by verb (all read this round):**

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
recompute discipline** (`hasInit` + FIFO probing, `container_linux.go:874-891`): PDN's
`process_is_alive(pid, start_time)` (`crates/proot-distro-core/src/session.rs:93`) already applies
the *same* rule — `(pid, kernel start-time, not-a-zombie)` — which is the runc contract, arrived at
independently.

**Competitive test.** Does PDN lose to proot-distro by not exposing an OCI lifecycle? **No.**
proot-distro exposes none either; both are `install/login/run/remove` front-ends. Roadmap §10.1–10.4
do not ask for one. **v0.3.0 obligation: none.** Keep the two cheap tests below; build nothing.

- **Acceptance test (create/status honesty).** `podnext` killed mid-`create` (SIGKILL between
  container-dir creation and metadata write) leaves **no orphan process and no half-written
  `metadata.json`** — counts of both taken before and after. Mirrors the failure mode runc documents
  and guards against by writing state inside the readiness handshake.
- **Acceptance test (liveness).** A recycled PID is not reported as a live session, and a zombie is
  not reported running: `ps` after `kill -STOP` + upstream reap returns `stopped`, not `running`.

### 1.2 The mechanism PDN genuinely lacks: a subreaper (STABILITY)

runc's top-level process becomes a subreaper before it signals anything
(`signals.go:24 system.SetSubreaper(1)` → `unix.Prctl(PR_SET_CHILD_SUBREAPER,...)`) and reaps with
`Wait4(-1, &ws, WNOHANG, &rus)` in a loop (`signals.go:130-150`). `runc init` itself **does not
reap** — after `standard_init_linux.go:303 linux.Exec(...)` it is the user's binary; the parent is the
reaper precisely because nsexec forked with `CLONE_PARENT` (`nsenter/nsexec.c:322`).

PDN has **no subreaper at all**. proot-distro does (`run_step.py:_become_subreaper`, NEXT_DEV doctrine
7). Without it, a guest that double-forks (daemons, `apt`, `systemd --user`) reparents to Android's
init and PDN cannot find or kill it — the soak harness (`scripts/soak_sessions.py`) would report
leftovers that `podnext kill` cannot reach.

**Competitive test.** Does PDN lose to proot-distro without it? **Yes** — proot-distro reaps its
build/run descendants; PDN's kill ladder can only reach processes it still owns. **v0.3.0 obligation:
YES.** Cost: one `prctl(PR_SET_CHILD_SUBREAPER,1)` at launcher start plus a `wait4(-1,WNOHANG)` drain.

- **Acceptance test.** A guest command that double-forks (`sh -c '(sleep 999 &) ; exit'`) leaves
  **zero** surviving processes after the launcher exits, proved by counting `proot`/`podnext` rows in
  `/proc` before and after, with a negative control that goes red when the subreaper call is removed.

### 1.3 The one part of runc's create the competitive test does not ask for

`container_linux.go:530 exec.Command(exePath, "init")` (fork/exec of runc's own binary with argv[1]
== "init"), the `exec.fifo` handshake (`container_linux.go:233-254` parent reads; `standard_init_linux.go:264-269`
child writes one byte), and the nsexec `CLONE_PARENT` staging (`nsenter/nsexec.c:322,867`) together
exist **only to enter namespaces and hand a container its PID 1**. PDN has no namespaces (roadmap
§1.2: `unshare(CLONE_NEW*)` → `EPERM`, `CapEff=0`) and does not re-exec an init — proot substitutes
its own loader for the guest program. Copying runc's bootstrap into PDN is **incompatible with the
proot tracee model** and is a dead end (§8). The FIFO idea alone (a one-byte readiness gate instead of
a sleep/poll) is *reusable* for PDN's own supervisor readiness, but that is an internal nicety, not a
v0.3.0 obligation.

---

## 2. runc — exit codes, signals without a PID namespace, stdio EOF

### 2.1 Exit status: the triple (STABILITY)

**(R)** Exactly one place turns a wait into a process exit code:

```go
// libcontainer/utils/utils.go:12-23
const ( exitSignalOffset = 128 )
func ExitStatus(status unix.WaitStatus) int {
    if status.Signaled() { return exitSignalOffset + int(status.Signal()) }
    return status.ExitStatus()
}
```

- signal death → `128 + N`; clean exit → the child's own status; call site `signals.go:148
  status: utils.ExitStatus(ws)`.
- `255` is a **runtime failure**, from two origins: init could not report over the pipe
  (`libcontainer/init_linux.go:122 os.Exit(255)`) or exec failed (`exec.go:114 fatalWithCode(...,255)`).
- there is **no symbol named `waitStatus`** in the tree (R: grep); the wait status is a local
  `ws unix.WaitStatus` at `signals.go:132`.

**PDN already implements the exact triple** (`crates/proot-core/src/lib.rs:549-563`,
`exit_status_code_parts`), with the 255 branch tested and the failure it closed documented (A6:
collapsing a signal death to -1 made 137/139/143 indistinguishable from a runtime failure).
**v0.3.0 obligation: none — it is done.** The acceptance test already exists
(`crates/podnext/tests/exit_codes.rs`, 7 cases); the remaining value is the *contract* being asserted
against a signal death (`kill -9` in the guest → 137), which the 255-and-signal branches cover.

### 2.2 Signals without a PID namespace (STABILITY)

**(R)** `Container.Signal` (`container_linux.go:396-425`) branches on the namespace:

```go
// container_linux.go:407
if s == unix.SIGKILL && !c.config.Namespaces.IsPrivate(configs.NEWPID) {
    if err := signalAllProcesses(c.cgroupManager, unix.SIGKILL); err != nil { ... }
    return nil
}
return c.signal(s)
```

- Everything except SIGKILL goes to **init only** (`process_linux.go:139-141
  p.cmd.Process.Signal(sig)`); with a private pidns init==PID1 and the kernel broadcasts, without one
  only the init pid hears it (`container_linux.go:400-406`, verbatim).
- The no-pidns SIGKILL path calls `signalAllProcesses` (`init_linux.go:684-720`): cgroup v2 writes
  `cgroup.kill=1` (`:691`), else freeze → `GetAllPids` → `kill` each (`:699-715`) → thaw.
- **No `PidfdSendSignal` in runc core** (R: grep of non-vendor) — pidfds are used only for fd-passing
  (`process_linux.go:1108,1113`; `init_linux.go:727-732`).

**This is PDN's burned case** (roadmap §2.1: `kill -9` to PRoot orphans the tracees; proot ignores
SIGTERM/SIGINT/SIGHUP). PDN's answer is the kill ladder
(`container.rs:388 [SIGTERM,SIGQUIT,SIGKILL]`) plus inode/`(pid,start_time)` session clearing
(`session.rs:93`, `wait_and_clear`), which is the proot-distro route, not runc's.

**The runc route is a dead end here.** `signalAllProcesses` needs a delegated cgroup subtree to write
`cgroup.kill`, and on this device `/sys/fs/cgroup` is read-only for the Termux uid and `CapEff=0`
(roadmap §1.2). PDN cannot use cgroup-kill; the ladder is the only reachable path.
**Competitive test.** Does PDN lose to proot-distro here? **No — PDN already has the ladder.** The
gap is not the ladder but the *reaper behind it* (§1.2): the ladder can only signal processes PDN
still owns. **v0.3.0 obligation: the subreaper only; keep the ladder.**
- **Acceptance test.** A guest that ignores `SIGTERM` is escalated `TERM→QUIT→KILL`, and
  `kill -s STOP` is delivered once and never escalated (`container.rs:455`, the
  `_NON_TERMINATING_SIGNALS` set). Coverage exists; keep it green.

### 2.3 stdio EOF and PTY handling (STABILITY)

**(R)** EOF propagation is `io.Copy` returning:

```go
// tty.go:56-59 (pipes)
go func() { _, _ = io.Copy(i.Stdin, os.Stdin); _ = i.Stdin.Close() }()
// tty.go:129-131 (pty)
go func() { _, _ = io.Copy(epollConsole, os.Stdin) }()
go t.copyIO(os.Stdout, epollConsole)
```

The `Close()` after `Copy` is what turns the host's stdin EOF into the container's EOF — no signal, no
out-of-band marker. PTY side: `console_linux.go:100` opens `/dev/pts/ptmx`, `:121
console.NewPtyFromFile` (note: **not** `console.NewPty` — absent), `init_linux.go:410
cmsg.SendRawFd(socket, pty.Name(), pty.Fd())` passes the master over a unix socket, and
`dupStdio` (`init_linux.go:158-166`) binds the peer to 0/1/2.

**PDN already has the PTY half** (`native/session.rs:505-522`: `openpty`, `setsid`, `TIOCSCTTY`,
`dup2` slave onto 0/1/2) and a socket + append log for attach (`:531-556`). What it has *not* wired is
runc's exact "Copy-returns-then-Close" EOF discipline on the *host* side: the session master is
drained non-blocking on the reactor (`:558-561`), and the declared limitations are that a resize while
attached is not re-forwarded and detached output is appended rather than replayed
(`known-limitations.md`, roadmap §9.3).

**Competitive test.** Does PDN lose to proot-distro without runc's exact EOF path? **No** —
proot-distro's `login` has no detach/attach at all, so PDN is ahead. **v0.3.0 obligation: none**;
the remaining items are the two declared session limitations, not execution-model gaps.
- **Acceptance test.** Attached session: sending EOF (Ctrl-D) at the client makes the guest shell
  exit and the supervisor reap it — same guest pid, no orphan; a client drop without EOF leaves the
  guest running and reattachable (the #57 round trip).

### 2.4 The runc seccomp/setup order (reference only)

**(R)** `standard_init_linux.go`: `no_new_privs` at `:152-156`; **seccomp early** only when NNP is
false and the container is privileged (`:191-197`, before `finalizeNamespace`); **seccomp late** (as
close to `execve` as possible) when NNP is true (`:239-245`, after `finalizeNamespace`); then the
final `linux.Exec` at `:303`. This is the ordering a **tracee-owning** runtime follows. PDN does not
install a filter of its own (§5), so the order is reference knowledge, not a task.

---

## 3. shroot — the admission probe, in the order it runs

**(R)** Entry `src/bootstrap.c:54` `if (argc == 2 && md_equal(argv[1], "--probe")) {`. The probe is
**inline in `md_boot`**, there is no `probe*.c` file (R: `ls src | grep probe` → empty). Every step
fails with `md_die("<label>", rc)` → **binary pass/fail**, threshold = the call returns `>=0`, with a
single exact-value check (step 6). Runs **after** the identity gate `bootstrap.c:52-53` — the probe
only certifies what the *selected shell/root identity* can do (`md_launch_identity`, `-EACCES` on
mismatch), i.e. the unprivileged-reachable subset.

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

**Not probed:** ptrace (refuted, §0); real cgroup authority; mount/userns. The floor is Linux 5.15
(`kernel-compatibility.md:9`), and `:144` names the assumed baseline — "pidfds, selective ptrace,
seccomp notifications, `openat2`, `faccessat2`, `close_range` and shared/sealed memfds" — while
`:152` requires `SECCOMP_ADDFD_FLAG_SEND` (≥5.14) for FD publication. So the probe certifies **only**
the unprivileged subset the shell uid can reach; the privileged primitives are assumed.

### The PDN equivalent, in Android order

PDN's natural home is `podnext doctor` (`crates/podnext/src/doctor.rs`), which today checks the proot
binary, TMPDIR, `/dev/shm`, dns, host kernel, `/proc/self/cgroup` and the guest `os-release`/`shm`/
argv — and **probes no capability primitive** (R: `grep -n 'unshare\|mount\|SIGSYS\|pidfd_open\|prctl\|faccessat2\|openat2' doctor.rs` → none). A PDN admission list, in the order that fails cheapest-first:

1. **identity** — which uid the executor will run as (Termux shell, not root) — the gate every later
   check depends on, exactly as `bootstrap.c:52-53`.
2. **`prctl(PR_SET_CHILD_SUBREAPER,1)`** — PDN's own missing reaper (§1.2); fail here means the soak
   guarantee is void.
3. **`pidfd_open` + `pidfd_send_signal`** — the race-free kill primitive the shim uses for escrow;
   worth knowing whether the kernel exposes it before relying on it.
4. **`faccessat2` / `openat2`** — the descriptor-VFS primitives (roadmap §6.2 `dirfd`).
5. **`mmap(RW)` + `mprotect(RX)`** — executable mappings; guest JITs (V8, .NET) and any future
   in-process gate need it, and Android SELinux can refuse `PROT_EXEC` on app data.
6. **seccomp filter install with `SECCOMP_FILTER_FLAG_NEW_LISTENER`** — only if PDN ever owns its own
   gate (§5); on the proot path it is informational.
7. **external syscall dispatch** — a raw `svc` returning the expected errno, i.e. is a userspace trap
   even reachable under the current bionic filter.

**Competitive test.** proot-distro has **no** admission probe. Does PDN lose to it without one? **No.**
**v0.3.0 obligation: NO.** It is CAPABILITY insurance and a better `doctor`, not a mobility
requirement — but it is cheap and it is the honest way to say *why* a guest fails on a given kernel.
- **Acceptance test.** `podnext doctor --probe` prints one named pass/fail/fail-reason per check
  above, in that order, on this device; each line carries the errno it saw; a check that cannot be
  attempted reports `unknown` rather than `ok` (mirrors `doctor.rs`'s existing Verdict model).

---

## 4. shroot — the cgroupfs VIEW versus policy ENFORCEMENT

**(R) `src/cgroup.c` (1,239 lines) is a userspace cgroupfs view, not enforcement.** It owns a private
file enum (`cgroup.c:25-27` `PROCS, THREADS, EVENTS, CONTROLLERS, SUBTREE, TYPE, FREEZE, ALLOW, DENY,
LIST, TASKS, NOTIFY, RELEASE, PIDS_MAX, PIDS_CURRENT, PIDS_EVENTS`), per-file modes
(`:32-36`, e.g. `[PROCS]={"cgroup.procs",0644,...}`, `[PIDS_CURRENT]={"pids.current",0444,...}`),
answers `mkdir`/`subtree_control` itself (`:755 md_cgroup_mkdir` → real `mkdirat` of a backing dir at
`:766`; `:872 subtree_write`, `:878` returning `-EOPNOTSUPP` for an unsupported control), backs control
files with real files (`:485 openat(...,O_CREAT|O_EXCL,...)`), and produces its own errno
(`:781-986`). **`grep -rn 'sys/fs/cgroup' src/*.c src/*.h` → no matches**: the host cgroupfs is never
touched.

**Its own boundary statement (R), `README.md:214-216`:**

> Cgroups enforce hierarchical pids limits and freezer state, and provide sampled CPU/RSS accounting.
> CPU quotas, cpuset affinity, memory and device policies remain compatibility state, not enforced
> limits.

**Precise split, confirmed from source:**

- **Enforced (in userspace, by the supervisor):** hierarchical **pids limit** — the charge check walks
  ancestors (`cgroup.c:279 for (g=group;g;g=g->parent) if (g->pids_charged>=g->pids_max) return 1;`),
  write at `:1062-1068`, counter reconciled at `:980`; **freezer** — `:301` ancestor walk, `:972`
  read, `:1072-1076` write + wakeup, backed by owned `PTRACE_INTERRUPT` stops
  (`kernel-compatibility.md:723`); **membership** (`procs`/`tasks`/`events`) and **sampled CPU/RSS
  counters** (`:983-986` `cpu.stat`, `memory.current/peak/stat`).
- **View only (stored, never enforced):** all of `cpu.weight/cpu.max/.burst`, all `cpuset.*`, all
  write-side `memory.*` (`min/low/high/max/swap.max/oom.group`), `devices.allow/deny` — declared in
  `cgroup_resources.c:12-33` and bounded by `README.md:215`.

**How Docker still runs there** (roadmap §4.4, DISSECTION_SHROOT §4): not by enforcement, but by a
private cgroup *namespace view* plus the real kernel operations the executor is permitted to perform.
That permission is load-bearing and is exactly what PDN's executor lacks.

**PDN's current behaviour is worse than either half.** `crates/pn-next/src/supervisor.rs:1694-1724`
`setup_cgroup` creates `runtime_dir/cgroups/<name>/` and **writes `cgroup.procs`, `cpu.max`,
`memory.max`, `pids.max` into ordinary files**, returning `Ok(())`. Those writes succeed on a plain
directory and **enforce nothing** — no host cgroupfs, no errno, no signal to the operator. shroot at
least refuses what it cannot honour with `-EOPNOTSUPP`; proot-distro simply does not claim limits.

**Competitive test.** Does PDN lose to proot-distro on cgroup *enforcement*? **No** — proot-distro
enforces nothing and `/sys/fs/cgroup` is read-only here (roadmap §1.2). Does PDN lose on *honesty*?
**Yes, mildly**: it writes numbers that mean nothing. **v0.3.0 obligation: make the view honest, do
not build enforcement.** Concretely: either refuse (`-EOPNOTSUPP`-style named error) or label the
values "compatibility state, not enforced" in `doctor`/`known-limitations.md`, mirroring
`README.md:214-216`.
- **Acceptance test.** With `pids_limit` set, the supervisor either reports `unavailable: no delegated
  cgroup subtree` (and `doctor` says so on both host and guest), or the limit is *shown to bind* — a
  guest that exceeds `pids.max` is refused. A silent `Ok(())` with no effect fails the test.
- **Enforcement itself (CPU/cpuset/memory/devices) is a DEAD END for PDN** (§8): it needs a rootful
  executor and a delegated subtree; `Signal`/`setup_cgroup` cannot supply either on Android.

---

## 5. shroot — selective seccomp interception, and the MINIMAL PDN version

### What it is (R)

Two seccomp return paths, chosen per syscall category:

- **`SECCOMP_RET_TRACE` with a per-category cookie** for calls needing adaptation:
  `src/fd_filter_program.c:66` (`ADAPT` → `MD_INTERCEPT_DISPATCH` for `dup`/`dup3`), `:80`
  (`CONTROL(...,MD_INTERCEPT_PROC)`), `:107` (`SECCOMP_RET_TRACE|MD_INTERCEPT_WATCH` for
  read/readv/ioctl/splice/tee/sendfile on a selected fd), `:117`
  (`MD_INTERCEPT_MEMORY` for `mprotect(PROT_WRITE)`), `:139` (`MD_INTERCEPT_PROC` for `fcntl`
  seal/getfl on the selected fd); the global filter at `src/interception.c:282,284` mirrors this.
  Categories **not** selected run native — `NATIVE(n) → SECCOMP_RET_ALLOW` (`interception.c:286`).
- **`SECCOMP_RET_USER_NOTIF`** for descriptor publication, installed with
  `SECCOMP_FILTER_FLAG_NEW_LISTENER` (`interception.c:521-522`: `prctl(PR_SET_NO_NEW_PRIVS,1,...)`
  then `seccomp(SECCOMP_SET_MODE_FILTER, ...NEW_LISTENER, &program)`), and served with
  `SECCOMP_IOCTL_NOTIF_ADDFD` + `SECCOMP_ADDFD_FLAG_SEND` (`fd_channel.c:64-66`,
  fd_install.c:50-52`), validated by `SECCOMP_IOCTL_NOTIF_ID_VALID` (`fd_install.c:14,36,59`).
- **arm64 register detail (R)** `interception.md:27-28`: the dispatch gate receives policy and the
  retained descriptor kind in **x9**, *not* x7, because the kernel restores x7 at an ARM64 syscall
  stop (`arch/arm64/kernel/ptrace.c#L1694-L1726`). x10 can carry a leased input-buffer address
  (`:36`).
- **The gate is one ptracer per tree, not ptrace-everything** — `supervisor.c:2956` is the
  `PTRACE_SETOPTIONS` call (a debugger-options setter, §0), and `supervisor.c:2624` is the SVC#0
  encoding test `(word & 0xffe0001f) == 0xd4000001`, preceded at `:2623` by the registry rewrite.

### PDN's state today

**PDN has no seccomp gate; it has a stub that looks like one.** `crates/proot-core/src/seccomp.rs`
holds a `SeccompFilter` with an `allowed_syscalls` vector of raw numbers (`:28-58`) and an
`install()` that sets `PR_SET_NO_NEW_PRIVS` and then **does nothing** — its own comment: *"In a full
implementation, we would build and install a BPF filter … For now, we just document the intent"*
(`seccomp.rs:72-76`). `filter_program` stays empty. The struct is constructed (`lib.rs:158`) but never
gates a syscall. On the proot path this is correct: **proot owns interception** (roadmap §2.1,
`proot_sysnums[]` static filter, `PROOT_NO_SECCOMP` gate). What PDN *should* stop claiming is that it
installs a filter — it does not.

### The minimal version, if PDN ever needs it

Only two futures need it: (a) a proot-free backend, or (b) returning a *graceful* errno for a
syscall Android refuses rather than letting proot's blanket handling decide (roadmap §7.2
"Distro-Tailored Seccomp Profiling"). Minimal = **both halves tiny, neither re-implemented here**:

1. A BPF program of a few hundred instructions: `SECCOMP_RET_TRACE|cookie` for a short selected set
   (the numbers PDN already lists), `SECCOMP_RET_ALLOW` for everything else — no per-fd selector
   masks (shroot's `fd_filter_program.c` machinery exists to shadow *descriptor metadata*, which PDN
   does not model).
2. `NEW_LISTENER` + a notifier loop using `SECCOMP_IOCTL_NOTIF_ADDFD` **only** for the one job it is
   irreplaceable at: publishing a fd to the guest without a ptrace stop — the descriptor-publication
   primitive. Everything else can return `-ENOSYS`/`-EPERM` from a plain `TRACE` handler.

**Competitive test.** proot-distro does not own a seccomp gate either — it relies on proot. Does PDN
lose without one for v0.3.0? **No.** **v0.3.0 obligation: NO — deliverable is honesty plus one test.**
- **Acceptance test.** `doctor` reports seccomp as `unknown`/`not-owned` on the proot path rather than
  `ok` (it never installs one), and the number of BPF instructions PDN actually installs is **zero** —
  asserted. If a future gate lands, the test flips to: a guest calling an unselected syscall runs it
  natively (no ptrace stop), a selected one is intercepted, and the selected set is a subset of the
  documented numbers.

---

## 6. shroot — the descriptor-origin table, and the MINIMAL PDN version

### What it is (R)

A **sparse metadata table keyed by guest FD number** that retains *anchors and flags*, never
read/write descriptions or offsets: `descriptor-origins.md:11-12`; structure
`fd_origins.c:25-32` (`object`, `channel`, `loan`, `fd`, `flags`, `uncertain` — no rdwr/offset fields),
sparse via `enum { BUCKETS=61 }` (`:13`). It **follows `CLONE_FILES` independently of `CLONE_FS` and
mount-view ownership** (`descriptor-origins.md:13-15`): `fd_origins.c:134-136 md_fd_origins_fork(t,
shared)` returns one shared table when `CLONE_FILES` is set, else a deep copy; the caller keys only on
that flag (`task_resources.c:40 out->files=md_fd_origins_fork(r->files,!!(flags & CLONE_FILES));`);
`CLONE_FS` is tracked separately on the fs-context owner. The **positive, allocation-free IO proof**
`md_fd_origins_native_regular` (`fd_origins.c:199-202`) requires the *ordinary anchor object* and
`native.kind==S_IFREG`, declining on loan/uncertain/channel/selector cases
(`:193-195`); the supervisor uses it to resume an untouched kernel call only when it holds sole task
ownership (`supervisor.c:3754`). And the contract **"an absent entry means unknown provenance, not
EBADF"** is code, not just prose: a missing fd yields `e=NULL` → returns the selectors of NULL with no
error (`:175-177`), whereas an *uncertain* entry yields `-ESTALE` (`:176`).

### The minimal version for PDN

**None exists and none is needed while proot owns interception.** The table only pays off for a
runtime that intercepts *some* syscalls and must decide, per fd, whether the syscall can bypass the
gate. PDN's analogous discipline already lives at the *filesystem* layer, descriptor-first and
TOCTOU-safe: `crates/proot-distro-core::dirfd` (roadmap §6.2), identity-checked atomic writes
(`tests/atomic_identity.rs`, the `ESTALE` refusal), and the session file held open with `FD_CLOEXEC`
cleared (`session.rs:248-260`) so liveness is "some process holds the inode", never a PID
(`session.rs:93`). That is the PDN-shaped version of "an anchor identifies the object; a bare number
does not." A guest-fd provenance table would be redundant under proot.

Minimal, **if** a proot-free gate arrives: a `HashMap<RawFd, FdOrigin{ kind, flags, dev, ino }>`,
propagated on `CLONE_FILES` copy, **never storing offsets**, with absent = unknown (not an error) and
an `uncertain` bit that yields `ESTALE`. That is ~100 lines, not shroot's 600.

**Competitive test.** proot-distro has nothing of the kind. Does PDN lose without it for v0.3.0?
**No.** **v0.3.0 obligation: NO.** It is future R&D tied to §5, and naming its acceptance test now
without building it is the honest position.
- **Acceptance test (whenever built).** An fd with no entry is treated as unknown provenance and the
  syscall runs natively (never `EBADF`); an `uncertain` entry fails with `ESTALE`; a `CLONE_FILES`
  fork shares one table while a plain fork copies it — three assertions, all pure functions.

---

## 7. containerd-shim — the detach supervisor seen from the other side

**(R) Caveat first: the tree is a subset.** `cmd/containerd-shim-runc-v2/`, `pkg/shim`, `pkg/cio`,
`plugins/runtime/v2/` and `vendor/.../ttrpc` are **absent**. Every claim about *what the shim binary
does internally* is therefore **UNVERIFIED by source**; only the containerd-side counterpart is
readable. `github.com/containerd/ttrpc` is the library that would settle the fd-passing question.

What *is* readable, and what it means for PDN:

- **A shim survives containerd by design.** `core/runtime/v2/command_unix.go:23-27` returns
  `&syscall.SysProcAttr{Setpgid: true}` — **no death signal**; the only `Pdeathsig` in non-vendor
  `core/` is a userns mount helper (`core/mount/mount_idmapped_utils_linux.go:46`). Killing containerd
  therefore does not signal the shim — which is *exactly* PDN's `run --detach` supervisor (`setsid`,
  `PPid=1`, #68/#57). **Confirmed, R.**
- **stdio crosses as a path, not an fd, on the wire**: `CreateTaskRequest` carries `Stdin, Stdout,
  Stderr string` (`api/runtime/task/v3/shim.pb.go:52-54`), which are **FIFO paths** or (v3) streaming
  URLs (`internal/cri/io/helpers.go:79-109,168-184`). Whether ttrpc *also* passes fds is
  **UNVERIFIED** (`github.com/containerd/ttrpc` not vendored). PDN's unix-socket + append-log session
  is the same shape: a named rendezvous plus a log, not an inherited descriptor.
- **Reconnect is by discovery**: `core/runtime/v2/shim_load.go:39-64 LoadExistingShims` scans the
  state dir per namespace, `:66-126 loadShims` per task id, and the connection record is cached at
  `bundle/bootstrap.json` (`shim_manager.go:392-420 restoreBootstrapParams`). PDN's session files are
  keyed by pid in a scanned `sessions/` dir with a `.sock` beside each (`session.rs:45-63`) — the same
  "find it by looking, not by remembering" rule.
- **Exit status is cached across restart**: `shim.go:576-593 recordTaskDeleteResult` stores the last
  exit; a retried Delete returns it even when the shim answers `NotFound` (`:702-710`); when the shim
  is dead, the shim binary is invoked with action `delete` and its `task.DeleteResponse.ExitStatus` is
  parsed (`core/runtime/v2/binary.go:204-215`), with `255` as the no-response fallback
  (`shim.go:174-181`). PDN reports exit via the same triple (§2.1) and clears the session on the
  ladder's `wait_and_clear`.

**Competitive test.** Does PDN lose to proot-distro on session survival? **No** — proot-distro has no
detach/attach; PDN's #57 round trip is ahead. **v0.3.0 obligation: none beyond keeping it green.**
- **Acceptance test (adopt, cheap).** Kill the *launcher/daemon* while a `--detach` session runs: the
  session survives, `podnext ps` finds it by scanning the state dir, and a second `attach` reaches the
  same guest pid — the discovery rule from `shim_load.go`, already PDN's design.

---

## 8. DEAD ENDS for PDN (with the reason)

1. **runc's cgroup SIGKILL path** (`signalAllProcesses` → `cgroup.kill`, `init_linux.go:691`). Needs a
   delegated cgroup subtree; `/sys/fs/cgroup` is read-only for the Termux uid and `CapEff=0` (roadmap
   §1.2). PDN's ladder is the only reachable kill path.
2. **runc's namespace entry** (`unshare(CLONE_NEWPID|NEWNET)` `EPERM`, `CLONE_NEWUSER` `EINVAL` — roadmap
   §1.2). The OCI lifecycle's whole reason to exist is namespaces; without them it is a verb table PDN
   does not need (§1.1).
3. **runc's `init` re-exec + `exec.fifo` bootstrap** (`container_linux.go:530`, `standard_init_linux.go:264-269`,
   `nsenter/nsexec.c:322,867`). Incompatible with the proot tracee model: proot substitutes its loader
   and traces the guest; a re-exec'd init would fight it. Take only the FIFO *idea* for supervisor
   readiness, if anything.
4. **runc's real seccomp/capability order** (`standard_init_linux.go:152-303`). It presupposes owning
   the child's uid/caps/namespaces; PDN under proot owns none of that.
5. **shroot's cgroup *enforcement*** (CPU/cpuset/memory/devices — `README.md:215`). shroot does not
   enforce it; it needs a rootful executor. PDN cannot and should not reimplement 1,239 lines the
   reference refuses to enforce. Take the *view-or-refuse* posture only (§4).
6. **shroot's `--probe` as a hard gate.** It is a diagnostic; making PDN refuse to start without it
   would be stricter than proot-distro and pure capability-loss. Adopt it as `doctor` output (§3).
7. **shroot's SVC-code scanning / lazy machine-entry** (`supervisor.c:2623-2624`). Requires owning and
   rewriting the guest's instruction pages; under proot the svc sites belong to proot's loader.
8. **shroot's `SECCOMP_RET_USER_NOTIF` descriptor publication + the descriptor-origin table** for
   v0.3.0 (§5, §6). Building a second syscall gate *inside a proot guest* is double interception; the
   denormalization only pays in a proot-free runtime.
9. **runc's detach-keys.** There is no `detach-keys`/`DetachKeys` symbol anywhere in runc (R: grep) —
   detach handling lives in the *consumer* (Docker/containerd). Copying it from runc is copying
   nothing.
10. **A full OCI runtime surface (create/start/exec/kill/delete/state) for v0.3.0.** Roadmap §10.1–10.4
    never asks for it; proot-distro does not have one; the UML backend (§5.1) is where a real OCI
    runtime belongs, and it is a later release.

---

## 9. Verdict table (v0.3.0)

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
It is two small STABILITY fixes (a subreaper; stop writing cgroup limits that bind nothing), one
honesty fix (stop claiming a seccomp filter PDN does not install), and tests that lock the contracts
PDN already meets (the exit triple, the kill ladder, detach/attach). The heavy machinery in all three
comparators — namespaces, cgroup enforcement, SVC rewriting, a descriptor-origin table, an OCI
lifecycle — is either unreachable on Android or belongs to the proot-free/UML backend, and is
recorded as a dead end with its reason rather than scheduled.
