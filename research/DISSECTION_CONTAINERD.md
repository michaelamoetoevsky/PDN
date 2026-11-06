# Dissection: containerd v2.4.2 — supervisor/shim split, shim v2 wire, content+snapshots+GC+leases, events+task lifecycle

Audience: PDN (podnext), a proot-based container tool for Termux/Android.
Purpose: extract the mechanisms PDN must copy, adapt, or cannot copy. Every claim carries a
`file:line` anchor, a grep result, or command output. Where a thing could not be checked it is
marked **UNVERIFIED** with what would settle it.

Source tree (READ-ONLY): `/data/data/com.termux/files/home/.hermes/cache/scratch/src/containerd-src`

---

## 1. Method and inventory

### 1.1 Commands run (verbatim)

```sh
cd /data/data/com.termux/files/home/.hermes/cache/scratch/src/containerd-src
find . -type f | wc -l                 # 2115
find . -name '*.go' | wc -l            # 1979
for d in api cmd core internal plugins vendor; do \
  printf '%-10s files=%-6s go=%-6s go_nontest=%s\n' "$d" \
    "$(find $d -type f | wc -l)" "$(find $d -name '*.go' | wc -l)" \
    "$(find $d -name '*.go' ! -name '*_test.go' | wc -l)"; done
find . -name 'go.mod'                  # ./api/go.mod   (no root go.mod)
ls -1                                  # api cmd core internal plugins vendor
find core/runtime/v2 -maxdepth 1 -name '*.go' ! -name '*_test.go' | xargs wc -l
grep -rn "SCM_RIGHTS\|oob\|OOB\|fdpass\|PassedFd" --include=*.go .   # only vendor/wazero+hcsshim false hits
grep -rn "Pdeathsig\|SETSID\|Setsid" --include=*.go . | grep -v vendor   # only core/mount/...:46
grep -rn "TaskCreateEventTopic\|TaskExitEventTopic" --include=*.go . | grep -v _test
grep -rn "containerd.io/gc" --include=*.go core/ | grep -v _test
find plugins -type f -name '*.go' ! -name '*_test.go'   # 3 files, only plugins/server/internal
```

### 1.2 Inventory

| Path | files | .go | .go non-test | note |
|---|---|---|---|---|
| api | 174 | 117 | 116 | generated gRPC/ttrpc protos; has its own `api/go.mod` |
| cmd | 29 | 29 | 26 | `cmd/containerd/{main.go,command/,server/,builtins/}` |
| core | 277 | 276 | 206 | metadata, events, runtime, snapshots, content, leases |
| internal | 315 | 315 | 219 | cri, nri, eventq, fsverity, ... |
| plugins | 3 | 3 | 3 | **only `plugins/server/internal`** — service plugins not extracted |
| vendor | 1317 | 1239 | 1239 | includes go-etcd/bbolt, grpc, protobuf |
| **total** | **2115** | **1979** | 1808 | |

Selective nature confirmed: no root `go.mod`, no `version/` package, no `runtime/v2/` at the
repo root — the shim code lives under `core/runtime/v2/`. `plugins/` is nearly empty, so the
*daemon-side service plugins* (tasks, events, content services) are absent; only the client-side
and core-level machinery is present.

`core/runtime/v2` (non-test): shim.go 975, shim_manager.go 524, task_manager.go 461, bridge.go 330,
shim_load.go 296, binary.go 236, bundle.go 184, process.go 161, command.go 159, task_mounts.go 149,
shim_unix.go 125, bundle_linux.go 74,
plus task_mounts_deprecated.go, socket_{linux,unix,windows}.go, manager_{unix,windows}.go,
command_{unix,windows}.go, shim_windows.go — **4184 lines total**.

`core/metadata`: gc.go 1158, snapshot.go 1008, content.go 961, db.go 569, leases.go 563,
images.go 467, containers.go 459, sandbox.go 418, buckets.go 317, migrations.go 259,
namespaces.go 207, adaptors.go 177, bolt.go 55 — **6618 lines**.

`core/events`: events.go 80, exchange/exchange.go 248, proxy/remote_events.go 231.

### 1.3 Tag

**v2.4.2 is the tree's stated tag but is NOT self-evidencing in the extract.** There is no
`version/version.go`, no `.git`, no root `go.mod`.
Corroborating internal signals: `core/runtime/v2/shim.go:234 const CurrentShimVersion = 3`,
`runtime/task/v3` protos present, config migration code that explicitly upgrades 1.7.x state
(`core/runtime/v2/shim_manager.go:114-142`), `api/go.mod:3 go 1.25.0` and `ttrpc v1.2.9`.
Settling it: `git -C <checkout> describe --tags` on the original checkout, or presence of
`version/version.go` containing `Version = "2.4.2"`.

---

## 2. Architecture map: daemon → shim → runtime

