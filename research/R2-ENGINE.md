# R2-ENGINE.md: what of dockerd / containerd / containerd-shim PDN actually needs for v0.3.0

Scope: dissect the three Go trees on this device. Answer per component whether PDN needs it to compete
with Termux's `proot-distro` on the v0.3.0 mobility polygon. See `docs/src/roadmap.md` §10. The axis for
this pair is **CAPABILITY**. A real container engine needs what a translated root path cannot provide. A
translated root path rewrites paths with `ptrace`, that is PRoot. Read every verdict below against that
axis, not against Docker.

Markers, same convention as `workbench/NEXT_DEV.md`:

| marker | meaning |
|---|---|
| **R** | read from source in this tree; `file:line` cited |
| **M** | measured on this device (command cited) |
| **K** | inherited platform knowledge, not verifiable from these trees |
| **UNVERIFIED** | checked and could not be settled; names what would settle it |

Source trees: READ-ONLY on this device at `/data/data/com.termux/files/home/.hermes/cache/scratch/src/`

| tree | form | scale (M) |
|---|---|---|
| `moby-src` | dockerd, docker-v29.9.3 | 1,574 non-vendor Go files |
| `containerd-src` | containerd v2.4.2, **pruned** | 1,979 Go files / 531,855 lines |
| `runc-src` | runc v1.5.2 | 930 Go files / 346,450 lines (incl. vendor) |

**Pruning matters for this read (R).** `containerd-src` has no root `go.mod`. It has no `pkg/`
directory. It has no `plugins/services/`. It has no `version/`. It has only
`plugins/server/internal/{serve,listen_unix,listen_windows}.go`. The command `ls plugins` returns
`server` only. **The daemon-side service plugins are absent.** Present are the `core/` machinery, the
`api/` protobuf contracts, and the client/shim side. Claims about *server* plugin internals are
therefore marked UNVERIFIED below. One example is the local content-store path template.

---

## 0. The competitive test, stated once

`proot-distro` is installed at version 5.10.3. It has 77 files and 29,015 lines of Python and shell.
This figure is measured on this device. The `find` and `wc` commands give the figure. `NEXT_DEV.md` §1
records it. It has **no daemon, no shim, no content-addressed store, no namespaces, no cgroups and no
OCI artifacts**. It shells out to `proot`. It unpacks a rootfs tarball into a directory. That is the
entire bar that §10 asks PDN to meet.

Therefore the default verdict for any dockerd/containerd *engine* component is:

> **PDN does not need it for v0.3.0.** You cannot lose to `proot-distro` on a mechanism `proot-distro`
> does not possess.

Capability is the exception worth a loud statement. Neither `proot-distro` **nor** PDN's PRoot path can
provide kernel-enforced isolation of PID, net, mount and memory. See §5. PDN lacks that isolation. PDN
loses nothing against `proot-distro` because of that lack. PDN loses only against **Docker**. Docker is
a different competitor. Docker uses a different backend, UML, per `roadmap.md` §4.1. Everything below is
graded against `proot-distro`.

The one class that survives this test is the **image/content store**. See §4. The v0.3.0 polygon already
holds §10.3 and §4.4. §10.3 covers `export`, import, push and pull. This class survives *not* because
`proot-distro` has it.

---

## 1. The daemon API surface: containerd's gRPC services

### What it is (R)

containerd exposes **16 gRPC services**, each a `service` block in `api/services/*/v1/*.proto`, each with
a generated `_grpc.pb.go`. Enumerated below with the `service` line:

