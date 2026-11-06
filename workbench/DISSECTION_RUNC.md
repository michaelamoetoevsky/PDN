# Dissection — runc v1.5.2 (OCI runtime) for the PDN (podnext) reimplementation

Scope: the Go source of runc, tag **v1.5.2**, extracted at
`/data/data/com.termux/files/home/.hermes/cache/scratch/src/runc-src`.
Purpose: extract runc's **contracts and their failure modes** — not the syscall wrappers — so PDN
(a ptrace-based proot-compatible runtime for Termux/Android) can copy the contracts it can honour
and explicitly refuse the ones proot makes impossible.

Every claim below carries a `file:line` anchor or a command output. Anything I could not check is
marked **UNVERIFIED** with the check that would settle it.

---

## 1. Method and inventory

### 1.1 Extraction inventory (counts I ran)

Command:

```sh
cd /data/data/com.termux/files/home/.hermes/cache/scratch/src/runc-src
find . -type f | wc -l
find . -name '*.go' | wc -l
find . -name '*.go' -not -path './vendor/*' -exec cat {} + | wc -l
find ./vendor -name '*.go' | wc -l
ls *.go | wc -l
```

Observed output:

```
944           # total files
930           # *.go files
31969         # lines of Go outside vendor/
751           # *.go files under vendor/
28            # top-level (package main) .go files
```

### 1.2 Missing build metadata (recorded as a gap, not inferred)

```
$ cat VERSION
cat: VERSION: No such file or directory
$ ls -la go.mod go.sum
ls: cannot access 'go.mod': No such file or directory
ls: cannot access 'go.sum': No such file or directory
$ ls vendor/modules.txt
ls: cannot access 'vendor/modules.txt': No such file or directory
$ ls -a | grep git      # no .git
(no output)
```

Consequences, stated plainly:

* `main.go:26-29` embeds a `VERSION` file via `//go:embed VERSION`; that file is **absent** from the
  extraction, so I cannot read the version string out of the tree.
  **The "v1.5.2" tag is therefore the task's assertion, not something I verified from the source.**
* There is no `go.mod`/`go.sum`/`vendor/modules.txt`, so the pinned dependency versions (notably
  `github.com/opencontainers/cgroups`, whose cgroup v2 code is the subject of §4) are
  **UNVERIFIED**. To settle: check out the tag with its `.git` and read `go.mod`.
* No `.git`, so no commit hash. `main.go:38` shows `gitCommit` is injected at build time from the
  Makefile and is empty here.

### 1.3 Files read (line counts from `wc -l`)

| File | Lines | Why read |
|---|---|---|
| `main.go` | 232 | CLI root, global flags, command list |
| `create.go` | 71 | `create` state-machine entry |
| `start.go` | 54 | `start` state switch |
| `run.go` | 84 | `run` = create+start |
| `delete.go` | 89 | teardown, aborted-start handling |
| `exec.go` | 281 | `exec` (setns path) |
| `state.go` | 58 | `state` output contract |
| `list.go` | 179 | reaping containers not started by this process |
| `utils.go` | 130 | `fatalWithCode`, `checkArgs`, root/pid-file revision |
| `utils_linux.go` | 451 | `startContainer`, `runner`, `detach` rules, cgroup warning |
| `signals.go` | 151 | exit-status capture and signal forwarding |
| `notify_socket.go` | 230 | systemd readiness/sd_notify contract |
| `tty.go` | 194 | console/tty + detach contract |
| `libcontainer/container.go` | 59 | `Status`, `BaseState` (on-disk fields) |
| `libcontainer/container_linux.go` | 1191 | re-exec, state save/refresh, `hasInit`, bootstrap data |
| `libcontainer/process_linux.go` | 1209 | `initProcess`/`setnsProcess` start, cgroup join, sync loop |
| `libcontainer/init_linux.go` | 737 | `runc init` entry, `finalizeNamespace`, `signalAllProcesses` |
| `libcontainer/standard_init_linux.go` | 304 | **the setup order** |
| `libcontainer/setns_init_linux.go` | 166 | exec-in-container init |
| `libcontainer/state_linux.go` | 245 | state machine + `destroy()` |
| `libcontainer/factory_linux.go` | 219 | `Create`/`Load`/`loadState`, id validation |
| `libcontainer/rootfs_linux.go` | 1543 | mounts, `prepareRoot`, `pivotRoot`, `finalizeRootfs` |
| `libcontainer/restored_process.go` | 128 | `nonChildProcess` (foreign container handle) |
| `libcontainer/sync.go` | 203 | parent↔child sync protocol |
| `libcontainer/error.go` | 14 | sentinel errors |
| `libcontainer/nsenter/nsexec.c` | 1232 | stage-0/1/2 clone dance |
| `libcontainer/nsenter/nsenter.go` | 17 | cgo↔Go bridge |
| `libcontainer/capabilities/capabilities.go` | 150 | bounding set / capset order |
| `vendor/.../cgroups/manager/new.go` | 77 | v1-vs-v2 manager selection |
| `vendor/.../cgroups/fs2/fs2.go` | 365 | cgroup v2 `Apply`/`Set`/`Destroy` |
| `vendor/.../cgroups/fs2/create.go` | 150 | cgroup v2 directory + subtree_control |

Total read directly: ~8 200 lines across the files in the table, plus scripted greps over the
remaining tree.

### 1.4 Every command run to reach a conclusion

```sh
# inventories (§1.1, §1.2)
ls ; find . -type f | wc -l ; find . -name '*.go' | wc -l
find . -name '*.go' -not -path './vendor/*' -exec cat {} + | wc -l
find ./vendor -name '*.go' | wc -l ; ls *.go | wc -l
cat VERSION ; ls -la go.mod go.sum ; ls vendor/modules.txt ; ls -a | grep git ; head -5 go.mod

# line counts referenced throughout
cd libcontainer && wc -l container.go container_linux.go process_linux.go init_linux.go \
  standard_init_linux.go state_linux.go factory_linux.go rootfs_linux.go setns_init_linux.go \
  restored_process.go sync.go error.go capabilities/capabilities.go
wc -l ../main.go ../create.go ../start.go ../run.go ../delete.go ../exec.go ../state.go \
  ../list.go ../utils.go ../utils_linux.go ../signals.go ../tty.go ../notify_socket.go
wc -l nsenter/nsexec.c nsenter/nsenter.go
wc -l ../../vendor/github.com/opencontainers/cgroups/fs2/*.go \
  ../../vendor/github.com/opencontainers/cgroups/manager/*.go

# targeted greps used to locate anchors
ls libcontainer/ ; ls libcontainer/state ; ls libcontainer/nsenter
grep -n 'func prepareRootfs\|func finalizeRootfs\|func mountToRootfs\|func prepareRoot\|func pivotRoot\|pivot_root' rootfs_linux.go mount_linux.go
grep -n 'stage\|STAGE\|nsexec\|clone(\|CLONE_\|_LIBCONTAINER\|init' nsenter/nsexec.c
grep -n 'WriteFile\|filename :=\|"[a-z0-9]*\.\(max\|weight\|limit\|high\|low\|id\|cpus\|mems\|procs\|stat\|events\|pressure\)"' fs2/*.go
grep -n 'func (w \*caps)\|func New(\|ApplyBoundingSet\|ApplyCaps\|capset' capabilities/*.go
grep -n 'procReady\|procRun\|procHooks\|procSeccomp\|procError\|type syncT' sync.go
grep -n 'func InitSeccomp\|PR_SET_SECCOMP' seccomp/seccomp_linux.go
grep -rn 'type nonChildProcess\|func (p \*nonChildProcess)' libcontainer/*.go
grep -rn 'func ExitStatus' libcontainer/utils/
grep -n 'func \|ReadDir\|Load(' list.go
ls crates/proot-core/src crates/podnext/src          # in the PDN repo, read-only
grep -rn 'state.json\|exec.fifo\|pid-file' crates/podnext/src
```

No write occurred outside the single deliverable. No `git` state-changing command was issued; no Go
toolchain was invoked; no build was attempted.

---

## 2. Architecture map