```
containerd (daemon, one process)
  │  ShimPlugin "manager"  ──────────────► ShimManager  (core/runtime/v2/shim_manager.go:176)
  │     manages ShimInstance map, NSMap[ShimInstance]        (shim_manager.go:180)
  │
  ├─ start a task ─► TaskManager.Create                        (task_manager.go:159)
  │      NewBundle → MkdirAll state/<ns>/<id>, mkdir rootfs    (bundle.go:50-133)
  │      ShimManager.Start(ctx,id,bundle,opts)                 (shim_manager.go:205)
  │         b := shimBinary(bundle, cfg{runtime,tcp/grpc addr,socketDir,env})  (shim_manager.go:290)
  │         b.Start(...)  ─► command() builds exec.Cmd         (binary.go:66 / command.go:67)
  │            runtime path resolved from runtime NAME         (shim_manager.go:422 resolveRuntimePath)
  │            cmd.Stdin = marshalled BootstrapParams (proto)  (command.go:129-155)
  │            cmd.SysProcAttr = {Setpgid:true}                (command_unix.go:23-26)
  │         out,err := cmd.CombinedOutput()  ── shim prints BootstrapResult on stdout (binary.go:115)
  │         parseStartResponse(out) → {Version,Protocol,Address,Extensions} (shim.go:293)
  │         writeBootstrapParams(bundle/bootstrap.json)         (binary.go:140 / shim.go:324)
  │         makeConnection(protocol ttrpc|grpc, address, onClose)  (shim.go:369)
  │         ⇒ *shim{ bundle, client, address, version, bootstrap }  (shim.go:479)
  │
  ├─ per-task calls ─► shimTask wraps ShimInstance + TaskServiceClient   (shim.go:598,603)
  │      Create/Start/State/Wait/Kill/Delete/Pause/...  → ttrpc/grpc Task v3 methods
  │      e.g. State()  (shim.go:952) ; PID() via Connect()  (shim.go:633)
  │
  └─ restart ─► TaskManager init calls LoadExistingShims(state,root)  (task_manager.go:102)
         read state/<ns>/* dirs, LoadBundle, loadShim  (shim_load.go:39,66,128)
         restoreBootstrapParams(bundle) → bootstrap.json        (shim.go:116 / shim_manager.go:392)
         makeConnection(..., AnonReconnectDialer)               (shim.go:121)

shim process (containerd-shim-*, SEPARATE process, one per container/sandbox)
  holds the runtime (runc) container; exposes containerd.task.v3.Task over unix socket;
  prints BootstrapResult on stdout at startup; logs via FIFO bundle/log (shim_unix.go:37)
```

Poster child for why the split exists: `shim_manager.go:204-206` comment — *"Start launches a new
shim instance"* and `shim_load.go:37-38` — *"LoadExistingShims loads existing shims from the path
specified by stateDir"*. The daemon is stateless w.r.t. running containers; state is on disk
(`bootstrap.json`, bundle dir, shim socket) and rediscovered at startup.

---

## 3. The four mechanisms

### 3.1 Supervisor/shim split

**What it buys.** The shim is launched as its own process in its **own process group**
(`Setpgid:true`), and containerd sets **no `Pdeathsig`** for it. Therefore containerd exiting
does not signal the shim or the container. Grep proof:

```
grep -rn "Pdeathsig" --include=*.go . | grep -v vendor
./core/mount/mount_idmapped_utils_linux.go:46:   Pdeathsig: syscall.SIGKILL,
```
That single hit is for the *mount helper* (a child of containerd), not the shim. The shim's
`SysProcAttr` is only `Setpgid`:

`core/runtime/v2/command_unix.go:23-26`
```go
func getSysProcAttr() *syscall.SysProcAttr {
	return &syscall.SysProcAttr{
		Setpgid: true,
	}
}
```

**How the shim is started** — `core/runtime/v2/command.go:129-156`
```go
} else if config.Action == "start" {
	// Use the new Bootstrap protocol for all newer shims.
	params := bootapi.BootstrapParams{
		InstanceID:             config.ID,
		Namespace:              ns,
		LogLevel:               bootapi.LogLevelFromString(config.LogLevel.String()),
		ContainerdVersion:      version.Version,
		ContainerdGrpcAddress:  config.GRPCAddress,
		ContainerdTtrpcAddress: config.TTRPCAddress,
		ContainerdBinary:       self,
	}
	if config.SocketDir != "" {
		params.SocketDir = &config.SocketDir
	}
	if config.Opts != nil {
		if err := params.AddExtension(config.Opts); err != nil {
			return nil, fmt.Errorf("unable to add runtime options extensions: %w", err)
		}
	}
	data, err := proto.Marshal(&params)
	...
	cmd.Stdin = bytes.NewReader(data)
}
```
Bootstrap params cross on **stdin** (marshalled proto); the shim answers on **stdout** with the
`BootstrapResult` (`binary.go:115 out, err := cmd.CombinedOutput()`, `binary.go:129 parseStartResponse(out)`).

**How the daemon finds the shim again after a restart.**
`binary.go:139-142` persists the result:
```go
// Save bootstrap configuration (so containerd can restore shims after restart).
if err := writeBootstrapParams(filepath.Join(b.bundle.Path, "bootstrap.json"), params); err != nil {
	return nil, fmt.Errorf("failed to write bootstrap.json: %w", err)
}
```
On startup, `task_manager.go:102` calls `shimManager.LoadExistingShims(ic.Context, state, root)`,
which walks `stateDir/<ns>/<id>` dirs (`shim_load.go:39-63,66-126`), and for each bundle:

`shim_load.go:116` → `loadShim` → `shim.go:116`:
```go
params, err := restoreBootstrapParams(bundle.Path)
if err != nil {
	return nil, fmt.Errorf("failed to read bootstrap.json when restoring bundle %q: %w", bundle.ID, err)
}
conn, err := makeConnection(ctx, bundle.ID, params, onCloseWithShimLog, client.AnonReconnectDialer)
```
`AnonReconnectDialer` = *fail fast if pipe is missing* vs `AnonDialer` = *retry, pipe may not exist
yet* (`shim.go:366-368` comment). Legacy shims with only an `address` file are migrated in place
(`shim_manager.go:392-420` writes `bootstrap.json` from the old `address`).

**Disconnect and reaping.** Each shim client carries an `onClose` callback that, when the shim dies,
runs `cleanupAfterDeadShim` (delete the dead shim, publish task exit+delete) and removes it from the
manager map (`shim_manager.go:297-306`, `shim.go:146-196`). A shim that had never created a task is
reaped on load (`shim_load.go:197-215`, `shouldCleanupShim` at `shim_load.go:222-226`).

---

### 3.2 Shim v2 protocol (ttrpc bootstrap, fds, stdio, exit status)

**Service definition (Task v3, ttrpc).** `api/runtime/task/v3/shim_ttrpc.pb.go:11-29`
```go
type TTRPCTaskService interface {
	State(context.Context, *StateRequest) (*StateResponse, error)
	Create(context.Context, *CreateTaskRequest) (*CreateTaskResponse, error)
	Start(context.Context, *StartRequest) (*StartResponse, error)
	Delete(context.Context, *DeleteRequest) (*DeleteResponse, error)
	Pids(context.Context, *PidsRequest) (*PidsResponse, error)
	Pause(context.Context, *PauseRequest) (*emptypb.Empty, error)
	Resume(context.Context, *ResumeRequest) (*emptypb.Empty, error)
	Checkpoint(context.Context, *CheckpointTaskRequest) (*emptypb.Empty, error)
	Kill(context.Context, *KillRequest) (*emptypb.Empty, error)
	Exec(context.Context, *ExecProcessRequest) (*emptypb.Empty, error)
	ResizePty(context.Context, *ResizePtyRequest) (*emptypb.Empty, error)
	CloseIO(context.Context, *CloseIORequest) (*emptypb.Empty, error)
	Update(context.Context, *UpdateTaskRequest) (*emptypb.Empty, error)
	Wait(context.Context, *WaitRequest) (*WaitResponse, error)
	Stats(context.Context, *StatsRequest) (*StatsResponse, error)
	Connect(context.Context, *ConnectRequest) (*ConnectResponse, error)
	Shutdown(context.Context, *ShutdownRequest) (*emptypb.Empty, error)
}
```
Registered as service name `"containerd.task.v3.Task"` (`shim_ttrpc.pb.go:32`). v2 equivalent exists
at `api/runtime/task/v2/shim_ttrpc.pb.go`; containerd auto-downgrades v3→v2 on `ErrNotImplemented`
(`shim.go:244-256`, `shim.go:511-518 Downgrade()`).

**Bootstrap handshake, both directions.**

*daemon → shim* (stdin, proto) — `api/runtime/bootstrap/v1/bootstrap.pb.go:166-198`
```go
type BootstrapParams struct {
	InstanceID string
	Namespace  string
	LogLevel LogLevel
	ContainerdVersion string
	ContainerdTtrpcAddress string
	ContainerdGrpcAddress string
	ContainerdBinary string
	Extensions []*Extension
	SocketDir *string
}
```
*shim → daemon* (stdout, proto) — `bootstrap.pb.go:350-378`
```go
type BootstrapResult struct {
	Version int32      // "expected 2 for shim v2"
	Address string     // e.g. "unix:///run/containerd/shim.sock"
	Protocol string    // "ttrpc" or "grpc"
	Capabilities []Capability
	Metadata map[string]string
	Extensions []*Extension
}
```
Parsed leniently, with a legacy fallback and a version ceiling — `shim.go:300-320`:
```go
// Fallback to legacy parsing ... that return the address as a plain string or JSON.
response = bytes.TrimSpace(response)
params := &bootapi.BootstrapResult{}
if err := json.Unmarshal(response, params); err != nil || params.Version < 2 {
	params = &bootapi.BootstrapResult{Address: string(response), Protocol: "ttrpc", Version: 2}
}
if params.Version > CurrentShimVersion {
	return nil, fmt.Errorf("unsupported shim version (%d): %w", params.Version, errdefs.ErrNotImplemented)
}
```

**What crosses the boundary.**

- **stdio = FIFO paths, not fds.** The protobuf carries *strings*: `CreateTaskRequest.Stdin/Stdout/Stderr`
  (`api/runtime/task/v3/shim.pb.go:52-54`), set from `opts.IO.*` in `shim.go:721-724`. The shim-log
  channel is a FIFO opened by the daemon: `shim_unix.go:37-39`
  ```go
  func openShimLog(ctx context.Context, bundle *Bundle, _ func(string, time.Duration) (net.Conn, error)) (io.ReadCloser, error) {
  	return fifo.OpenFifo(ctx, filepath.Join(bundle.Path, "log"), unix.O_RDWR|unix.O_CREAT|unix.O_NONBLOCK, 0700)
  }
  ```
  Supported log URI schemes for the daemon: `fifo, binary, binary-v2, file` (`task_manager.go:345-352`).