| # | service | proto `file:line` | RPCs |
|---|---|---|---|
| 1 | `Containers` | `api/services/containers/v1/containers.proto:44` | Get, List, ListStream, Create, Update, Delete (6) |
| 2 | `Content` | `api/services/content/v1/content.proto:28` | Info, Update, List, Delete, Read, Status, ListStatuses, Write, Abort (9) |
| 3 | `Diff` | `api/services/diff/v1/diff.proto:29` | Apply, Diff (2) |
| 4 | `Events` | `api/services/events/v1/events.proto:27` | Publish, Forward, Subscribe (3) |
| 5 | `Images` | `api/services/images/v1/images.proto:39` | Get, List, Create, Update, Delete (5) |
| 6 | `Introspection` | `api/services/introspection/v1/introspection.proto:30` | Plugins, Server, PluginInfo (3) |
| 7 | `Leases` | `api/services/leases/v1/leases.proto:26` | Create, Delete, List, AddResource, DeleteResource, ListResources (6) |
| 8 | `Mounts` | `api/services/mounts/v1/mounts.proto:27` | Activate, Deactivate, Info, Update, List (5) |
| 9 | `Namespaces` | `api/services/namespaces/v1/namespace.proto:37` | Get, List, Create, Update, Delete (5) |
| 10 | `Store` + `Controller` (sandbox) | `api/services/sandbox/v1/sandbox.proto:40,88` | 5 + 9 |
| 11 | `Snapshots` | `api/services/snapshots/v1/snapshots.proto:29` | Prepare, View, Mounts, Commit, Remove, Stat, Update, List, Usage, Cleanup (10) |
| 12 | `Streaming` | `api/services/streaming/v1/streaming.proto:25` | Stream (1) |
| 13 | `Tasks` | `api/services/tasks/v1/tasks.proto:31` | Create…Wait (17) |
| 14 | `Transfer` | `api/services/transfer/v1/transfer.proto:26` | Transfer (1) |
| 15 | `Version` | `api/services/version/v1/version.proto:26` | Version (1) |
| 16 | `Events` (ttrpc) | `api/services/ttrpc/events` | append/subscribe |

### Which of them dockerd actually calls (R)

Run this command over `moby-src`: `grep -rhoE "\.(containerdClient|client)\.[A-Z][A-Za-z]+\(" daemon/`.
The counts as run follow.

```
9 client.SnapshotService(      7 client.LeasesService(     6 client.ContentStore(
4 client.Update(               4 client.Do(                3 containerdClient.ContentStore(
2 client.LoadContainer(        2 containerdClient.SnapshotService(  1 containerdClient.ImageService(
1 client.ImageService(        1 client.DiffService(       1 client.EventService(
1 client.IntrospectionService(1 client.Pull(              1 client.Import(  1 client.Export(
```

So the services dockerd touches are **Containers, Content, Diff, Events, Images, Introspection, Leases,
Namespaces, Snapshots, Tasks, Streaming, Transfer and Version**. It does **not** touch Mounts or the
sandbox `Store`/`Controller`. The client seam is dialled at `daemon/daemon.go:1057-1068`. That call is
`containerd.New(addr, WithDefaultNamespace(...), WithExtraDialOpts(...), WithTimeout(...))`.
`WithDefaultNamespace` installs the `Namespaces` interceptor. Namespace is not optional. It is baked
into every call.

### The seam that makes the rest make sense (R)

dockerd does **not** hold a container process. It drives containerd through a deliberately tiny
interface at `daemon/internal/libcontainerd/types/types.go:57-63`:

```go
type Client interface {
        Version(ctx) (containerd.Version, error)
        LoadContainer(ctx, containerID) (Container, error)
        NewContainer(ctx, containerID, spec, shim, runtimeOptions, opts...) (Container, error)
}
```

Below that interface is `Container` at `types.go:66`, with `NewTask`/`Task`/`AttachTask`/`Delete`. Then
`Task`/`Process` at `types.go:45-55`, with `Pid`/`Kill`/`Resize`/`Delete`. The concrete implementation
is `daemon/internal/libcontainerd/remote/client.go` for Linux. That file maps each call onto a
containerd client call. For example, `NewContainer` maps to
`containerd.WithRuntime(shim, runtimeOptions)` at `client.go:119-125`. `NewTask` maps to
`c8dCtr.NewTask(...)` at `client.go:216-223`.

### What it buys / PDN verdict