```
main.go:87  main()                       urfave/cli app; global flags --root, --log, --debug,
                                         --systemd-cgroup, --rootless (main.go:103-132)
main.go:133 app.Commands                 checkpoint create delete events exec kill list pause ps
                                         restore resume run spec start state update features
                                         (main.go:133-151)

CLI command layer  (package main)
  create.go:63   startContainer(ctx, CT_ACT_CREATE, nil)   -> container.Start()  (no exec)
  run.go:76      startContainer(ctx, CT_ACT_RUN, nil)      -> container.Run()    (= start+exec)
  start.go:39    getContainer() -> Status() -> container.Exec()   (releases the exec FIFO)
  delete.go:74/82 killContainer()/Destroy()
  exec.go:155    execProcess() -> runner{init:false} -> setnsProcess
  utils_linux.go:367 startContainer()   builds spec, config, runner; r.run() (utils_linux.go:222)
  utils_linux.go:258 detach := r.detach || (r.action == CT_ACT_CREATE)
  utils_linux.go:277 switch r.action { CT_ACT_CREATE|CT_ACT_RESTORE|CT_ACT_RUN }

libcontainer (package libcontainer)
  factory_linux.go:35   Create(root,id,config)  -> stateDir, cgroup manager, stoppedState
  factory_linux.go:108  Load(root,id)           -> loadState() -> nonChildProcess -> loadedState
  container_linux.go:205 Start / :214 Run / :227 Exec   (mutex-guarded state transitions)
  container_linux.go:496 newParentProcess()     -> exec.Command(exePath,"init")   <-- RE-EXEC
  container_linux.go:874 refreshState()         <-- truth for a foreign container
  container_linux.go:894 hasInit()              <-- liveness / abandonment test
  container_linux.go:837 saveState()            <-- state.json written temp+rename
  state_linux.go:33     containerState iface: stopped/running/created/paused/restored/loaded
  state_linux.go:39     destroy()               <-- teardown order
  process_linux.go:779  initProcess.start()     <-- parent side of "runc init"
  process_linux.go:460  setnsProcess.start()    <-- parent side of "runc exec"
  process_linux.go:910  parseSync()             <-- the parent↔child protocol loop
  init_linux.go:106     Init()                  <-- CHILD entry point ("runc init")
  standard_init_linux.go:51 Init()              <-- the ordered setup
  rootfs_linux.go:170   prepareRootfs() / :277 finalizeRootfs()
  nsenter/nsexec.c:725  nsexec()                <-- C, runs before Go runtime: stages 0/1/2

vendored cgroup abstraction
  manager/new.go:29  NewWithPaths  -> fs2.NewManager (v2) | fs.NewManager (v1) | systemd.*
  fs2/fs2.go:65      Apply(pid)    -> CreateCgroupPath + WriteCgroupProc
  fs2/fs2.go:213     Set(resources) -> the controller write order
  fs2/fs2.go:205     Destroy()     -> cgroups.RemovePath(dirPath)  == rmdir of cgroup dir
```

---

## 3. The five contracts

### 3.1 CLI state machine and the `runc init` re-exec

**The split.** `create`, `run` and `start` are three entry points into one function,
`startContainer` (`utils_linux.go:367`), differing only by a `CtAct` value and whether they call
`Exec()`:

```go
// run.go:76-82
status, err := startContainer(context, CT_ACT_RUN, nil)
if err == nil {
    // exit with the container's exit status so any external supervisor is
    // notified of the exit with the correct exit status.
    os.Exit(status)
}
return fmt.Errorf("runc run failed: %w", err)
```

`create` is the same call with `CT_ACT_CREATE` (`create.go:63`) and the same `os.Exit(status)`
(`create.go:67`). Anchors: `CT_ACT_CREATE/CT_ACT_RUN/CT_ACT_RESTORE` at `utils_linux.go:361-365`;
`detach := r.detach || (r.action == CT_ACT_CREATE)` at `utils_linux.go:258`; the action switch at
`utils_linux.go:277-286` mapping to `container.Start` / `Restore` / `Run`.

`start` is the *only* command that releases the gate, and it refuses illegal states first
(`start.go:33-52`):

```go
// start.go:33-52
switch status {
case libcontainer.Created:
    if err := container.Exec(); err != nil { return err }
    ...
case libcontainer.Stopped:
    return errors.New("cannot start a container that has stopped")
case libcontainer.Running:
    return errors.New("cannot start an already running container")
```

**Where the process is executed twice.** The parent builds a command whose argv[1] is the literal
string `init`, pointing at a freshly cloned copy of its own binary:

```go
// container_linux.go:530-537
cmd := exec.Command(exePath, "init")
if cmd.Err != nil {
    return nil, fmt.Errorf("exec.Command: %w", cmd.Err)
}
cmd.Args[0] = os.Args[0]
cmd.Stdin = p.Stdin
```

`exePath` is either `/proc/self/exe` (already cloned, `container_linux.go:512-518`) or
`/proc/self/fd/N` of an `exeseal.CloneSelfExe(c.stateDir)` copy (`container_linux.go:521-525`).
The child is told which flavour of init it is via an environment variable
(`container_linux.go:626` → `_LIBCONTAINER_INITTYPE=standard`; `container_linux.go:655` →
`initSetns`), and the failure to be a re-exec is what re-triggers it on the child side:

```go
// init.go:10-15
func init() {
    if len(os.Args) > 1 && os.Args[1] == "init" {
        // This is the golang entry point for runc init, executed
        // before main() but after libcontainer/nsenter's nsexec().
        libcontainer.Init()
    }
}
```