- **fds via SCM_RIGHTS: UNVERIFIED in this tree.** `grep -rn "SCM_RIGHTS|PassedFd|oob" --include=*.go .`
  returns only unrelated vendor hits (wazero, hcsshim). The `github.com/containerd/ttrpc` source is
  **not vendored** (`ls vendor/github.com/containerd/` → `containerd errdefs nri otelttrpc`), and ttrpc's
  fd-passing lives in that module. What would settle it: read `ttrpc@v1.2.9` `client.go`/`server.go`
  for the `SCM_RIGHTS` sendmsg path and the `Metadata`/`Oob` mechanism.
- **Socket address.** Unix socket path = `<socketDir>/<sha256hash>`; the dir is capped at
  `maxSocketDirLen = 42` (Linux, `socket_linux.go:19-25`) because `107 - 1 - 64 = 42`.
- **Bundle path.** `state/<ns>/<id>`, with `rootfs/`, a `work` symlink, `config.json`,
  `bootstrap.json`, `log` FIFO, `shim-binary-path` (`bundle.go:59-131`, `binary.go:125`).

**Exit status.** Represented as a `uint32` three ways, all mapped onto `runtime.Exit`:

`api/runtime/task/v3/shim.pb.go:264-272`
```go
type DeleteResponse struct {
	Pid        uint32
	ExitStatus uint32
	ExitedAt   *timestamppb.Timestamp
}
```
`core/runtime/task.go:62-67`
```go
type Exit struct {
	Pid       uint32
	Status    uint32
	Timestamp time.Time
}
```
Dead-shim fallback synthesises status 255 — `shim.go:174-188`:
```go
if response != nil {
	pid = response.Pid; exitStatus = response.Status; exitedAt = response.Timestamp
} else {
	exitStatus = 255
	exitedAt = time.Now()
}
events.Publish(ctx, runtime.TaskExitEventTopic, &eventstypes.TaskExit{ ... ExitStatus: exitStatus ... })
```
`Wait`/`State` carry the same fields (`shim.pb.go:1224` WaitResponse, `:586` StateResponse), surfaced
in `shim.go:890-906, 952-975`.

---

### 3.3 Content store, snapshotters, GC, leases

**The content store is a two-level design:** metadata (bolt buckets) + a byte store. `Store`
composes four interfaces — `content/content.go:41-46`:
```go
type Store interface { Manager; Provider; IngestManager; Ingester }
```
Lifecycle doc, `content/content.go:31-40`: *"Ingester is used to initiate a write operation …
Once an ingestion is complete (see Writer.Commit), Provider is used to query a single piece of
content by its digest … until ingestion is complete, its content is not visible through Provider
or Manager."* Hence an ingest is not a blob until committed.

**Ingest → commit.** `core/metadata/content.go:427-435` creates the ingest bucket and attaches an
ingest lease; `:582-642 Commit` pre-syncs, then in one bolt tx writes the blob bucket, deletes the
ingest bucket, swaps ingest lease for content lease, and publishes:

`core/metadata/content.go:631-639`
```go
if publisher := nw.db.Publisher(ctx); publisher != nil {
	if err := publisher.Publish(ctx, "/content/create", &eventstypes.ContentCreate{
		Digest:    actualDgst.String(),
		Size:      actualSize,
		MediaType: nw.desc.MediaType,
	}); err != nil {
		return err
	}
}
```
`commit` enforces size and expected digest (`content.go:670-694`):
```go
if size != 0 && size != nw.desc.Size {
	return "", 0, fmt.Errorf("%q failed size validation: %v != %v: %w", ... errdefs.ErrFailedPrecondition)
}
if expected != "" && expected != nw.desc.Digest {
	return "", 0, fmt.Errorf("%q unexpected digest: %w", nw.ref, errdefs.ErrFailedPrecondition)
}
```

**Blob verification.** `metadata/content.go` validates *declared* size/digest at commit, but the
byte-level re-hash on write lives in the local content store plugin, which is **not in this extract**
(`core/content/` contains only `adaptor.go`, `content.go`, `helpers.go`, `proxy/`, `testsuite/`).
**UNVERIFIED**: the exact digest-verify-on-write code path. What *is* present is the copied-bytes
size guard in `content/helpers.go:224-245 Copy()` and `content/helpers.go:69-77 useDescriptorData`
(validates `desc.Digest.Algorithm().FromBytes(desc.Data) == desc.Digest` for inline data).
Settling it: read `github.com/containerd/containerd/v2/plugins/content/local` `writer.go`, whose
reference is even named in `metadata/content.go:598` comment.