- **Axis:** capability, orchestration liveness, and stability.
- **PDN v0.3.0 need: no for all 16.** The competitive test gives the verdict. `proot-distro` has no daemon
  and no client/server split at all. It is one process. §10 wants a *thin client + daemon* at
  `roadmap.md` §5.1. §10 does not want a gRPC/ttrpc engine surface. PDN's own IPC is a unix socket with
  JSON-RPC/D-Bus and a direct-exec fallback at §5.1. No §10.1 to §10.4 acceptance test requires any of
  these 16 services.
- **If it ever mattered:** the minimum PDN-side engine for *a Docker-shaped client* is the subset
  dockerd actually calls. That subset is task lifecycle, that is Tasks Create, Start, Delete, Wait and
  Kill over the shim at §3. It is snapshots, that is a `Prepare`/`Mounts`/`Commit` triple, replaceable
  by "unpack layer to path". It is content at §4, that is blob read and write. It is images, that is
  name to digest. It is transfer, that is pull and push. It is leases, that is drop, at §4. That is
  about 6 of 16. The sandbox API, Mounts, Diff and Introspection are droppable outright.

### Acceptance test

Not required for v0.3.0. If a PDN daemon surface is ever built, the gate is clear. A stock `docker` CLI
cannot be the test. It needs the Docker Engine REST API, not containerd gRPC. The honest gate is
proot-distro parity, already covered by §10. Concretely, `podnext install`, `login`, `export` and
`import` must pass with the daemon down. That is the direct-exec fallback at `roadmap.md` §5.1. PDN must
*not* acquire a hard daemon dependency.

### Dead ends

- Do not reimplement the sandbox API at `sandbox.proto:40,88` and the `Mounts` service. dockerd itself
  does not call them. They exist for Kubernetes/CRI sandboxing, out of PDN scope at §6.
- Do not add namespace isolation *inside containerd's API sense*, that is a free `Namespaces`
  multitenancy layer. A single user on one phone has one namespace. The API namespace is not the kernel
  one and adds nothing.

---

## 2. containerd-shim: role and protocol

### What it is (R)

The shim is **containerd v2's per-container process supervisor**. Its wire contract is the `Task`
service in `api/runtime/task/v2/shim.proto:33`, with 17 RPCs. The shape is the same in v3 at
`api/runtime/task/v3/shim.proto:33`. The proto's own header states the role verbatim at
`shim.proto:29-32`:

> "Shim service is launched for each container and is responsible for owning the IO for the container
> and its additional processes. The shim is also the parent of each container and allows reattaching
> to the IO and receiving the exit status for the container processes."

The **17 RPCs** are `State, Create, Start, Delete, Pids, Pause, Resume, Checkpoint, Kill, Exec,
ResizePty, CloseIO, Update, Wait, Stats, Connect, Shutdown` at `shim.proto:34-50`.

**What the shim owns vs proxies (R):**
- **Owns:** the *parentship* of the container process. A containerd restart therefore does not kill the
  task. It owns the stdio pipes at `CreateTaskRequest.stdin/stdout/stderr`, `shim.proto:57-59`. It owns
  the exit status at `DeleteResponse.exit_status/exited_at`, `shim.proto:73-76`, and at `WaitResponse`,
  `shim.proto:171-174`. It owns detach and reattach through `Connect` at
  `ConnectResponse.shim_pid/task_pid`, `shim.proto:186-189`.
- **Proxies:** every RPC is a thin forward to the OCI runtime binary `runc` for act and state. The shim
  keeps its own copy for liveness.
- **Transport:** ttrpc, a protobuf-over-unix-socket RPC, with a gRPC variant. The client abstracts both.
  See `core/runtime/v2/bridge.go:57-77`, the `TaskServiceClient` interface. See also the
  `bridge.go:24-28` comment, "handles both GRPC and TTRPC servers".
- **Launched per container:** `core/runtime/v2/binary.go:54-80`, that is `binary.Start` → `command(...)`
  with `GRPCAddress`/`TTRPCAddress`/`SocketDir`. A containerd restart rediscovers it through
  `core/runtime/v2/shim_load.go:36-76`, that is `LoadExistingShims`, which walks
  `stateDir/<namespace>/…`. The manager registers as a plugin at `core/runtime/v2/shim_manager.go:58-80`.