So the same binary image is executed twice: once as `runc create` (bootstrap/CLI), once as
`runc init` (which will become the container's pid 1). `nsexec.c` additionally runs *before* the Go
runtime starts, in C, and further splits into three stages
(`nsexec.c:46-53`, `nsexec.c:847-869`): `STAGE_PARENT` spawns `STAGE_CHILD` via
`clone_parent(&env, STAGE_CHILD)` which is `clone(child_func, stack, CLONE_PARENT|SIGCHLD, &ca)`
(`nsexec.c:322`); the child clones again into `STAGE_INIT`, and the parent writes
`uid_map`/`gid_map` (`nsexec.c:890-913`). The comment at `nsexec.c:800-845` is the entire
justification: `unshare(CLONE_NEWUSER)` before `clone()` is unworkable, `clone()` cannot carry all
namespace flags at once, and CLONE_NEWPID only applies to children — hence the double fork.

**How the fork/exec race is avoided.** Three mechanisms, none of them timing-based:

1. *The exec FIFO is the gate.* `create` deliberately does **not** exec. The child init, after all
   setup, opens the FIFO and writes one byte, **then** execs; the parent (a later `runc start`)
   blocks on opening that FIFO:
   ```go
   // standard_init_linux.go:264-271
   fifoFile, err := pathrs.Reopen(l.fifoFile, unix.O_WRONLY|unix.O_CLOEXEC)
   if err != nil { return fmt.Errorf("reopen exec fifo: %w", err) }
   defer fifoFile.Close()
   if _, err := fifoFile.Write([]byte("0")); err != nil {
       return &os.PathError{Op: "write exec fifo", Path: fifoFile.Name(), Err: err}
   }
   ```
   The parent side waits on exactly this, with a liveness fallback:
   ```go
   // container_linux.go:233-254
   func (c *Container) exec() error {
       path := filepath.Join(c.stateDir, execFifoFilename)
       pid := c.initProcess.pid()
       blockingFifoOpenCh := awaitFifoOpen(path)
       for {
           select {
           case result := <-blockingFifoOpenCh:
               return handleFifoResult(result)
           case <-time.After(time.Millisecond * 100):
               stat, err := system.Stat(pid)
               if err != nil || stat.State == system.Zombie {
                   if err := handleFifoResult(fifoOpen(path, false)); err != nil {
                       return errors.New("container process is already dead")
   ```
   Note the failure mode named verbatim: **`"container process is already dead"`**
   (`container_linux.go:248`) and **`"cannot start an already running container"`**
   (`container_linux.go:262`, from an empty FIFO read). An empty read means someone already
   consumed the gate byte.
2. *An explicit sync-socket protocol for everything that must be ordered.* The child announces each
   milestone and blocks for an ACK (`sync.go:18-51`):
   ```
   procMountPlease --> ... <-- procMountFd
   procSeccomp    --> ...  (no return sync)
   procHooks      --> ... <-- procHooksDone
   procReady      --> ... <-- procRun
   ```
   `procReady` is sent *before* seccomp is applied, with the reason stated in-tree:
   *"This must be done before the Seccomp rules have been applied, because we need to be able to
   read and write to a socket"* (`standard_init_linux.go:177-182`).
3. *The state record is persisted inside the handshake, not after it.* In the parent's `procReady`
   handler the container is transitioned to `createdState` and `updateState` is called **before**
   `procRun` is written back (`process_linux.go:984-1016`), with the failure mode documented at
   `process_linux.go:998-1006`: if `procRun` were sent first and runc-create were killed in between,
   the init process would leak *and* no `state.json` would exist for `runc delete` to find.

### 3.2 libcontainer state: the `state.json` contract and abandoned-container detection

**The file names are constants.**

```go
// factory_linux.go:20-23
const (
    stateFilename    = "state.json"
    execFifoFilename = "exec.fifo"
)
```

The per-container directory is `<root>/<id>` where `root` defaults to `/run/runc`
(`main.go:95`) or `$XDG_RUNTIME_DIR/runc` (`main.go:98-101`), joined securely
(`factory_linux.go:48`, `SecureJoin`) and created mode `0o711` (`factory_linux.go:91`).

**What is recorded.** The on-disk `State` is `BaseState` (`container.go:42-59`) plus platform
fields (`container_linux.go:48-80`):

```go
// container.go:44-58 (field names are the JSON keys)
type BaseState struct {
    ID string `json:"id"`
    InitProcessPid int `json:"init_process_pid"`
    InitProcessStartTime uint64 `json:"init_process_start"`
    Created time.Time `json:"created"`
    Config configs.Config `json:"config"`
}
```

plus, in `container_linux.go:56-79`: `Rootless`, `CgroupPaths map[string]string` (key `""` for
cgroup v2 unified — see also `fs2/fs2.go:304-308`), `NamespacePaths map[NamespaceType]string`,
`ExternalDescriptors []string`, `IntelRdtPath`, `IntelRdtMonPath`.

**How it is written — atomically, and how failure is kept honest.**

```go
// container_linux.go:837-861
func (c *Container) saveState(s *State) (retErr error) {
    tmpFile, err := os.CreateTemp(c.stateDir, "state-")
    if err != nil { return err }
    defer func() {
        if retErr != nil {
            tmpFile.Close()
            os.Remove(tmpFile.Name())
        }
    }()
    err = utils.WriteJSON(tmpFile, s)
    if err != nil { return err }
    err = tmpFile.Close()
    if err != nil { return err }
    stateFilePath := filepath.Join(c.stateDir, stateFilename)
    return os.Rename(tmpFile.Name(), stateFilePath)
}
```

**How a runtime finds and reaps a container it did not start.** `Load` reads the file and rebuilds
a *read-only* handle; the init is a `nonChildProcess`, i.e. a handle that cannot wait on or
terminate it, only `kill` it by pid:

```go
// restored_process.go:84-107
type nonChildProcess struct {
    processPid       int
    processStartTime uint64
    fds              []string
}
func (p *nonChildProcess) start() error {
    return errors.New("restored process cannot be started")
}
func (p *nonChildProcess) pid() int { return p.processPid }
func (p *nonChildProcess) terminate() error {
    return errors.New("restored process cannot be terminated")
}
func (p *nonChildProcess) wait() (*os.ProcessState, error) {
    return nil, errors.New("restored process cannot be waited on")
}
```

`Load` (`factory_linux.go:108-148`) then calls `c.refreshState()` (`factory_linux.go:144`).

**How a dead/abandoned container is detected.** Two independent tests.

*(a) Liveness = pid identity, not pid existence.* PID reuse is defeated by comparing the recorded
start time and checking for zombie/dead:

```go
// container_linux.go:894-907
func (c *Container) hasInit() bool {
    if c.initProcess == nil { return false }
    pid := c.initProcess.pid()
    stat, err := system.Stat(pid)
    if err != nil { return false }
    if stat.StartTime != c.initProcessStartTime || stat.State == system.Zombie || stat.State == system.Dead {
        return false
    }
    return true
}
```

*(b) The FIFO's existence distinguishes `created` from `running`.*

```go
// container_linux.go:874-891
func (c *Container) refreshState() error {
    paused, err := c.isPaused()
    if err != nil { return err }
    if paused { return c.state.transition(&pausedState{c: c}) }
    if !c.hasInit() {
        return c.state.transition(&stoppedState{c: c})
    }
    // The presence of exec fifo helps to distinguish between
    // the created and the running states.
    if _, err := os.Stat(filepath.Join(c.stateDir, execFifoFilename)); err == nil {
        return c.state.transition(&createdState{c: c})
    }
    return c.state.transition(&runningState{c: c})
}
```

**Aborted start = directory but no `state.json`.** This is the one explicit branch for it, and it is
handled by `delete`, not by `Load`:

```go
// delete.go:55-65
if errors.Is(err, libcontainer.ErrNotExist) {
    // if there was an aborted start or something of the sort then the container's directory could exist but
    // libcontainer does not see it because the state.json file inside that directory was never created.
    path := filepath.Join(context.GlobalString("root"), id)
    if e := os.RemoveAll(path); e != nil {
        fmt.Fprintf(os.Stderr, "remove %s: %v\n", path, e)
    }
    if force { return nil }
}
```

`loadState` is where `ErrNotExist` originates — a missing `state.json` is the *only* condition that
maps to it (a malformed one returns the raw JSON error):

```go
// factory_linux.go:155-166
f, err := os.Open(stateFilePath)
if err != nil {
    if errors.Is(err, os.ErrNotExist) {
        return nil, ErrNotExist
    }
    return nil, err
}
defer f.Close()
var state *State
if err := json.NewDecoder(f).Decode(&state); err != nil {
    return nil, err
}
```

`runc list` is the bulk reaper: it `ReadDir`s the root, skips non-dirs (`list.go:127-129`), `Load`s
each entry and prints a per-entry error to stderr while continuing on failure (`list.go:146-150`).
`errEmptyID` guards the empty-id lookup (`utils_linux.go:27`, `:33-35`).

**Deletion semantics keyed to state** (`delete.go:76-87`): `Stopped` → `Destroy()`;
`Created` → `killContainer()` (SIGKILL to init, poll up to 100 × 100 ms for it to go, then
`Destroy()` — `delete.go:16-25`); anything else → refused with
`"cannot delete container %s that is not stopped: %s"`. With `--force`, kill-and-destroy is done
even for a stopped container, because a non-private-PID-namespace container can have leftovers in
its cgroup (`delete.go:68-75`).

### 3.3 cgroup v2 handling: file layout, order, refusals, delegation, destruction

**Manager selection.** `NewWithPaths` branches on the local hierarchy first, then on systemd mode:

```go
// manager/new.go:37-47
if cgroups.IsCgroup2UnifiedMode() {
    path, err := getUnifiedPath(paths)
    if err != nil { return nil, fmt.Errorf("manager.NewWithPaths: inconsistent paths: %w", err) }
    if config.Systemd {
        return systemd.NewUnifiedManager(config, path)
    }
    return fs2.NewManager(config, path)
}
```

`getUnifiedPath` (`manager/new.go:64-77`) enforces the single-key invariant: `len(paths) > 1` is an
error, `filepath.Clean(path) != path || !filepath.IsAbs(path)` is an error.

**Directory creation and the subtree_control handshake.** `CreateCgroupPath` walks each path
element from `/sys/fs` down, and at every intermediate level enables *all* currently-supported
controllers:

```go
// fs2/create.go:86, :107-143 (condensed to the load-bearing lines)
res := "+" + strings.Join(ctrs, " +")
...
cgType, _ := cgroups.ReadFile(current, cgTypeFile)   // "cgroup.type"
cgType = strings.TrimSpace(cgType)
switch cgType {
case "domain invalid":
    if containsDomainController(c.Resources) {
        return fmt.Errorf("cannot enter cgroupv2 %q with domain controllers -- it is in an invalid state", current)
    } else {
        _ = cgroups.WriteFile(current, cgTypeFile, "threaded")
    }
case "domain threaded":
    fallthrough
case "threaded":
    if containsDomainController(c.Resources) {
        return fmt.Errorf("cannot enter cgroupv2 %q with domain controllers -- it is in %s mode", current, cgType)
    }
}
// enable all supported controllers
if i < len(elements)-1 {
    if err := cgroups.WriteFile(current, cgStCtlFile, res); err != nil {      // "cgroup.subtree_control"
        // try write one by one
        for ctr := range strings.SplitSeq(res, " ") {
            _ = cgroups.WriteFile(current, cgStCtlFile, ctr)
        }
    }
}
```

Note the deliberate shape: the bulk `+ctr1 +ctr2 ...` write is attempted, and on failure each
controller is retried **individually and its error dropped** — the comment says the real error will
surface later in the `setXXX()` functions (`fs2/create.go:144-146`). The domain-controller
classification is `containsDomainController = isMemorySet || isIoSet || isCPUSet || isHugeTlbSet`
(`fs2/create.go:66-68`).

**The write order when resources are applied** — this is the contract worth copying:

```go
// fs2/fs2.go:213-266, error handling elided for readability
func (m *Manager) Set(r *cgroups.Resources) error {
    if r == nil { return nil }
    if err := m.getControllers(); err != nil { return err }
    if err := setPids(m.dirPath, r); err != nil { return err }        // pids.max
    if err := setMemory(m.dirPath, r); err != nil { return err }      // memory.max / .low / .swap.max
    if err := setIo(m.dirPath, r); err != nil { return err }          // io.weight / io.max
    if err := setCPU(m.dirPath, r); err != nil { return err }         // cpu.weight / cpu.max / .burst / cpu.idle
    if err := setDevices(m.dirPath, r); err != nil {                  // pseudo-controller (eBPF)
        if !m.config.Rootless || errors.Is(err, cgroups.ErrDevicesUnsupported) { return err }
    }
    if err := setCpuset(m.dirPath, r); err != nil { return err }      // cpuset.cpus / cpuset.mems
    if err := setHugeTlb(m.dirPath, r); err != nil { return err }     // hugetlb.<size>.max / .rsvd.max
    if err := fscommon.RdmaSet(m.dirPath, r); err != nil { return err }
    if err := setFreezer(m.dirPath, r.Freezer); err != nil { return err }
    if err := m.setUnified(r.Unified); err != nil { return err }      // raw key=value escape hatch
    m.config.Resources = r
    return nil
}
```

**Exact controller files written** (from the greps, all under `/sys/fs/cgroup/<path>/`):

| Order | File | Written by |
|---|---|---|
| 1 | `pids.max` | `fs2/pids.go:34` |
| 2 | `memory.max`, `memory.low`, `memory.swap.max` | `fs2/memory.go:64,72,55` |
| 3 | `io.weight`, `io.max` (`rbps`/`wbps`/`riops`/`wiops`) | `fs2/io.go:64,77,82,87,92` |
| 4 | `cpu.weight`, `cpu.max`, `cpu.max.burst`, `cpu.idle` | `fs2/cpu.go:32,66,40/70,25` |
| 5 | (devices — eBPF, no file) | `fs2/fs2.go:269-277` |
| 6 | `cpuset.cpus`, `cpuset.mems` | `fs2/cpuset.go:17,22` |
| 7 | `hugetlb.<size>.max`, `hugetlb.<size>.rsvd.max` | `fs2/hugetlb.go:25,31` |
| 8 | (rdma) | `fscommon.RdmaSet` |
| 9 | `cgroup.freeze` (freezer) | `fs2/freezer.go` |
| 10 | arbitrary `CONTROLLER.PARAMETER` | `fs2/fs2.go:279-302` |
| — | `cgroup.procs` (membership) | `fs2/fs2.go:81` `WriteCgroupProc` |
| — | `cgroup.events` (poll) | `fs2/freezer.go:104` |
| — | `cgroup.kill` (fast kill) | `init_linux.go:691` |

**What is refused when a controller is unavailable.** Two distinct refusals:

```go
// fs2/fs2.go:284-296
if err := cgroups.WriteFileByLine(m.dirPath, k, v); err != nil {
    if errors.Is(err, os.ErrPermission) || errors.Is(err, os.ErrNotExist) {
        c, _, ok := strings.Cut(k, ".")
        if !ok {
            return fmt.Errorf("unified resource %q must be in the form CONTROLLER.PARAMETER", k)
        }
        if _, ok := m.controllers[c]; !ok && c != "cgroup" {
            return fmt.Errorf("unified resource %q can't be set: controller %q not available", k, c)
        }
    }
    return fmt.Errorf("unable to set unified resource %q: %w", k, err)
}
```

and for devices, a sentinel instead of a silent no-op:

```go
// fs2/fs2.go:269-275
func setDevices(dirPath string, r *cgroups.Resources) error {
    if cgroups.DevicesSetV2 == nil {
        if len(r.Devices) > 0 {
            return cgroups.ErrDevicesUnsupported
        }
        return nil
    }
    return cgroups.DevicesSetV2(dirPath, r)
}
```

The `m.controllers` set comes from reading `cgroup.controllers` once
(`fs2/fs2.go:44-63`); a read failure is tolerated **only** when rootless with no explicit path:

```go
// fs2/fs2.go:49-55
data, err := cgroups.ReadFile(m.dirPath, "cgroup.controllers")
if err != nil {
    if m.config.Rootless && m.config.Path == "" {
        return nil
    }
    return err
}
```

**Delegation.** Delegation is not detected by a probe; it is inferred from whether the writes
succeed, and the error is converted into one of two outcomes. In `Apply`:

```go
// fs2/fs2.go:65-85
func (m *Manager) Apply(pid int) error {
    if err := CreateCgroupPath(m.dirPath, m.config); err != nil {
        if m.config.Rootless {
            if m.config.Path == "" {
                if blNeed, nErr := needAnyControllers(m.config.Resources); nErr == nil && !blNeed {
                    return cgroups.ErrRootless
                }
                return fmt.Errorf("rootless needs no limits + no cgrouppath when no permission is granted for cgroups: %w", err)
            }
        }
        return err
    }
    if err := cgroups.WriteCgroupProc(m.dirPath, pid); err != nil {
        return err
    }
    return nil
}
```

`needAnyControllers` (`fs2/create.go:20-61`) compares the requested resources against the
*root's* `cgroup.controllers` and returns false if nothing requested is actually available — so
"rootless + no limits" is a no-op rather than an error, while "rootless + limits" is refused with
an explanatory message. The caller then distinguishes the two:

```go
// process_linux.go:827-839
if err := p.manager.Apply(p.pid()); err != nil {
    if errors.Is(err, cgroups.ErrRootless) {
        if !p.config.Config.Namespaces.IsPrivate(configs.NEWPID) {
            // TODO: make this an error in runc 1.3.
            logrus.Warn("Creating a rootless container with no cgroup and no private pid namespace. ...")
        }
    } else {
        return fmt.Errorf("unable to apply cgroup configuration: %w", err)
    }
}
```

A separate rootless warning exists for the permission case (`utils_linux.go:447-450`):
*"runc <op> failure might be caused by lack of full access to cgroups"*, keyed on
`errors.Is(err, fs.ErrPermission)`.

**Destruction order** — and it is *not* just `rmdir`:

```go
// state_linux.go:39-67
func destroy(c *Container) error {
    // As the container without init process running is considered stopped,
    // and destroy is supposed to remove all the container resources, we need
    // to kill those processes here.
    if !c.config.Namespaces.IsPrivate(configs.NEWPID) {
        // Likely to fail when c.config.RootlessCgroups is true
        _ = signalAllProcesses(c.cgroupManager, unix.SIGKILL)
    }
    if err := c.cgroupManager.Destroy(); err != nil {
        return fmt.Errorf("unable to remove container's cgroup: %w", err)
    }
    ...
    if err := os.RemoveAll(c.stateDir); err != nil {
        return fmt.Errorf("unable to remove container state dir: %w", err)
    }
    c.initProcess = nil
    err := runPoststopHooks(c)
    c.state = &stoppedState{c: c}
    return err
}
```

`cgroupManager.Destroy()` for v2 is literally `cgroups.RemovePath(m.dirPath)` (`fs2/fs2.go:205-207`).
`signalAllProcesses` prefers the single-write kernel kill and only falls back to freeze+enumerate:

```go
// init_linux.go:684-697 (condensed)
if !m.Exists() { return ErrCgroupNotExist }
if s == unix.SIGKILL {
    if p := m.Path(""); p != "" { // Either cgroup v2 or hybrid.
        err := cgroups.WriteFile(p, "cgroup.kill", "1")
        if err == nil || !errors.Is(err, os.ErrNotExist) {
            return err
        }
        // Fallback to old implementation.
    }
}
if err := m.Freeze(cgroups.Frozen); err != nil { logrus.Warn(err) }
```

The fallback is freeze → `GetAllPids` → `unix.Kill(pid, s)` each → thaw
(`init_linux.go:699-717`).

### 3.4 Namespace, mount, seccomp, capability, `no_new_privs` setup order

**Namespaces** are created in `nsexec.c` from the CLONE flags the parent encoded into a netlink
bootstrap message (`container_linux.go:1043-1164`, `CloneFlagsAttr` at `container_linux.go:1061`),
using this table (`nsexec.c:470-477`):

```c
{ CLONE_NEWCGROUP, "cgroup" },
{ CLONE_NEWIPC, "ipc" },
{ CLONE_NEWNS, "mnt" },
{ CLONE_NEWNET, "net" },
{ CLONE_NEWPID, "pid" },
{ CLONE_NEWTIME, "time" },
{ CLONE_NEWUSER, "user" },
{ CLONE_NEWUTS, "uts" },
```

Custom namespace paths (join an existing namespace) are ordered deterministically
(`container_linux.go:990-1019` `orderNamespacePaths`, driven by `configs.NamespaceTypes()`), and
commas in a path are rejected because the list is comma-joined
(`container_linux.go:1074`, `:1008-1012`).

**The canonical setup order lives in one function**, `linuxStandardInit.Init()`
(`standard_init_linux.go:51-304`). Verbatim, the decisive stretch:

```go
// standard_init_linux.go:148-203
pdeath, err := system.GetParentDeathSignal()
if err != nil { return fmt.Errorf("can't get pdeath signal: %w", err) }
if l.config.NoNewPrivileges {
    if err := unix.Prctl(unix.PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0); err != nil {
        return &os.SyscallError{Syscall: "prctl(SET_NO_NEW_PRIVS)", Err: err}
    }
}
if err := setupScheduler(l.config); err != nil { return err }
if err := setupIOPriority(l.config); err != nil { return err }
if l.config.Config.Personality != nil {
    if err := setupPersonality(l.config.Config); err != nil { return err }
}
if err := setupMemoryPolicy(l.config.Config); err != nil { return err }
// Tell our parent that we're ready to exec. This must be done before the
// Seccomp rules have been applied, because we need to be able to read and
// write to a socket.
if err := syncParentReady(l.pipe); err != nil { return fmt.Errorf("sync ready: %w", err) }
if l.config.ProcessLabel != "" {
    if err := selinux.SetExecLabel(l.config.ProcessLabel); err != nil { ... }
}
// Without NoNewPrivileges seccomp is a privileged operation, so we need to
// do this before dropping capabilities; otherwise do it as late as possible
// just before execve so as few syscalls take place after it as possible.
if l.config.Config.Seccomp != nil && !l.config.NoNewPrivileges {
    seccompFd, err := seccomp.InitSeccomp(l.config.Config.Seccomp)
    if err != nil { return err }
    if err := syncParentSeccomp(l.pipe, seccompFd); err != nil { return err }
}
if err := finalizeNamespace(l.config); err != nil { return err }
```

Read as an ordered contract, the whole function is:

1. session keyring, unless `NoNewKeyring` (`:52-79`);
2. network interfaces and routes (`:81-86`);
3. `selinux.GetEnabled()` init (`:89`);
4. `prepareRootfs` — `prepareRoot`, all mounts, `/dev`, **then** `syncParentHooks` → parent runs
   pre-start hooks, **then** the `CreateContainer` hook, **then** `pivot_root`/`chroot`/`msMoveRoot`,
   **then** root mount propagation, **then** `reOpenDevNull`, **then** `MkdirAll(Cwd)`
   (`:91-94`; `rootfs_linux.go:170-273`);
5. console setup + `setctty` (`:99-106`) — comment: *"This has to be done before we finalize the
   rootfs, but after we've given the user the chance to set up all of the mounts"*;
6. pidfd handover (`:108-112`);
7. `finalizeRootfs` only if `NEWNS` (`:115-119`) — makes tmpfs/`/dev` and optionally `/` read-only
   (`rootfs_linux.go:277-299`);
8. `sethostname`, `setdomainname` (`:121-130`);
9. AppArmor profile (`:131-133`);
10. sysctls, read-only paths, masked paths (`:135-146`);
11. **`PR_SET_NO_NEW_PRIVS`** if configured (`:152-156`);
12. scheduler, IO priority, personality, memory policy (`:158-175`);
13. **`syncParentReady` → `procReady`** (`:180`);
14. selinux exec label (`:183-187`);
15. **seccomp** — two placements depending on `NoNewPrivileges` (`:191-200`, `:239-248`);
16. **`finalizeNamespace`** (`:201-203`);
17. restore pdeath signal, because `finalizeNamespace` changes uid/gid and clears it (`:204-208`);
18. parent-pid unchanged check, `LookPath` of argv[0] (`:224-232`);
19. close the sync pipe, close the log pipe, **write the exec FIFO** (`:250-271`);
20. `StartContainer` hooks (`:282-288`);
21. `utils.UnsafeCloseFrom(PassedFilesCount + 3)` then `linux.Exec` (`:300-303`).

`finalizeNamespace` is the ordering that decides privilege (init_linux.go:300-368):

```go
// init_linux.go:336-366 (condensed)
w, err := capabilities.New(config.Capabilities)
if err != nil { return err }
// drop capabilities in bounding set before changing user
if err := w.ApplyBoundingSet(); err != nil { return fmt.Errorf("unable to apply bounding set: %w", err) }
// preserve existing capabilities while we change users
if err := system.SetKeepCaps(); err != nil { ... }
if err := setupUser(config); err != nil { ... }
if doChdir { if err := unix.Chdir(config.Cwd); err != nil { ... } }
if err := verifyCwd(); err != nil { return err }
if err := system.ClearKeepCaps(); err != nil { ... }
if err := w.ApplyCaps(); err != nil { ... }
```

Semantics: bounding set **before** uid change; `KEEP_CAPS` held across the uid change; cwd re-tried
after the uid change if the first attempt hit `EPERM` (`init_linux.go:309-326`); `verifyCwd` rejects
a cwd outside the mount namespace root — the CVE-2024-21626 check
(`init_linux.go:271-295`, message *"current working directory is outside of container mount
namespace root -- possible container breakout detected"*); full capset applied only after
`ClearKeepCaps`. `ApplyBoundingSet`/`ApplyCaps` are `Clear`+`Set`+`Apply` on the bounding set and
on `CAPS|BOUNDS` respectively (`capabilities/capabilities.go:100-107`, `:110-150`), with ambient
caps set one-by-one and failures merely warned (`capabilities/capabilities.go:142-147`).

**Mount order** (rootfs_linux.go:170-273), the parts that matter:

```go
// rootfs_linux.go:234-256
if config.NoPivotRoot {
    err = msMoveRoot(config.Rootfs)
} else if config.Namespaces.Contains(configs.NEWNS) {
    err = pivotRoot(rootFd)
} else {
    err = chroot()
}
if err != nil { return fmt.Errorf("error jailing process inside rootfs: %w", err) }
rootFd.Close()
// Apply root mount propagation flags.
// This must be done after pivot_root/chroot because the mount propagation flag is applied
// to the current root ("/"), and not to the old rootfs before it becomes "/".
if config.RootPropagation != 0 && config.RootPropagation&(unix.MS_PRIVATE|unix.MS_SLAVE) == 0 {
    if err := mount("", "/", "", uintptr(config.RootPropagation), ""); err != nil { ... }
}
```

`prepareRoot` runs first and does three things in order (`rootfs_linux.go:1106-1120`): `mount("", "/",
"", MS_SLAVE|MS_REC)` on the current root, then `rootfsParentMountPropagation(Rootfs, ...)`, then
`mount(Rootfs, Rootfs, "bind", MS_BIND|MS_REC, "")`. `pivotRoot` is the `pivot_root(".", ".")` trick
with oldroot pinned by an `O_PATH` fd, made `MS_SLAVE|MS_REC`, then `MNT_DETACH`-unmounted
(`rootfs_linux.go:1146-1195`).

### 3.5 Error reporting and exit-status propagation

**Child→parent error channel.** The child's `startInitialization` installs a defer that fires only
on failure, and — critically — sets `retErr = nil` after hand-off so the error is not reported twice:

```go
// init_linux.go:138-152
defer func() {
    // If this defer is ever called, this means initialization has failed.
    if syncPipe.isClosed() { return }
    ierr := initError{Message: retErr.Error()}
    if err := writeSyncArg(syncPipe, procError, ierr); err != nil {
        fmt.Fprintln(os.Stderr, err)
        return
    }
    // The error is sent, no need to also return it (or it will be reported twice).
    retErr = nil
}()
```

`initError` exists because `encoding/json` cannot unmarshal into an `error` (`sync.go:80-88`), and
the parent converts it back on read (`sync.go:140-149`). If the error *cannot* be sent, the child
writes it to stderr as last resort and exits **255**:

```go
// init_linux.go:106-123
func Init() {
    runtime.GOMAXPROCS(1)
    runtime.LockOSThread()
    if err := startInitialization(); err != nil {
        fmt.Fprintln(os.Stderr, err)
    }
    // Normally, StartInitialization() never returns, meaning
    // if we are here, it had failed.
    os.Exit(255)
}
```

The parent loop is `parseSync` (`process_linux.go:910-1052`) and it hard-fails on a missing
milestone rather than proceeding:

```go
// process_linux.go:1057-1062
if !seenProcReady && ierr == nil {
    ierr = errors.New("procReady not received")
}
if ierr != nil {
    return fmt.Errorf("error during container init: %w", ierr)
}
```

**OOM is named, not inferred from the error text.** The parent counts OOM kills *before* tearing
down, so the cgroup still exists:

```go
// process_linux.go:790-810 (condensed)
oom, err := p.manager.OOMKillCount()
if err != nil {
    logrus.WithError(err).Warn("unable to get oom kill count")
} else if oom > 0 {
    const oomError = "container init was OOM-killed (memory limit too low?)"
    if logrus.GetLevel() >= logrus.DebugLevel {
        retErr = fmt.Errorf(oomError+": %w", retErr)
    } else {
        retErr = errors.New(oomError)
    }
}
```

`OOMKillCount` reads `memory.events` key `oom_kill` (`fs2/fs2.go:322-333`). For `runc exec` the
same idea is applied as a *diff* of the counter around the start (`process_linux.go:463-475`), with
the annotation `"(possibly OOM-killed)"`.

**Init-side errors are also routed through the log pipe**, so a child failure surfaces even when the
sync channel is quiet:

```go
// container_linux.go:351-366
if logsDone := parent.forwardChildLogs(); logsDone != nil {
    defer func() {
        err := <-logsDone
        if err != nil {
            // runc init errors are important; make sure retErr has them.
            err = fmt.Errorf("runc init error(s): %w", err)
            if retErr != nil {
                retErr = fmt.Errorf("%w; %w", retErr, err)
            } else {
                retErr = err
            }
        }
    }()
}
```

**Exit status contract — three rules.**

*(1) Signal deaths are reported as 128+signum.*

```go
// libcontainer/utils/utils.go:12-23
const (
    exitSignalOffset = 128
)
func ExitStatus(status unix.WaitStatus) int {
    if status.Signaled() {
        return exitSignalOffset + int(status.Signal())
    }
    return status.ExitStatus()
}
```

*(2) The foreground CLI returns the container's status as its own.*

```go
// signals.go:99-111 (condensed)
for _, e := range exits {
    logrus.WithFields(...).Debug("process exited")
    if e.pid == pid1 {
        // call Wait() on the process even though we already have the exit
        // status because we must ensure that any of the go specific process
        // fun such as flushing pipes are complete before we return.
        _, _ = process.Wait()
        return e.status, nil
    }
}
```

`e.status` was produced by `utils.ExitStatus(ws)` in `reap()` (`signals.go:146-149`). The runner
then returns it up (`utils_linux.go:301-313`) and the command calls `os.Exit(status)`
(`create.go:67`, `run.go:80`, `exec.go:112`).

*(3) Runtime failure is 1, or 255 for `exec`.* `fatalWithCode(err, 1)` for general CLI errors
(`utils.go:56-68`, reached from `main.go:177-179`); `exec` uses 255 explicitly:

```go
// exec.go:110-115
status, err := execProcess(context)
if err == nil {
    os.Exit(status)
}
fatalWithCode(fmt.Errorf("exec failed: %w", err), 255)
```

Signals are forwarded by default and never eaten (`signals.go:117-122`); `SIGWINCH` becomes a tty
resize and `SIGURG` is deliberately swallowed as a Go-runtime artefact (`signals.go:89-116`).

---

## 4. What PDN can take, and what proot makes impossible

PDN's runtime is `crates/proot-core` (`ptrace.rs`, `syscall.rs`, `tracee.rs`, `tracee_state.rs`,
`vfs.rs`, `seccomp.rs`, `path.rs`) driven from `crates/podnext` (`cli.rs`, `daemon.rs`, `direct.rs`,
`dispatch.rs`, `doctor.rs`, `native.rs`). There is no state file name, no exec FIFO and no
`pid-file` usage anywhere in `crates/podnext/src` today (grep for `state.json|exec.fifo|pid-file`
returned zero hits). That is the gap the runc contracts fill.

### 4.1 Takeable as-is (no kernel privileges required)

**T1 — The three-command split as an explicit, enforced state machine.**
`create` must not exec; `start` must be the only thing that releases the gate; `run` is
`create`+`start` in one process. The point is not the command names but the *transition matrix*
(`state_linux.go:84-244`): `stopped → running|restored`, `running → stopped` only if
`!hasInit()` (`state_linux.go:117-131`, returning `ErrRunning` otherwise), `created →
running|paused|stopped`, `paused → running|stopped`. PDN should copy the matrix and the refusal
strings, because they are the contract every OCI client checks.

**T2 — The FIFO gate instead of a sleep or a poll.** runc does not synchronise create→start with a
timeout; it synchronises with a byte. PDN can do exactly this under proot: create the gate as a
named pipe *or* a socket in the container's state directory (proot does not forbid FIFOs), have the
trampoline block on opening it, and have `pdn start` be the opener. The liveness fallback runc uses
— "if the pid is gone or a zombie, do a non-blocking open and report `already dead`"
(`container_linux.go:242-250`) — is directly portable and is the difference between a hang and a
diagnosis.

**T3 — The exact `state.json` contract.** Fields, names and types are fully takeable
(`container.go:44-58` + `container_linux.go:56-79`): `id`, `init_process_pid`,
`init_process_start`, `created`, `config`, `cgroup_paths`, `namespace_paths`,
`external_descriptors`. Two rules must come with it:
* write it **temp + `os.Rename`** (`container_linux.go:837-860`), never in place;
* write it **before** releasing the gate (`process_linux.go:1007-1016`), because that is what makes
  a crashed launcher recoverable.

**T4 — Abandonment detection by (pid, start-time) tuple, not by pid.** `hasInit()`
(`container_linux.go:894-907`) is the single most transferable idea in the file. PDN under proot has
no PID namespace, so pids are host pids and are recycled aggressively on Android; comparing
`/proc/<pid>/stat` field 22 (`system.Stat().StartTime`) plus a zombie/dead check is the only cheap
way to answer "is this still my container". The failure mode this prevents is stated in-tree at
`container_linux.go:429-431`: *"To avoid a PID reuse attack, don't kill non-running container."*

**T5 — The "directory but no state file" recovery path.** `delete.go:55-65` is a complete
specification for a partially-created container: treat `ErrNotExist` from the state load as
*aborted start*, `RemoveAll` the state dir, and succeed when `--force`. Mapping only
`os.ErrNotExist` → `ErrNotExist` and letting a malformed file propagate as a parse error
(`factory_linux.go:155-166`) is likewise the right failure split.

**T6 — The exit-status triple.** 128+signum for signals (`utils.go:12-23`); the container's status
returned as the foreground command's own status; 255 for "runtime could not even start it". This is
cheap, portable, and is what `docker`/`podman` actually read.

**T7 — The ordered `no_new_privs` → seccomp → capability-drop sequence, as a *documented order*.**
Even where PDN cannot enforce each step, it should execute them in runc's order and *record* which
steps were skipped, because the order is what makes the failure modes comprehensible
(`standard_init_linux.go:148-203`; `init_linux.go:336-366`).

**T8 — The sync-milestone naming.** `procMountPlease`, `procHooks`/`procHooksDone`,
`procReady`/`procRun`, `procSeccomp`/`procSeccompDone`, `procError` (`sync.go:41-51`) is a clean
vocabulary for any two-process handshake, and `initError{Message}` (`sync.go:80-88`) is the correct
answer to JSON's inability to marshal an error type.

**T9 — The interface shape for a foreign container handle.** `nonChildProcess`
(`restored_process.go:84-128`) is the pattern PDN needs for `pdn` talking to a container a *daemon*
started: `pid()` and `signal()` work; `wait()`, `start()` and `terminate()` must return distinct,
explicit errors rather than silently no-op.

### 4.2 Impossible under proot — and the reason

These are not "hard"; they are precluded by the mechanism. proot implements the container by
`ptrace`-intercepting syscalls of an otherwise ordinary, unprivileged process and rewriting paths;
it does not create kernel objects. Anything that requires a kernel namespace or a capability in a
user-namespace is therefore unavailable, on Android in particular, where the app sandbox provides
neither.

**I1 — The stage-0/1/2 namespace creation is impossible.** `nsexec.c:847-869` exists solely to
`clone(CLONE_NEWUSER|CLONE_NEWNS|CLONE_NEWPID|...)`. On Android, an app process holds no
`CAP_SYS_ADMIN` and cannot unshare these. Consequence: PDN has **no** pid-1-in-a-new-pidns. The
whole "the trampoline becomes the container's pid 1" property is unavailable, so
`hasInit()` cannot be transplanted literally — the init is a traced child, and the pid recorded in
`state.json` must be that child's.

**I2 — `pivot_root` / `chroot` as a jail is impossible.** `rootfs_linux.go:1146-1195` requires
`CAP_SYS_ADMIN` and a private mount namespace; the comment at `rootfs_linux.go:1076-1080` already
says outright that `pivot_root()` fails if the parent mount is shared. proot's substitute is path
translation in the tracee (`proot-core/src/path.rs`, `vfs.rs`), plus `fake_id0` for the uid/gid
illusion. PDN must therefore **never** claim `rootfs` isolation; the honest statement is "path
translation, escape by absolute-path construction is prevented at the translation layer, not by the
kernel".

**I3 — Mount namespace semantics are impossible.** `prepareRootfs` (`rootfs_linux.go:170-273`) —
`MS_SLAVE|MS_REC` on `/`, bind of the rootfs, per-mount propagation, `MNT_DETACH` teardown —
requires `CLONE_NEWNS` plus `CAP_SYS_ADMIN` in it. proot's answer is to *lie*: it fakes
`/proc/self/mountinfo` (`proot`'s `mountinfo` extension; documented in this repo's
`research/PROOT_AND_PROOTDISTRO_SOURCE_DISSECTION.md:129`). PDN cannot take the mount order; it can
take only the *ordering discipline* and must document that the mount view is synthetic.

**I4 — seccomp cannot be installed the way runc installs it.** `seccomp.InitSeccomp`
(`libcontainer/seccomp/seccomp_linux.go:32`) is called from inside the container init *after* the
privileged operations, and its placement is conditioned on `NoNewPrivileges` precisely because
without it the operation is privileged (`standard_init_linux.go:188-200`). Under proot, PDN already
*runs a ptracer*; installing a seccomp filter on the tracee changes syscall semantics underneath
the ptrace loop and, on Android, the relevant filters typically require privileges the app lacks.
PDN's `proot-core/src/seccomp.rs` should therefore be read as an emulation allow-list, not as runc's
`SCMP_ACT_NOTIFY` machinery.

**I5 — Real capability dropping is impossible.** `ApplyBoundingSet` uses `prctl(PR_CAPBSET_DROP)`
via the capability library and `ApplyCaps` uses `capset(2)`
(`capabilities/capabilities.go:100-125`). Neither has any meaning relative to proot's emulated
root, because the process is not actually root: proot's `fake_id0` (this repo:
`PROOT_AND_PROOTDISTRO_SOURCE_DISSECTION.md:125`) reports uid 0 to the tracee while the kernel sees
the app uid. PDN can and should *report* the requested capability set, and must state that it is
not enforced in the kernel.