**Snapshot vs layer.** A *snapshot* is a filesystem state with a parent (chain), managed
independently of blobs; a *layer* is a tar blob in the content store. `core/snapshots/snapshotter.go:169-182`:
```go
// A snapshot represents a filesystem state. Every snapshot has a parent, where
// the empty parent is represented by the empty string. A diff can be taken
// between a parent and its snapshot to generate a classic layer.
// An active snapshot is created by calling `Prepare`. After mounting, changes
// can be made to the snapshot. The act of committing creates a committed
// snapshot. ... Active snapshots can never act as a parent.
```
Kinds: `KindView, KindActive, KindCommitted` (`snapshotter.go:73-78`). The binding from a layer blob
to a snapshot is by **label**: `LabelSnapshotRef = "containerd.io/snapshot.ref"` (target chainID),
`LabelSnapshotDiffID = "containerd.io/snapshot/diff-id"` (`snapshotter.go:37-46`), and only
`containerd.io/snapshot/`-prefixed labels are inherited (`snapshotter.go:406-421`).
Metadata impl: `core/metadata/snapshot.go:274 Prepare`, `:293 View`, `:520 Commit`, `:683 Remove`,
`:862 garbageCollect` (prunes branches via `walkTree`/`pruneBranch`, `:949,983`).

**GC is mark-and-sweep over bolt, rooted in labels.** Root/reference label constants,
`core/metadata/gc.go:66-95`:
```go
labelGCRoot = []byte("containerd.io/gc.root")
labelGCRef        = []byte("containerd.io/gc.ref.")
labelGCSnapRef    = []byte("containerd.io/gc.ref.snapshot.")
labelGCContentRef = []byte("containerd.io/gc.ref.content")
labelGCImageRef   = []byte("containerd.io/gc.ref.image")
labelGCContainerBackRef = []byte("containerd.io/gc.bref.container")
labelGCContentBackRef   = []byte("containerd.io/gc.bref.content")
labelGCImageBackRef     = []byte("containerd.io/gc.bref.image")
labelGCSnapBackRef      = []byte("containerd.io/gc.bref.snapshot.")
labelGCExpire = []byte("containerd.io/gc.expire")
labelGCFlat   = []byte("containerd.io/gc.flat")
```
`labelGCRef*` = parent→child edges; `labelGC*BackRef` = child→parent edges (no parent mutation);
`gc.expire` = time-boxed root; `gc.flat` = lease the roots only, not their tree.
Scanning: `gc.go:495 scanRoots` walks every namespace; for each lease (`gc.go:561-657`) it collects
the lease node, its content, snapshots, ingests and images as roots — *unless* the lease is flat
(`gc.go:596-598 if flat { ctype = resourceContentFlat }`) — and treats expired leases as expired
(`gc.go:571-580`). Label handlers (ref + backref + conditional `containerd.io/gc.cond.snapshot`)
are wired in `gc.go:223-345`. Removal deletes the bolt bucket and returns an event payload —
`gc.go:1034-1107` (e.g. `gc.go:1069 &eventstypes.SnapshotRemove{...}`).

**Leases** retain resources so concurrent GC does not delete them. Manager API,
`core/leases/lease.go:32-39`:
```go
type Manager interface {
	Create(context.Context, ...Opt) (Lease, error)
	Delete(context.Context, Lease, ...DeleteOpt) error
	List(context.Context, ...string) ([]Lease, error)
	AddResource(context.Context, Lease, Resource) error
	DeleteResource(context.Context, Lease, Resource) error
	ListResources(context.Context, Lease) ([]Resource, error)
}
```
Expiration is a label, not a field — `lease.go:93-102` sets `containerd.io/gc.expire =
now+d` formatted RFC3339. Impl `core/metadata/leases.go:51 Create`, `:186 AddResource` (creates the
nested `content`/`snapshots`/`ingests`/`images` buckets under the lease and puts the ref, `:203-210`),
`:386 addContentLease`, `:337 addSnapshotLease`, `:430 addIngestLease`. Writers opportunistically
lease on open: `metadata/content.go:405 addContentLease` when the blob already exists.

---

### 3.4 Event service and task lifecycle

**Event model.** An event is `{Timestamp, Namespace, Topic, Event(typeurl.Any)}` packaged as an
`Envelope` — `core/events/events.go:26-32`:
```go
type Envelope struct {
	Timestamp time.Time
	Namespace string
	Topic     string
	Event     typeurl.Any
}
```
Publishers/subscribers: `events.go:68-79` (`Publisher.Publish`, `Forwarder.Forward`,
`Subscriber.Subscribe(ctx, filters...) (ch, errs)`).

**Task topics** — `core/runtime/events.go:24-47`: `/tasks/create`, `/tasks/start`, `/tasks/oom`,
`/tasks/exit`, `/tasks/delete`, `/tasks/exec-added`, `/tasks/exec-started`, `/tasks/paused`,
`/tasks/resumed`, `/tasks/checkpointed`. `GetTopic(e)` type-switches an event value to its topic
(`events.go:51-77`).

**Subscription.** In-process: `core/events/exchange/exchange.go:128-199 Subscribe` builds a
`goevents.Channel`→`Queue` (optionally wrapped in a filter) and fans out envelopes; `Publish`
stamps `time.Now().UTC()` and the namespace from context (`exchange.go:80-119`). Over the wire the
service is `containerd.services.events.v1.Events` with `Forward`, `Subscribe` (server-streaming) —
`api/services/events/v1/events_ttrpc.pb.go:12-16`, request carries `Filters []string`
(`events.pb.go:143-149 ForwardRequest/SubscribeRequest`). A shim or external process reaches the
daemon's bus via the proxy: `core/events/proxy/remote_events.go:42-63` chooses grpc/ttrpc client and
implements all three interfaces (`:69,:84,:99` grpc; `:153,:168,:183` ttrpc).