### What it buys / PDN verdict

- **Axis:** stability. That means survival of a supervisor restart, IO reattach, and no zombies. This is
  the single most transferable idea. PDN already has its analogue.
- **PDN v0.3.0 need: no. The mechanism is already in PDN, and `proot-distro` lacks it.** The competitive
  test returns **no** for the shim *as a component*. `proot-distro` has no shim and no daemon to survive.
  The shim's *reason for existing* is a supervised process that owns IO and exit status. That process can
  be reattached without a new exec. §10.2 `--detach`+`attach` and the session registry deliver exactly
  that. See the landed `run --detach`+`attach`, roadmap §10.2. PDN's equivalent is the session file held
  open. See `NEXT_DEV.md` §4 and proot-distro `session.py`. A **file held open is a poorer shim**. A fd
  survives exec, but gives no attach RPC, no pty resize and no exit-status query.
- **Honest gap:** PDN can leave a shell running and reattach, so §10.2 passes. PDN cannot answer `State`,
  `Pids` or `Resize` for a detached session the way the shim does. That gap is **not** on the
  `proot-distro` bar. `proot-distro` has none of it either. The gap is not a v0.3.0 blocker.

### Acceptance test

§10.2 already carries the gate, "detach, then reattach to the same live shell". To *match the shim you
must add* the three RPCs `proot-distro` lacks. Those RPCs are `State` for liveness, `Pids` for the live
process list, and `Resize` for the pty size. Add them over the existing session socket. That is the test
of record if PDN ever wants a shim-shaped surface. The shim is **not** currently required.

### Dead ends

- Do not reproduce ttrpc/gRPC + fd-passing. The first-pass dissection's "ttrpc has zero source hits"
  claim was **refuted**. The real count is 1,168, all imports and generated stubs. Either way the **wire
  is not the value**. The value is the ownership contract, which PDN can express over its existing
  socket. Do not vendor a shim.
- Checkpoint and restore, the `Checkpoint` RPC at `shim.proto:41`, depends on CRIU and a writable
  `/proc`. It is a permanent non-goal under a shell UID.

---

## 3. Image / content store: the one class that survives the competitive test

### What it is (R)

`content.Store` is one interface that composes four sub-interfaces. Together they cover the whole blob
lifecycle. See `core/content/content.go:41-46`, where `Store = Manager + Provider + IngestManager +
Ingester`. `Info{Digest,Size,CreatedAt,UpdatedAt,Labels}` is at `core/content/content.go:90-96`. A
`Writer` commits content-addressed data with `Commit(ctx,size,expected)`. It aborts if the content
already exists, at `content.go:146-166`.

- **Image record = name → descriptor, NOT the manifest.** See `core/images/image.go:35-56`:
  `Image{Name, Labels, Target ocispec.Descriptor}`, and `Target` is "typically a manifest, index or
  manifest list". Resolution walks content at read time, `image.go:107` Config, `:115` RootFS, `:153`
  Manifest. No derived data is stored. The record is a bolt bucket and a `target` sub-bucket. See
  `core/metadata/images.go:379` read, `:419` write, `:347,361` validate → `Target.Digest.Validate()`.
- **GC = mark-and-sweep over a label graph.** See the resource types at `core/metadata/gc.go:35-58`. The
  roots are at `gc.go:67`. `DB.GarbageCollect` is at `core/metadata/db.go:383`. Unmarked nodes are
  removed at `db.go:426`.
- **Leases = GC protection.** See `core/leases/lease.go:32-54`. `AddResource` is at
  `core/metadata/leases.go:186`. Content is leased the moment a writer references an existing digest, at
  `core/metadata/content.go:400-407`.
- **Registry HEAD-skip**, the feature §4.4 wants. See `core/remotes/docker/pusher.go:118-126`. Manifests
  probe `getManifestPath`. Blobs probe `["blobs", digest]`. Then a `HEAD` follows. A `200 OK` gives
  `PushStatus{Exists}` and a return of `ErrAlreadyExists`, at `pusher.go:140-164`. A manifest `PUT`
  follows with the descriptor media type, at `pusher.go:186-192`. A blob does `POST blobs/uploads/` then
  `PUT …?digest=`, at `pusher.go:195-302`. Pull is `HEAD`→`GET` at
  `core/remotes/docker/fetcher.go:376-400`.