**I6 — `PR_SET_NO_NEW_PRIVS` does not create a boundary here.** runc sets it at
`standard_init_linux.go:152-156` as a hardening step on top of a real setuid transition. With
`fake_id0`, there is no setuid transition to harden; setting the flag is cosmetic unless PDN also
controls every execve — which it does not outside its traced tree.

**I7 — cgroup v2 `Apply`/`Set` is impossible on an unrooted Android/Termux app.** Every write in
`fs2/fs2.go:213-267` targets a path under `/sys/fs/cgroup`, and `CreateCgroupPath`
(`fs2/create.go:71-147`) needs to `mkdir` and write `cgroup.subtree_control` in the delegated
subtree. Without delegation, `fs2.NewManager` will read `cgroup.controllers` and either fail
(`fs2/fs2.go:49-55`) or produce the specific refusals quoted in §3.3. **The takeable part is the
knowledge, not the mechanism**: PDN can record the *intended* resource limits in its state file
using runc's key names, and — crucially — should copy runc's refusal semantics: a requested limit
that cannot be applied must be an error, not a warning, except in the explicitly rootless case
(`fs2/fs2.go:71-78`, `fs2/fs2.go:241-245`). `cgroup.kill` (`init_linux.go:691`) has no analogue;
PDN's teardown must fall back to enumerating and killing traced pids.