**State transitions.** The status enum lives in `core/runtime/task.go:103-116`:
```go
const (
	CreatedStatus Status = iota + 1
	RunningStatus
	StoppedStatus
	DeletedStatus
	PausedStatus
	PausingStatus
)
```
and is carried over the wire as a numeric field in `StateResponse` (`api/runtime/task/v3/shim.pb.go:586`),
decoded via `statusFromProto` (`shim.go:967`). `State` includes `Pid, Stdin, Stdout, Stderr,
Terminal, ExitStatus, ExitedAt` (`runtime/task.go:119-134`). The transitions themselves are published
by the shim process (not in this extract); the daemon only publishes **exit** and **delete** for a
shim that died without delivering them — `shim.go:169-195` (quoted in 3.2).

The container record that ties it together is a bolt bucket keyed by ID with a `Runtime.Name` and
`SnapshotKey`, GC-managed by labels; delete removes the bucket and bumps the dirty counter —
`core/metadata/containers.go:287-321`, validation at `:323-354`.

---

## 4. What PDN should take, and what it cannot

### Take (portable to proot)

1. **Process split, not goroutine split.** A supervisor process per container, in its own process
   group, with **`Pdeathsig` unset**, so a supervisor restart never kills containers. This is the
   single most portable idea: PDN's supervisor ≈ containerd, PDN's per-container process ≈ the shim.
   Anchors: `command_unix.go:23-26`, `shim.go:66-152`.
2. **Bootstrap over stdin/stdout + a persisted record.** Shims are started with marshalled params on
   stdin and answer on stdout; the answer is cached in `bootstrap.json` inside the bundle
   (`command.go:129-155`, `binary.go:139-142`, `shim.go:116-121`). PDN can use the same shape without
   protobuf — JSON over a pipe plus a per-container `bootstrap.json` gives restart-reconnect for free.
3. **Reconnect-by-scan.** On startup, enumerate state dirs, read each record, dial each endpoint with
   a *fast-fail* dialer, and only then register (`shim_load.go:39-126`, `shim.go:366-368`). PDN's
   detach/attach should scan a state directory the same way, not keep an in-memory registry.
4. **stdio as named FIFOs, not inherited fds.** `fifo.OpenFifo(bundle/log, O_RDWR|O_CREAT|O_NONBLOCK)`
   and protobuf string paths (`shim_unix.go:37-39`, `shim.pb.go:52-54`). On Termux this sidesteps
   fd-lifetime problems across a detached process; PDN can literally reuse FIFO paths.
5. **Exit status as `uint32` with a sentinel for "unknown".** The daemon synthesises `255` when the
   shim is gone (`shim.go:178-181`). PDN should adopt an explicit `unknown` exit sentinel instead of
   collapsing "no data" into success.
6. **An immutable, content-addressed blob store with a two-phase ingest.** The `Store` split
   (inline ingest vs committed blob, `content/content.go:31-46`) is the fix for PDN's blocked
   push/pull/export: it keeps layer blobs, which the current PDN image store does not. The minimum
   viable version is: ingest to a temp path, verify size+digest, atomically rename to
   `blobs/<algo>/<hex>`, record metadata separately.
7. **Snapshots as a parent-linked chain separate from blobs**, with the layer→snapshot binding held
   in **labels** (`snapshotter.go:169-182`, `snapshotter.go:37-46`). A layer can then be shared by N
   container rootfs mounts without copying.
8. **Label-driven mark-and-sweep GC plus leases.** The `containerd.io/gc.root` / `gc.ref.*` /
   `gc.bref.*` / `gc.expire` vocabulary and the "walk every namespace, treat leases + roots as marks,
   sweep the rest" loop (`gc.go:66-119`, `gc.go:495-657`, `gc.go:1034-1107`) is directly reusable
   with a bolt-like or SQLite store. Leases (`leases/lease.go:32-39`, `metadata/leases.go:186-211`)
   are what let pull/unpack run concurrently with GC safely.
9. **A topic-string event bus with typed envelopes and filter subscriptions.** Topics `/tasks/*` and
   `/content/*`, `Envelope{Timestamp,Namespace,Topic,Event}` (`events.go:26-32`), subscribe with
   filters (`exchange.go:128-199`), and forward across process boundaries via a proxy interface
   (`remote_events.go:36-63`). PDN can implement the same with a unix socket + JSON lines.
10. **A shim version + downgrade path.** `CurrentShimVersion = 3` (`shim.go:234`) and automatic
    v3→v2 retry on `ErrNotImplemented` (`shim.go:244-256`). PDN should version its supervisor protocol
    from day one; it is cheap now and impossible to retrofit.

### Cannot take (or must be replaced)