- **OCI image-layout import.** `core/images/archive/importer.go:108,137` reads `oci-layout` +
  `index.json`. `onUntarBlob` at `importer.go:248` computes the digest *as it streams*, with `digester` +
  `TeeReader`. It then verifies the committed blob against its declared digest.

### The subset PDN's OCI store item (`roadmap.md` §4.4, "next major version") needs

§4.4 already specifies the design. Images become OCI artifacts: a manifest, a config and layer blobs,
all digest-addressed. A layer cache holds one file per blob. Each file name is the digest of its
content. A digest-grammar validator checks it. Against the source above, the mapping is:

| # | Mechanism | source | PDN v0.3.0 (§4.4) |
|---|---|---|---|
| 1 | digest-addressed blobs + size index | `core/content/content.go:41,90` | **keep, minimal** — one blob file per digest `<root>/blobs/<alg>/<enc[:2]>/<enc>` + `{digest→size}` |
| 2 | image = name→root descriptor | `core/images/image.go:35` | **keep** — do not store config/layers; walk the manifest |
| 3 | GC + leases | `core/leases/lease.go:32`; `core/metadata/gc.go:35` | **DROP leases; refcount-by-manifest** — leases exist only to protect blobs while a concurrent writer is mid-pull (`metadata/content.go:400-407`); PDN is single-writer |
| 4 | registry HEAD-skip + manifest PUT | `core/remotes/docker/pusher.go:118-192` | **keep — this *is* the §4.4 push feature** |
| 5 | digest grammar validation | call sites `core/content/helpers.go:73`, `core/metadata/images.go:364`; grammar in `opencontainers/go-digest` (**not vendored — UNVERIFIED**) | **keep, mandatory** — validate before the digest enters a path (traversal safety, §4.4's own "digest-grammar validator") |
| 6 | OCI image-layout import | `core/images/archive/importer.go:108,248` | **keep import** (§10.1 archive install); export is §10.3, can follow |

### What it buys / PDN verdict

- **Axis:** mobility, the §10.1/§10.3 round-trip, plus stability and blob integrity.
- **PDN v0.3.0 need: yes for items 1, 2, 4, 5 and 6. No for item 3.** This is the **only** class where
  the competitive test is partially "yes". It is "yes" because §10.1/§10.3 place export/import/push/pull
  *inside* the v0.3.0 polygon. It is **not** "yes" because `proot-distro` has an image store.
  `proot-distro` has none. It unpacks a tarball. Stated plainly, PDN does **not** lose to `proot-distro`
  on the content store. PDN loses to §10.3 against the *roadmap's own acceptance table*. The store is a
  §10 requirement, not a `proot-distro` requirement.
- **Cost, already measured in §4.4 (M):** snapshot and diff take 0.1 to 0.25 s/build. The tar-at-push
  alternative takes ≈0.35 s raw or ≈2.1 s gzip per 307 MB. That alternative yields one opaque layer.

### Acceptance test

§10.3's table gives the sequence. First `podnext export <c> ./c.oci.tar`, then `install` it on another
device. Then `build local` → `push` → `pull` on a second machine. The store-level gate underneath it is
this. **A blob written under a crafted digest cannot escape the cache directory**, the stated trust
boundary of §4.4. Also, **an import with a flipped bit in any blob is refused**, the
verify-against-declared-digest behaviour of `importer.go:248`.

### Dead ends

- The containerd GC label graph at `core/metadata/gc.go:35-119`, with ref/expire/flat labels and
  back-refs, and the `leases` service, are **overkill** for one writer. Replace them with "a blob is live
  iff some stored image's manifest tree names its digest".
- Do not use boltdb/`core/metadata` as a store. PDN already plans an embedded `state.db` at §5.2. A
  second DB for content is a duplicate.
- Do not use erofs, dmverity or fsverity, which are present in `internal/`. They are kernel features,
  dead under a shell UID.

---

## 4. Namespaces and cgroups at the syscall level, the CAPABILITY axis

Here "a real container engine" and "a translated root path" actually diverge. The text separates
source-provable requirements (R) from the Android failure mode (K), which is inherited platform
knowledge.

### 4.1 Namespaces (R)

`runc-src/libcontainer/configs/namespaces_syscall.go:11-20` maps type→flag. `CloneFlags()` ORs them, at
`:24-33`:

| ns | flag | line |
|---|---|---|
| NET | `CLONE_NEWNET` | `:12` |
| MNT | `CLONE_NEWNS` | `:13` |
| USER | `CLONE_NEWUSER` | `:14` |
| IPC | `CLONE_NEWIPC` | `:15` |
| UTS | `CLONE_NEWUTS` | `:16` |
| PID | `CLONE_NEWPID` | `:17` |
| CGROUP | `CLONE_NEWCGROUP` | `:18` |
| TIME | `CLONE_NEWTIME` | `:19` |

`clone(2)` applies them, at `runc-src/libcontainer/nsenter/nsexec.c:322`, with flags from
`CLONE_FLAGS_ATTR` at `nsexec.c:103,408`. `unshare(2)` also applies them, at `nsexec.c:675-691`, and for
a user namespace at `nsexec.c:1054-1055`. The Go side joins with `unix.Unshare(CLONE_FS)` at
`libcontainer/process_linux.go:705` and `unix.Setns(...CLONE_NEWNS)` at `process_linux.go:718`.
**Rootless is forced when `os.Geteuid() != 0` at `runc-src/rootless_linux.go:24`, package `main`, and
rootless runc *requires* `CLONE_NEWUSER`.** Note that the first-pass citation
`libcontainer/rootless_linux.go:24` is wrong. The file is at the tree root. See
`libcontainer/configs/config.go:220-228`, which says "When RootlessEUID is set, runc creates a new
userns for the container".

### 4.2 mount(2) (R)

Default OCI mounts live in `runc-src/libcontainer/specconv/example.go`. They are `/proc` procfs at `:61`,
`/dev` tmpfs at `:67`, `/dev/pts` devpts at `:73`, `/dev/shm` tmpfs at `:79`, `/dev/mqueue` mqueue at
`:85`, `/sys` sysfs ro at `:91`, and `/sys/fs/cgroup` cgroup ro at `:97`. `mountToRootfs` dispatches
them at `rootfs_linux.go:610`, with cases at `:621` proc/sysfs, `:660` mqueue, `:667` tmpfs and `:783`
cgroup. The rootfs swap is `unix.PivotRoot(\".\", \".\")` at `rootfs_linux.go:1164`. A `chroot` fallback
is at `:239`. `unix.Mount(\"\", path, \"\", flags, \"\")` handles propagation at `:1089`. **Every one
needs `CAP_SYS_ADMIN` in the owning user namespace.**

### 4.3 cgroups (R)

cgroupfs is `/sys/fs/cgroup` at `runc-src/vendor/github.com/opencontainers/cgroups/utils.go:23`. Access
goes through `openat2` at `.../cgroups/file.go:110-112`. The cgroup v2 files written are
**`cgroup.subtree_control`** at `.../cgroups/fs2/create.go:83,143`, **`cgroup.procs`** at
`utils.go:380-410`, **`memory.max`** at `fs2/memory.go:64`, **`cpu.max`** at `fs2/cpu.go:66`, and
**`pids.max`** at `fs2/pids.go:34`. The cgroup v1 files are `memory.limit_in_bytes` at
`fs/memory.go:21,46` and `cpu.cfs_quota_us` at `fs/cpu.go:132`. **Rootless runc concedes it cannot do
this.** See `specconv/spec_linux.go:832`, which says "In rootless containers, any attempt to make cgroup
changes is likely to fail… ignores the error". See `config.go:226-228`, `RootlessCgroups ⇒ cgroups
errors are ignored`.

### 4.4 What is IMPOSSIBLE under an Android shell UID

Requirements below are **R**, source-provable. Each failure is **K**, the Android platform, inherited:

| requirement | source (R) | failure under shell UID (K) |
|---|---|---|
| `unshare(CLONE_NEWUSER)` | `nsexec.c:1054`; needed by rootless `config.go:222` | `EINVAL` — unprivileged userns disabled on Android GKI |
| `unshare(CLONE_NEWNS/PID/NET/UTS/IPC/CGROUP/TIME)` | `namespaces_syscall.go:12-19` | `EPERM` — `CapEff=0` |
| `mount(2)`, `pivot_root(2)`, `chroot(2)` | `rootfs_linux.go:1164,239,1089` | `mount(2)` → `SIGSYS` (Bionic seccomp); `chroot` → `EPERM` |
| cgroup writes | `fs2/create.go:143`, `fs2/memory.go:64` | `/sys/fs/cgroup` read-only; enabling writes blocked |

**UNVERIFIED:** whether the ro cgroup mount alone blocks an already-per-UID-writable `cgroup.procs`, or
only the enabling writes. One test settles it. Run `echo $$ > /sys/fs/cgroup/.../cgroup.procs` as the
shell UID on this device.

### 4.5 Consequence: what a translated root path still CANNOT provide, R for the requirement

PRoot avoids all of §4.1 to 4.3. It never issues `unshare`/`mount`/`pivot_root`. It rewrites paths under
`ptrace` instead. What it structurally cannot fake follows:

- **Real PID isolation.** There is no `CLONE_NEWPID` at `namespaces_syscall.go:17`, so `/proc` is host
  `/proc`. PIDs are rewritten for display, not renumbered. There is no PID-1, no `waitpid` reaping and no
  `NSpid`.
- **Real network isolation.** There is no `CLONE_NEWNET` at `:12`, so there is no per-container iface or
  IP. Host ports collide. This is why §10.2 needs `--fix-low-ports` and `--redirect-ports` as
  *workarounds*.
- **Resource limits.** There are no cgroup writes at `fs2/*.go`, so there is no `memory.max`, `cpu.max`
  or `pids.max`, and no OOM scoping. PDN can enforce quotas only at the `ptrace`/supervisor layer, on a
  best-effort basis.
- **Mount-namespace semantics.** There is no `CLONE_NEWNS`+`pivot_root`, so host mounts stay visible.
  Bind and ro semantics are emulated, not enforced.

**Verdict:** axis = **capability**. `proot-distro` has none of this either, so this is **not** a v0.3.0
blocker against `proot-distro`. It is the reason a *full-fidelity* mode exists only behind the UML
backend. See `roadmap.md` §4.1, `UmlBackend`. PDN's v0.3.0 does not need namespaces or cgroups.

**Acceptance test, if a full-fidelity claim is ever made:** run `unshare --user --pid --mount` as the
shell UID and show the errno. A claim of isolation without a UML backend is false. This probe shows it.
For v0.3.0 the acceptance is the **negative** one. PDN's `login` must **not** advertise isolation it
cannot enforce.

### Dead ends

- Do not attempt namespaces or cgroups under a shell UID at all, per §4.4. The result is `SIGSYS`/`EPERM`,
  not a policy choice.
- Rootless Docker recipes, `rootlesskit`, `slirp4netns` and `newuidmap`, are moot once `CLONE_NEWUSER`
  returns `EINVAL`. dockerd documents the same wall at `daemon/internal/rootless/rootless_linux.go:52-55`.
  A setns back to the host netns "fails with EPERM in rootless mode … the daemon lacks CAP_SYS_ADMIN".

---

## 5. Deliberately out of scope for PDN, with the reason

| out of scope | source anchor | reason |
|---|---|---|
| Namespaces (all 8) | `namespaces_syscall.go:11-20` | impossible under shell UID (§4.4); UML backend owns it (§4.1) |
| cgroups v1/v2 | `fs2/create.go:83`, `fs/cpu.go:132` | `/sys/fs/cgroup` ro; rootless already ignores (§4.3) |
| mount/pivot_root/chroot rootfs build | `rootfs_linux.go:1164` | `mount(2)` → `SIGSYS`; PDN translates paths instead |
| containerd shim (ttrpc/gRPC, fd-passing) | `api/runtime/task/v2/shim.proto:33` | `proot-distro` has none; ownership value already in PDN's session file (§2) — wire is not the value |
| containerd daemon service plugins (tasks/events/content servers) | `plugins/` empty in tree; `core/runtime/v2` is client-side | PDN is a client+daemon of its own shape (§5.1), not a containerd plugin host |
| Leases + label-graph GC | `core/leases/lease.go:32`; `core/metadata/gc.go:35` | designed for concurrent multi-writer GC; PDN is single-writer → refcount (§3) |
| CRI / sandbox API / Mounts service | `api/services/sandbox/v1/sandbox.proto:40,88`; `mounts.proto:27` | Kubernetes/CRI surface; dockerd itself does not call them (§1) |
| Checkpoint/restore (CRIU) | `core/runtime/v2/shim.go:68-71` (`rootFsDiffTar`), `Checkpoint` RPC | needs CRIU + writable `/proc`; permanent non-goal under shell UID |
| Snapshotters / overlayfs / erofs / dmverity / fsverity | `core/snapshots/`, `internal/{dmverity,erofsutils,fsverity}` | `mount(2)`-based copy-on-write; impossible without mounts; PDN's rootfs is a plain directory |
| iptables / bridge networking | `moby-src/daemon/libnetwork/iptables/iptables.go`; `rootless_linux.go:52-55` | needs `CAP_NET_ADMIN` + netns; §10.2 ports are emulated via PDN's redirect, not netfilter |
| Docker Registry auth-scope machinery, cross-repo mount, chunked upload | `core/remotes/docker/pusher.go:195-302` | only the HEAD-skip + single-PUT is in §4.4; the rest is registry-scale |

---

## 6. One-line verdicts, index

| component | axis | PDN v0.3.0? | competitive test (vs proot-distro) |
|---|---|---|---|
| containerd gRPC services (16) | capability/stability | **no** | proot-distro has no daemon |
| dockerd↔containerd seam (3-method) | capability | **no** | no daemon to seam to |
| containerd-shim (Task/task proto) | stability | **no** (value already in PDN) | proot-distro has no shim |
| content store (blobs+Info) | mobility/stability | **yes** (§4.4) | §10.3 requirement, not a proot-distro one |
| image record (name→digest) | mobility | **yes** (§4.4) | §10.3 requirement |
| GC + leases | stability | **no** | single-writer → refcount |
| registry HEAD-skip + PUT | mobility | **yes** (§4.4/§10.3) | §10.3 requirement |
| digest grammar validation | stability/security | **yes — mandatory** | path-traversal safety, §4.4 |
| OCI layout import | mobility | **yes** (§10.1) | §10.1 archive install |
| namespaces (8) | capability | **no** | impossible + proot-distro lacks |
| cgroups v1/v2 | capability/optimization | **no** | impossible + proot-distro lacks |
| mount/pivot_root rootfs | capability | **no** | `SIGSYS`; path translation instead |

**Bottom line.** Measured against `proot-distro`, PDN needs **none** of the engine machinery. That
machinery is the daemon API, the seam, the shim, namespaces, cgroups and mounts. `proot-distro` has none
of it, and PDN already beats or matches it there. The **only** dockerd/containerd material PDN needs for
v0.3.0 is a small subset of the **image/content store**. That subset is blobs, image-as-pointer,
registry HEAD-skip, digest validation and OCI-layout import. PDN needs that subset because §10.1/§10.3
put it in the polygon, **not** because a competitor has it. The capability axis, namespaces, cgroups and
mounts, is a wall under a shell UID. It belongs to the UML backend, exactly as `roadmap.md` §1.2 and
§4.1 already state. The honest v0.3.0 obligation there is to **avoid claiming** isolation the PRoot path
cannot deliver.