**I8 — The `cgroup.kill`-based fast teardown and the "kill everything in the cgroup" guarantee are
impossible.** `destroy()` (`state_linux.go:48-51`) and `Signal()` (`container_linux.go:407-422`)
depend on cgroup membership to catch orphaned processes. Under proot, an orphaned descendant that
escapes the ptrace tree is simply gone. This is exactly the condition runc warns about at
`process_linux.go:833-835` for the rootless/no-cgroup case: *"Such configuration is strongly
discouraged (as it is impossible to properly kill all container's processes)"*. PDN is permanently
in that configuration and should say so where runc warns.

### 4.3 One-line summary of the trade

Take: the **state machine, the state file + FIFO gate, the (pid, start-time) liveness test, the
aborted-start recovery, the exit-status triple, and the naming of every refusal**.
Refuse, explicitly and in the docs: **namespaces, `pivot_root`/mount isolation, real seccomp, real
capabilities, `no_new_privs`, and cgroup v2 resource control** — because proot emulates these with
path rewriting and `ptrace`, and Android's app sandbox withholds the kernel objects they need.

---

## 5. Claim table

| # | Claim | Support |
|---|---|---|
| 1 | Extraction has 944 files, 930 `*.go`, 31 969 non-vendor Go lines, 751 vendor `*.go` | `find`/`wc` output, §1.1 |
| 2 | No `VERSION`, `go.mod`, `go.sum`, `vendor/modules.txt` or `.git` in the tree | `cat VERSION`; `ls -la go.mod go.sum`; `ls vendor/modules.txt`; `ls -a \| grep git` |
| 3 | `main.go` embeds `VERSION`; `gitCommit` is build-injected | `main.go:26-29`, `main.go:36-38` |
| 4 | Default state root `/run/runc`, or `$XDG_RUNTIME_DIR/runc` | `main.go:95`, `main.go:98-101` |
| 5 | `create`/`run` are `startContainer` with different `CtAct`; both `os.Exit(status)` | `create.go:63,67`; `run.go:76,80`; `utils_linux.go:359-365` |
| 6 | `create` implies detach | `utils_linux.go:258` |
| 7 | `start` requires `Created`, else refuses with named errors | `start.go:33-52` |
| 8 | `delete` maps `Stopped→Destroy`, `Created→killContainer`, else refuses | `delete.go:76-87`; `delete.go:16-25` |
| 9 | `delete` treats "dir exists, no `state.json`" as aborted start → `RemoveAll` | `delete.go:55-65` |
| 10 | Re-exec: parent execs its own (cloned) binary with argv[1] == `init` | `container_linux.go:530-537`; `container_linux.go:512-528` |
| 11 | `init` subcommand handled in Go `init()` before `main()` | `init.go:10-15` |
| 12 | Child learns its role from `_LIBCONTAINER_INITTYPE` (`standard`/`setns`) | `container_linux.go:626`, `:655`; `init_linux.go:35-38` |
| 13 | nsexec runs before the Go runtime and splits into stages 0/1/2 | `nsexec.c:46-53`, `:847-869`, `:725-751` |
| 14 | Stage-1 spawn is `clone(..., CLONE_PARENT\|SIGCHLD, ...)` | `nsexec.c:322`, `:867` |
| 15 | Reason for the double fork documented in-tree (userns/SELinux/CLONE_NEWPID) | `nsexec.c:800-845` |
| 16 | Exec gate: child writes one byte to the FIFO then execs | `standard_init_linux.go:264-271` |
| 17 | Parent waits on the FIFO; empty read ⇒ "already running"; zombie ⇒ "already dead" | `container_linux.go:233-254`, `:256-265` |
| 18 | Sync protocol names and pairing | `sync.go:18-51` |
| 19 | `procReady` sent before seccomp, by design | `standard_init_linux.go:177-182`; `init_linux.go:422-429` |
| 20 | State persisted inside the `procReady` handler before `procRun` | `process_linux.go:984-1016`; rationale `process_linux.go:998-1006` |
| 21 | State file names | `factory_linux.go:20-23` |
| 22 | state dir mode `0o711`, created after cgroup existence checks | `factory_linux.go:69-93` |
| 23 | `State`/`BaseState` fields and JSON keys | `container.go:42-59`; `container_linux.go:48-80` |
| 24 | State written temp + `Rename` | `container_linux.go:837-861` |
| 25 | Missing `state.json` is the only source of `ErrNotExist` | `factory_linux.go:150-172` |
| 26 | `Load` reinflates a read-only handle via `nonChildProcess` and refreshes state | `factory_linux.go:108-148`; `restored_process.go:84-128` |
| 27 | `nonChildProcess` refuses start/wait/terminate with explicit errors | `restored_process.go:90-104` |
| 28 | `refreshState`: paused → stopped → FIFO-present ⇒ created → running | `container_linux.go:874-891` |
| 29 | `hasInit` compares start time and rejects zombie/dead | `container_linux.go:894-907` |
| 30 | PID-reuse guard before signalling | `container_linux.go:428-432` |
| 31 | `list` reaps foreign containers, one `Load` per directory, errors to stderr | `list.go:113-155` |
| 32 | cgroup v2 manager chosen when unified mode | `manager/new.go:37-47` |
| 33 | Unified path invariant: single `""` key, absolute, clean | `manager/new.go:64-77`; `fs2/fs2.go:304-308` |
| 34 | `Apply` = `CreateCgroupPath` + `WriteCgroupProc` | `fs2/fs2.go:65-85` |
| 35 | Delegation inferred from write success; `ErrRootless` only when rootless+no path+no controllers needed | `fs2/fs2.go:71-79`; `fs2/create.go:20-61` |
| 36 | Rootless+limits ⇒ explanatory error; caller distinguishes `ErrRootless` | `fs2/fs2.go:76`; `process_linux.go:827-839` |
| 37 | `subtree_control` written as `+ctr1 +ctr2 ...`, then individually, errors dropped | `fs2/create.go:86`, `:136-146` |
| 38 | `domain invalid`/`threaded` refusals for domain controllers | `fs2/create.go:107-134`; `:66-68` |
| 39 | Controller write order: pids, memory, io, cpu, devices, cpuset, hugetlb, rdma, freezer, unified | `fs2/fs2.go:213-266` |
| 40 | Exact file names per controller | `fs2/pids.go:34`; `fs2/memory.go:55,64,72`; `fs2/io.go:64,77-92`; `fs2/cpu.go:25,32,40,66,70`; `fs2/cpuset.go:17,22`; `fs2/hugetlb.go:25,31` |
| 41 | Unavailable-controller refusal names the controller | `fs2/fs2.go:284-298` |
| 42 | Devices unsupported ⇒ `ErrDevicesUnsupported`, not silent success | `fs2/fs2.go:269-277` |
| 43 | `Destroy()` for v2 = remove the cgroup path | `fs2/fs2.go:205-207` |
| 44 | `destroy()` order: kill-all (if no private pidns) → cgroup destroy → intelrdt → `RemoveAll(stateDir)` → poststop hooks | `state_linux.go:39-67` |
| 45 | `cgroup.kill` preferred, freeze+enumerate fallback | `init_linux.go:684-717` |
| 46 | Namespace flags table and custom-path ordering (comma-safe) | `nsexec.c:470-477`; `container_linux.go:990-1019` |
| 47 | `no_new_privs` set before scheduler/personality/mempolicy and before `procReady` | `standard_init_linux.go:148-182` |
| 48 | Seccomp placed before cap-drop when privileged, as late as possible under NNP | `standard_init_linux.go:188-200`, `:234-248` |
| 49 | Capability order: bounding set → KEEP_CAPS → uid/gid → chdir → verifyCwd → clear KEEP_CAPS → full capset | `init_linux.go:336-366` |
| 50 | `verifyCwd` is the CVE-2024-21626 cwd-outside-rootfs check | `init_linux.go:271-295`, `:357-360` |
| 51 | Mount jail order: `prepareRoot` → mounts → `/dev` → prestart hooks → `CreateContainer` hook → pivot/chroot → root propagation → `reOpenDevNull` → mkdir cwd | `rootfs_linux.go:170-273` |
| 52 | `pivotRoot` = `pivot_root(".", ".")`, oldroot `MS_SLAVE\|MS_REC`, `MNT_DETACH` | `rootfs_linux.go:1146-1195` |
| 53 | `finalizeRootfs` makes tmpfs/`/dev` (and optionally `/`) read-only | `rootfs_linux.go:277-299` |
| 54 | Last-ditch close-from before exec, CVE-2024-21626 rationale | `standard_init_linux.go:290-303` |
| 55 | Child error channel is `procError` + `initError`; `retErr=nil` avoids double-reporting | `init_linux.go:138-152`; `sync.go:80-88`, `:140-149` |
| 56 | Failed init exits 255 | `init_linux.go:106-123` |
| 57 | Parent fails hard on a missing `procReady` | `process_linux.go:1057-1062` |
| 58 | OOM reported by name via `memory.events:oom_kill`, counted before kill | `process_linux.go:790-810`; `fs2/fs2.go:322-333` |
| 59 | `exec` OOM annotation is a counter diff | `process_linux.go:463-475` |
| 60 | Init errors also arrive via the log pipe as `runc init error(s)` | `container_linux.go:351-366` |
| 61 | Exit status: 128+signum for signals | `libcontainer/utils/utils.go:12-23` |
| 62 | Foreground status is the pid1 exit status | `signals.go:94-111`, `:146-149`; `utils_linux.go:301-313` |
| 63 | CLI exit codes: 1 general, 255 for `exec` failure | `utils.go:56-68`; `main.go:177-179`; `exec.go:110-115` |
| 64 | Signal forwarding; `SIGWINCH`→resize, `SIGURG` swallowed | `signals.go:89-123` |
| 65 | PDN has no state file / FIFO / pid-file usage in `crates/podnext/src` | `grep -rn 'state.json\|exec.fifo\|pid-file' crates/podnext/src` → no output |
| 66 | proot's `fake_id0` and synthetic `/proc/self/mountinfo` | `research/PROOT_AND_PROOTDISTRO_SOURCE_DISSECTION.md:125`, `:129` (PDN repo, read-only) |

---

## 6. Open questions and unverified items

1. **Version.** The tag **v1.5.2 is unverified from the tree**: there is no `VERSION`, no `.git`, no
   `go.mod`. Everything above is anchored to line numbers in the extracted tree; if the same
   numbers are to be relied on later, the extraction must be pinned by hash.
   *Would settle it:* checkout of the tag with `.git` and `git describe --tags --exact-match`, or a
   `VERSION` file alongside the tree.
2. **Dependency versions unverified.** `vendor/` is present (`cyphar.com`, `github.com`,
   `golang.org`) but `vendor/modules.txt` and `go.mod` are absent, so the revision of
   `github.com/opencontainers/cgroups` behind §3.3 is unknown. The cgroup v2 file names and the
   write order could have changed between that revision and the one that ships today.
   *Would settle it:* `go.mod` + `vendor/modules.txt`.
3. **No build, no dynamic verification.** Nothing here was compiled or executed — no Go toolchain
   on the device and none was to be used. Therefore every statement is a *source* claim, not an
   observed-runtime claim. In particular I did **not** observe: the stderr text of a real runc
   failure, the actual on-disk `state.json`, or a real cgroup write.
   *Would settle it:* running `runc create`/`start`/`delete` on a Linux host with `--debug`.
4. **`tty.go` and `notify_socket.go` were read only in outline.** `notify_socket.go` (230 lines)
   implements the systemd `sd_notify` readiness contract; I did not trace the `NOTIFY_SOCKET`
   handshake in detail, so the "readiness is reported to systemd only after `procReady`" claim is
   **not made** above.
   *Would settle it:* read `notify_socket.go:1-230` and cross-reference
   `utils_linux.go:391-400` and `signals.go:76-83`.
5. **cgroup v1 (`fs/`, `systemd/`) was not dissected.** `manager/new.go:49-54` and the whole
   `vendor/.../cgroups/fs/` tree (v1, `devices`, `blkio`, …) are outside scope. PDN targets Android,
   which is cgroup v2 unified, so v1 is arguably irrelevant — but that is a judgement, not a
   measurement.
6. **`setns_init_linux.go` (166 lines) was not read line by line.** The `runc exec` path's own
   ordering is therefore asserted only from the parent side (`process_linux.go:460-594`) and from
   `container_linux.go:654-678`. If PDN ever needs an exec-into-existing-container contract, that
   file must be read.
   *Would settle it:* read `libcontainer/setns_init_linux.go` in full.
7. **No integration tests were read.** `tests/integration/` exists in the tree and is referenced by
   comments (e.g. `fs2/fs2.go:68-70`), but I did not read any `*.bats`. Those tests are the closest
   thing to a machine-checkable statement of the contracts in §3.
   *Would settle it:* read `tests/integration/*.bats` for `create.bats`, `delete.bats`,
   `cgroups.bats`.
8. **PDN-side assumption stated, not verified.** §4.2 asserts that namespace creation, `pivot_root`,
   real seccomp, real capabilities and cgroup v2 writes are impossible for PDN. The proot-side
   grounding is this repo's own `PROOT_AND_PROOTDISTRO_SOURCE_DISSECTION.md` (§1.3 of that file
   lists `fake_id0`, `mountinfo`, the ptrace state machine). I did **not** inspect PDN's
   `crates/proot-core/src/{ptrace,syscall,seccomp}.rs` line by line to confirm each impossibility
   at the code level.
   *Would settle it:* read `crates/proot-core/src/ptrace.rs` and `seccomp.rs`, and attempt a real
   `unshare(CLONE_NEWNS)` / `pivot_root` from inside a PDN container on-device.