1. **Namespaces to hand to a shim.** The whole chain assumes the runtime program can create Linux
   namespaces (`clone`/`unshare` with `CLONE_NEWNS|NEWPID|...`) and that the shim's only job is to
   supervise that. The extract shows the assumption implicitly: nothing in `core/runtime/v2` sets up
   namespaces; it delegates to the OCI runtime binary (`resolveRuntimePath`, `shim_manager.go:422-498`).
   proot has no kernel namespaces of its own — it is a `ptrace`-based syscall interposer with a
   virtual rootfs. PDN therefore cannot have a shim that "just supervises a namespaced process"; the
   isolation semantics live in proot, and the supervisor split must be justified on process-lifetime
   grounds alone, not isolation grounds.
2. **mount(2)-based rootfs assembly.** `Bundle.rootfs` is realised by mounts; `mount.UnmountRecursive`
   is called on delete (`bundle.go:149`) and snapshot mounts are passed to the runtime
   (`shim.go:728-735`). proot rootfs is a bind-translated directory tree, not a mount stack. The
   snapshotter abstraction survives; the *mounts* return type does not.
3. **ttrpc/gRPC and ttrpc fd-passing.** `makeConnection` speaks `ttrpc` or `grpc`
   (`shim.go:369-402`). That is a large dependency and it is exactly where fd-passing lives — and
   that module is **not** in this extract. PDN should not adopt ttrpc; a plain length-prefixed JSON
   protocol over a unix socket covers the same surface (`shim_ttrpc.pb.go:11-29` is only 17 methods).
4. **`Setpgid` alone as the "survives restart" guarantee.** It works on Linux; on Android/Termux the
   app's process group, `SIGCHLD` handling, and the Termux `run-command`/`am` service lifecycle add
   constraints. The containerd mechanism is necessary but not sufficient for PDN; PDN additionally
   needs its own supervision of the proot child. **UNVERIFIED on device** — would need a real test.
5. **Bolt's single-writer transactional model for GC.** `gc.go` relies on `bolt.Tx` for the whole
   mark phase. That is a design constraint as much as a feature; PDN must pick a store with the same
   "one transaction sees a consistent snapshot" property or GC will race with writes.

---

## 5. Claims table

| # | Claim | Evidence (file:line / command) |
|---|---|---|
| 1 | Extract is 2115 files, 1979 `.go` | `find . -type f \| wc -l` → 2115; `find . -name '*.go' \| wc -l` → 1979 |
| 2 | No root `go.mod`; only `api/go.mod` | `find . -name 'go.mod'` → `./api/go.mod` |
| 3 | No `version/` pkg; tag not self-evidencing | `ls -1` top-level; `grep -rn version... version/` empty |
| 4 | Shim code is under `core/runtime/v2`, not `runtime/v2` | `find core/runtime/v2 -maxdepth 1 -name '*.go' ! -name '*_test.go'` |
| 5 | Service plugins absent | `find plugins -type f -name '*.go' ! -name '*_test.go'` → 3 files, all `plugins/server/internal` |
| 6 | Shim is a separate process, one per container | `shim_manager.go:204-206`, `binary.go:66-152` |
| 7 | Shim gets its own process group; no Pdeathsig | `command_unix.go:23-26`; `grep -rn Pdeathsig` → only `core/mount/mount_idmapped_utils_linux.go:46` |
| 8 | Bootstrap params cross on stdin | `command.go:129-155` |
| 9 | Bootstrap result returns on stdout | `binary.go:115,129`; `bootstrap.pb.go:349-378` |
| 10 | Result cached to `bootstrap.json` for restart | `binary.go:139-142`, `shim.go:324-347` |
| 11 | Reload scans `stateDir/<ns>/<id>` at daemon start | `task_manager.go:102`, `shim_load.go:39-126` |
| 12 | Reconnect uses fail-fast dialer | `shim.go:116-124`, `shim.go:366-368` |
| 13 | Legacy shim migrated from `address` file | `shim_manager.go:392-420` |
| 14 | Dead-shim cleanup publishes exit+delete | `shim.go:146-196` |
| 15 | Task v3 ttrpc service = 17 methods, name `containerd.task.v3.Task` | `shim_ttrpc.pb.go:11-29,32` |
| 16 | stdio crosses as paths, not fds | `shim.pb.go:52-54`, `shim.go:721-724` |
| 17 | Shim log is a FIFO | `shim_unix.go:37-39` |
| 18 | Log URI schemes: fifo/binary/binary-v2/file | `task_manager.go:345-352` |
| 19 | Unix socket dir capped at 42 chars | `socket_linux.go:19-25` |
| 20 | Exit status is `uint32`; dead-shim fallback 255 | `shim.pb.go:269-271`, `shim.go:174-188`, `runtime/task.go:62-67` |
| 21 | SCM_RIGHTS fd-passing | **UNVERIFIED** — no hits; ttrpc not vendored (`ls vendor/github.com/containerd/`) |
| 22 | Content store = Manager+Provider+IngestManager+Ingester | `content/content.go:41-46` |
| 23 | Ingest invisible until Commit | `content/content.go:31-40` |
| 24 | Commit enforces size + expected digest | `metadata/content.go:670-694` |
| 25 | Commit publishes `/content/create` | `metadata/content.go:631-639` |
| 26 | Commit swaps ingest lease→content lease | `metadata/content.go:604-623` |
| 27 | Byte-level re-hash on write | **UNVERIFIED** — local content plugin not extracted (`ls core/content/`) |
| 28 | Snapshot = parented fs state; layer = tar blob | `snapshotter.go:169-182` |
| 29 | Snapshot kinds View/Active/Committed | `snapshotter.go:73-78` |
| 30 | Layer→snapshot binding via labels | `snapshotter.go:37-46`, `snapshotter.go:406-421` |
| 31 | GC root/ref/bref/expire/flat label vocabulary | `gc.go:66-119` |
| 32 | scanRoots iterates namespaces; leases become roots | `gc.go:495-657` |
| 33 | Flat lease leases roots only | `gc.go:583-598` |
| 34 | Expired lease skipped | `gc.go:571-580` |
| 35 | gc remove deletes bolt buckets, emits events | `gc.go:1034-1107` |
| 36 | Lease expiry is a label | `leases/lease.go:93-102` |
| 37 | Lease manager API (6 methods) | `leases/lease.go:32-39` |
| 38 | AddResource nests content/snapshots/ingests/images | `metadata/leases.go:186-211` |
| 39 | Envelope = Timestamp/Namespace/Topic/Event | `events/events.go:26-32` |
| 40 | Task topics list | `core/runtime/events.go:24-47` |
| 41 | Publish stamps time+namespace; subscribe filters | `exchange.go:80-119`, `exchange.go:128-199` |
| 42 | Wire events service: Forward + streaming Subscribe | `events_ttrpc.pb.go:12-16`; `events.pb.go:143-149` |
| 43 | Remote event proxy for shim/external | `proxy/remote_events.go:36-63,99-147,183-231` |
| 44 | Status enum Created/Running/Stopped/Deleted/Paused/Pausing | `runtime/task.go:103-116` |
| 45 | Daemon publishes exit/delete only for orphan shims | `shim.go:182-195`; `grep Task*EventTopic` non-test → only `core/runtime/v2/shim.go:182,190` |
| 46 | Container record bucket + validation | `metadata/containers.go:123-175,287-321,323-354` |
| 47 | Shim version 3 with downgrade | `shim.go:234,244-256,511-518` |

---

## 6. Open questions / could not verify

1. **The tag.** No VCS metadata or `version/version.go` in the extract. Witnessed signals are
   consistent with 2.x, not proof of 2.4.2. Settle with `git describe --tags` on the origin checkout or
   `version.Version` in the un-extracted `version/` package.
2. **fd-passing over ttrpc (SCM_RIGHTS).** `github.com/containerd/ttrpc` is not vendored and no source
   in this tree references `SCM_RIGHTS`. Settle by reading `ttrpc@v1.2.9` (pinned in `api/go.mod:6`).
3. **Byte-level blob digest verification.** The local content-store writer is absent
   (`core/content/` = `adaptor.go`, `content.go`, `helpers.go`, `proxy/`, `testsuite/`). Settle by
   reading `plugins/content/local/writer.go` (referenced at `metadata/content.go:598`).
4. **Task-state transition logic.** The state machine that emits `/tasks/create|start|paused|resumed`
   lives in the daemon's tasks service plugin and the shim binary, neither of which is extracted
   (`plugins` has 3 files; the shim is an external module). Only the exit/delete path is visible here.
   Settle by adding `plugins/services/tasks/` and `containerd-shim-runc-v2`.
5. **Supervision behaviour on Android/Termux.** Whether `Setpgid` + no `Pdeathsig` actually keeps a
   proot child alive across a PDN supervisor restart under Termux's process lifecycle is untested here.
   Settle with a device test: start a child, kill the supervisor, confirm the child and its output FIFO
   survive, then re-attach.
6. **GC triggers and cadence.** `gc.go` and `metadata/content.go:845 garbageCollect` return a
   `time.Duration` (next-run hint), but the scheduler that calls them is in the un-extracted daemon
   plugin. Not verified.
7. **Bolt version / DB layout details.** `core/metadata/buckets.go` defines the key namespace, but the
   full durable schema (migrations) was only skimmed (`migrations.go`, 259 lines) and is not
   reproduced here.
8. **`internal/eventq`** (a queue used around events on some platforms) was listed but not read;
   its relationship to `core/events/exchange` is unstated.

---

### Appendix — files read in full (non-test)

`core/runtime/{task.go,events.go,runtime.go}`; `core/events/events.go`; `core/events/exchange/exchange.go`;
`core/events/proxy/remote_events.go`; `core/leases/lease.go`; `core/content/{content.go,helpers.go}`;
`core/snapshots/snapshotter.go`; `core/runtime/v2/{shim.go,shim_manager.go,shim_load.go,binary.go,
command.go,command_unix.go,bundle.go,bundle_linux.go,task_manager.go,socket_linux.go,socket_unix.go,
manager_unix.go}`; `api/runtime/task/v3/{shim_ttrpc.pb.go,shim.pb.go (targeted ranges)}`;
`api/runtime/bootstrap/v1/bootstrap.pb.go (166-405)`; `api/services/events/v1/{events_ttrpc.pb.go,
events.pb.go (143-182)}`; `api/go.mod`; `core/metadata/{gc.go,content.go,leases.go,containers.go,
snapshot.go (selected ranges)}`.
