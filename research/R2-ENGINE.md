# R2-ENGINE.md — what of dockerd / containerd / containerd-shim PDN actually needs for v0.3.0

Scope: dissect the three Go trees on this device and answer, per component, **whether PDN needs it to
compete with Termux's `proot-distro` on the v0.3.0 mobility polygon** (`docs/src/roadmap.md` §10). The
axis this pair turns on is **CAPABILITY** — what a real container engine needs that a translated root
path (`ptrace` path rewriting, i.e. PRoot) structurally cannot provide — so every verdict below is read
against that axis, not against Docker.

Markers (same convention as `workbench/NEXT_DEV.md`):

| marker | meaning |
|---|---|
| **R** | read from source in this tree; `file:line` cited |
| **M** | measured on this device (command cited) |
| **K** | inherited platform knowledge, not verifiable from these trees |
| **UNVERIFIED** | checked and could not be settled; names what would settle it |

Source trees (READ-ONLY on this device, `/data/data/com.termux/files/home/.hermes/cache/scratch/src/`):

| tree | form | scale (M) |
|---|---|---|
| `moby-src` | dockerd, docker-v29.9.3 | 1,574 non-vendor Go files |
| `containerd-src` | containerd v2.4.2, **pruned** | 1,979 Go files / 531,855 lines |
| `runc-src` | runc v1.5.2 | 930 Go files / 346,450 lines (incl. vendor) |

**Pruning matters for this read (R).** `containerd-src` has no root `go.mod`, no `pkg/` directory, no
`plugins/services/`, no `version/` — only `plugins/server/internal/{serve,listen_unix,listen_windows}.go`
(`ls plugins` = `server` only). The **daemon-side service plugins are absent**; what is present is the
`core/` machinery, the `api/` protobuf contracts, and the client/shim side. Claims about *server*
plugin internals (e.g. the local content-store path template) are therefore marked UNVERIFIED below.

---

## 0. The competitive test, stated once

`proot-distro` (installed, 5.10.3) is **77 files / 29,015 lines of Python + shell** (M, `find`/`wc`,
recorded in `NEXT_DEV.md` §1). It has: **no daemon, no shim, no content-addressed store, no
namespaces, no cgroups, no OCI artifacts** — it shells out to `proot` and unpacks a rootfs tarball
into a directory. That is the entire bar §10 asks PDN to meet.

Therefore the default verdict for any dockerd/containerd *engine* component is:

> **PDN does not need it for v0.3.0.** You cannot lose to `proot-distro` on a mechanism `proot-distro`
> does not possess.

Capability is the exception worth stating loudly: neither `proot-distro` **nor** PDN's PRoot path can
provide kernel-enforced PID/net/mount/memory isolation (§5). PDN does not lose on capability *against
`proot-distro`* by lacking it — it loses only against **Docker**, which is a different competitor and a
different (UML) backend per `roadmap.md` §4.1. Everything below is graded against `proot-distro`.

The one class that survives this test is the **image/content store** (§4), because §10.3 (`export` /
import / push / pull) and §4.4 are already in the v0.3.0 polygon — *not* because `proot-distro` has it.

---

## 1. The daemon API surface — containerd's gRPC services

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

`grep -rhoE "\.(containerdClient|client)\.[A-Z][A-Za-z]+\(" daemon/` over `moby-src` (counts as run):

```
9 client.SnapshotService(      7 client.LeasesService(     6 client.ContentStore(
4 client.Update(               4 client.Do(                3 containerdClient.ContentStore(
2 client.LoadContainer(        2 containerdClient.SnapshotService(  1 containerdClient.ImageService(
1 client.ImageService(        1 client.DiffService(       1 client.EventService(
1 client.IntrospectionService(1 client.Pull(              1 client.Import(  1 client.Export(
```

So the services dockerd touches are: **Containers, Content, Diff, Events, Images, Introspection,
Leases, Namespaces, Snapshots, Tasks, Streaming, Transfer, Version**. It does **not** touch Mounts or
the sandbox `Store`/`Controller`. The client seam is dialled at `daemon/daemon.go:1057-1068`
(`containerd.New(addr, WithDefaultNamespace(...), WithExtraDialOpts(...), WithTimeout(...))`), where
`WithDefaultNamespace` is what installs the `Namespaces` interceptor — namespace is not optional, it is
baked into every call.

### The seam that makes the rest make sense (R)

dockerd does **not** hold a container process. It drives containerd through a deliberately tiny
interface — `daemon/internal/libcontainerd/types/types.go:57-63`:

```go
type Client interface {
        Version(ctx) (containerd.Version, error)
        LoadContainer(ctx, containerID) (Container, error)
        NewContainer(ctx, containerID, spec, shim, runtimeOptions, opts...) (Container, error)
}
```

Below that, `Container` (`types.go:66`, `NewTask`/`Task`/`AttachTask`/`Delete`) and `Task`/`Process`
(`types.go:45-55`, `Pid`/`Kill`/`Resize`/`Delete`). The concrete implementation is
`daemon/internal/libcontainerd/remote/client.go` (Linux), which maps each call onto a containerd client
call — e.g. `NewContainer` → `containerd.WithRuntime(shim, runtimeOptions)` (`client.go:119-125`),
`NewTask` → `c8dCtr.NewTask(...)` (`client.go:216-223`).

### What it buys / PDN verdict

- **Axis:** capability (orchestration liveness), stability.
- **PDN v0.3.0 need: no (all 16).** Verdict by the competitive test: `proot-distro` has no daemon and
  no client/server split at all — it is one process. §10 wants a *thin client + daemon*
  (`roadmap.md` §5.1) but not a gRPC/ttrpc engine surface; PDN's own IPC is a unix socket with
  JSON-RPC/D-Bus and a direct-exec fallback (§5.1). None of these 16 services is required to pass any
  §10.1–§10.4 acceptance test.
- **If it ever mattered:** the minimum PDN-side engine answering *a Docker-shaped client* would be the
  subset dockerd actually calls — task lifecycle (Tasks/Create,Start,Delete,Wait,Kill over the shim,
  §3), snapshots (a `Prepare`/`Mounts`/`Commit` triple, replaceable by "unpack layer → path"), content
  (§4, blob read/write), images (name→digest), transfer (pull/push), leases (drop, §4). That is ~6 of
  16; the sandbox API, Mounts, Diff and Introspection are droppable outright.

### Acceptance test

Not required for v0.3.0. If a PDN daemon surface is ever built, the gate is: a stock `docker` CLI
cannot be the test (it needs the Docker Engine REST API, not containerd gRPC); the honest gate is
proot-distro parity, already covered by §10. Concretely: **`podnext install`/`login`/`export`/`import`
pass with the daemon down** (direct-exec fallback, `roadmap.md` §5.1) — PDN must *not* acquire a hard
daemon dependency.

### Dead ends

- Reimplementing the sandbox API (`sandbox.proto:40,88`) and `Mounts` service — dockerd itself does not
  call them; they exist for Kubernetes/CRI sandboxing, out of PDN scope (§6).
- Namespace isolation *inside containerd's API sense* (a free `Namespaces` multitenancy layer) — a single
  user on one phone has one namespace; the API namespace is not the kernel one and adds nothing.

---

## 2. containerd-shim — role and protocol

### What it is (R)

The shim is **containerd v2's per-container process supervisor**. Its wire contract is the `Task`
service in `api/runtime/task/v2/shim.proto:33` (17 RPCs) — same shape in v3 at
`api/runtime/task/v3/shim.proto:33`. The proto's own header states the role verbatim
(`shim.proto:29-32`):

> "Shim service is launched for each container and is responsible for owning the IO for the container
> and its additional processes. The shim is also the parent of each container and allows reattaching
> to the IO and receiving the exit status for the container processes."

The **17 RPCs**: `State, Create, Start, Delete, Pids, Pause, Resume, Checkpoint, Kill, Exec, ResizePty,
CloseIO, Update, Wait, Stats, Connect, Shutdown` (`shim.proto:34-50`).

**What the shim owns vs proxies (R):**
- **Owns:** the container process's *parentship* (so a containerd restart does not kill the task), the
  stdio pipes (`CreateTaskRequest.stdin/stdout/stderr`, `shim.proto:57-59`), the exit status
  (`DeleteResponse.exit_status/exited_at`, `shim.proto:73-76`; `WaitResponse`, `shim.proto:171-174`),
  and detach/reattach via `Connect` (`ConnectResponse.shim_pid/task_pid`, `shim.proto:186-189`).
- **Proxies:** every RPC is a thin forward to the OCI runtime binary (`runc`) for act/state, while the
  shim keeps its own copy for liveness.
- **Transport:** ttrpc (a protobuf-over-unix-socket RPC), with a gRPC variant — the client abstracts
  both (`core/runtime/v2/bridge.go:57-77`, `TaskServiceClient` interface; `bridge.go:24-28` comment:
  "handles both GRPC and TTRPC servers").
- **Launched per container:** `core/runtime/v2/binary.go:54-80` (`binary.Start` → `command(...)` with
  `GRPCAddress`/`TTRPCAddress`/`SocketDir`), and re-discovered after a containerd restart by
  `core/runtime/v2/shim_load.go:36-76` (`LoadExistingShims`, walking `stateDir/<namespace>/…`). The
  manager registers as a plugin at `core/runtime/v2/shim_manager.go:58-80`.

### What it buys / PDN verdict

- **Axis:** stability (survive supervisor restart, reattach IO, no zombies) — the single most
  transferable idea, and PDN already has its analogue.
- **PDN v0.3.0 need: no — the mechanism is already in PDN, and `proot-distro` lacks it.** The
  competitive test returns **no** for the shim *as a component*: `proot-distro` has no shim and no
  daemon to survive. But the shim's *reason for existing* — a supervised process that owns IO and exit
  status and can be reattached without re-exec — is precisely what §10.2 `--detach`+`attach` and the
  session registry deliver (landed as `run --detach`+`attach`, roadmap §10.2). PDN's equivalent is the
  session file held open (`NEXT_DEV.md` §4, proot-distro `session.py`) — a **file held open is a
  poorer shim** (a fd survives exec but gives no attach RPC, no pty resize, no exit-status query).
- **Honest gap:** PDN can leave a shell running and reattach (§10.2 passing), but cannot answer `State`,
  `Pids`, or `Resize` for a detached session the way the shim does. That gap is **not** on the
  `proot-distro` bar (it has none of it either), so it is not a v0.3.0 blocker.

### Acceptance test

§10.2 already carries the gate: "detach, then reattach to the same live shell". To *match the shim you
would have to add* the three RPCs `proot-distro` lacks — `State` (is it alive), `Pids` (what is still
running), `Resize` (pty size) — over the existing session socket. That is the test of record if PDN ever
wants a shim-shaped surface; it is **not** currently required.

### Dead ends

- Reproducing ttrpc/gRPC + fd-passing. The first-pass dissection's "ttrpc has zero source hits" claim was
  **refuted** (it has 1,168, all imports/generated stubs); either way the **wire is not the value** — the
  value is the ownership contract, which PDN can express over its existing socket. Do not vendor a shim.
- Checkpoint/restore (`Checkpoint`, `shim.proto:41`) depends on CRIU + a writable `/proc` and is a
  permanent non-goal under a shell UID.

---

## 3. Image / content store — the one class that survives the competitive test

### What it is (R)

`content.Store` is one interface composing four sub-interfaces — the whole blob lifecycle
(`core/content/content.go:41-46`: `Store = Manager + Provider + IngestManager + Ingester`).
`Info{Digest,Size,CreatedAt,UpdatedAt,Labels}` at `core/content/content.go:90-96`. A `Writer` commits
content-addressed data with `Commit(ctx,size,expected)`, aborting if the content already exists
(`content.go:146-166`).

- **Image record = name → descriptor, NOT the manifest** (`core/images/image.go:35-56`:
  `Image{Name, Labels, Target ocispec.Descriptor}`, `Target` "typically a manifest, index or manifest
  list"). Resolution walks content at read time (`image.go:107` Config, `:115` RootFS, `:153` Manifest),
  nothing derived is stored. The record is a bolt bucket + a `target` sub-bucket
  (`core/metadata/images.go:379` read, `:419` write, `:347,361` validate → `Target.Digest.Validate()`).
- **GC = mark-and-sweep over a label graph** (`core/metadata/gc.go:35-58` resource types; roots
  `gc.go:67`; `DB.GarbageCollect` `core/metadata/db.go:383`; unmarked nodes removed `db.go:426`).
- **Leases = GC protection** (`core/leases/lease.go:32-54`; `AddResource` `core/metadata/leases.go:186`;
  content leased the moment a writer references an existing digest — `core/metadata/content.go:400-407`).
- **Registry HEAD-skip (the feature §4.4 wants)** — `core/remotes/docker/pusher.go:118-126`: manifests
  probe `getManifestPath`, blobs probe `["blobs", digest]`, then `HEAD`; `200 OK` ⇒ `PushStatus{Exists}`
  + return `ErrAlreadyExists` (`pusher.go:140-164`); manifest `PUT` with the descriptor media type
  (`pusher.go:186-192`); blob `POST blobs/uploads/` then `PUT …?digest=` (`pusher.go:195-302`). Pull is
  `HEAD`→`GET` (`core/remotes/docker/fetcher.go:376-400`).
- **OCI image-layout import** — `core/images/archive/importer.go:108,137` reads `oci-layout` +
  `index.json`; `onUntarBlob` (`importer.go:248`) computes the digest *as it streams* (`digester` +
  `TeeReader`) and verifies the committed blob against its declared digest.

### The subset PDN's OCI store item (`roadmap.md` §4.4, "next major version") needs

§4.4 already specifies the design ("Images become OCI artifacts — manifest + config + layer blobs,
digest-addressed… Layer cache — one file per blob named by the digest of its content, behind a
digest-grammar validator…"). Against the source above, the mapping is:

| # | Mechanism | source | PDN v0.3.0 (§4.4) |
|---|---|---|---|
| 1 | digest-addressed blobs + size index | `core/content/content.go:41,90` | **keep, minimal** — one blob file per digest `<root>/blobs/<alg>/<enc[:2]>/<enc>` + `{digest→size}` |
| 2 | image = name→root descriptor | `core/images/image.go:35` | **keep** — do not store config/layers; walk the manifest |
| 3 | GC + leases | `core/leases/lease.go:32`; `core/metadata/gc.go:35` | **DROP leases; refcount-by-manifest** — leases exist only to protect blobs while a concurrent writer is mid-pull (`metadata/content.go:400-407`); PDN is single-writer |
| 4 | registry HEAD-skip + manifest PUT | `core/remotes/docker/pusher.go:118-192` | **keep — this *is* the §4.4 push feature** |
| 5 | digest grammar validation | call sites `core/content/helpers.go:73`, `core/metadata/images.go:364`; grammar in `opencontainers/go-digest` (**not vendored — UNVERIFIED**) | **keep, mandatory** — validate before the digest enters a path (traversal safety, §4.4's own "digest-grammar validator") |
| 6 | OCI image-layout import | `core/images/archive/importer.go:108,248` | **keep import** (§10.1 archive install); export is §10.3, can follow |

### What it buys / PDN verdict

- **Axis:** mobility (the §10.1/§10.3 round-trip) + stability (blob integrity).
- **PDN v0.3.0 need: yes for 1,2,4,5,6; no for 3.** This is the **only** class where the competitive
  test is partially "yes" — and it is "yes" because §10.1/§10.3 place export/import/push/pull *inside*
  the v0.3.0 polygon, **not** because `proot-distro` has an image store (it has none; it unpacks a
  tarball). Stated plainly: PDN does **not** lose to `proot-distro` on the content store; it loses to
  §10.3 against the *roadmap's own acceptance table*. The store is a §10 requirement, not a
  `proot-distro` requirement.
- **Cost, already measured in §4.4 (M):** snapshot/diff 0.1–0.25 s/build; the tar-at-push alternative
  ≈0.35 s raw / ≈2.1 s gzip per 307 MB and yields one opaque layer.

### Acceptance test

§10.3's table: `podnext export <c> ./c.oci.tar`, then `install` it on another device; and
`build local` → `push` → `pull` on a second machine. The store-level gate underneath it:
**a blob written under a crafted digest cannot escape the cache directory** (§4.4's stated trust
boundary), and **an import with a flipped bit in any blob is refused** (`importer.go:248`'s
verify-against-declared-digest behaviour).

### Dead ends

- containerd's GC label graph (`core/metadata/gc.go:35-119`, ref/expire/flat labels, back-refs) and the
  `leases` service: **overkill** for one writer. Replace with "a blob is live iff some stored image's
  manifest tree names its digest".
- boltdb/`core/metadata` as a store: PDN already plans an embedded `state.db` (§5.2); a second DB for
  content is a duplicate.
- erofs/dmverity/fsverity (present in `internal/`) — kernel features, dead under a shell UID.

---

## 4. Namespaces and cgroups at the syscall level (the CAPABILITY axis)

This is where "a real container engine" and "a translated root path" actually diverge. Source-provable
requirements (R) are separated from the Android failure mode (K = inherited platform knowledge).

### 4.1 Namespaces (R)

`runc-src/libcontainer/configs/namespaces_syscall.go:11-20` maps type→flag, `CloneFlags()` ORs them
(`:24-33`):

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

Applied by `clone(2)` (`runc-src/libcontainer/nsenter/nsexec.c:322`, flags from `CLONE_FLAGS_ATTR`
`nsexec.c:103,408`) and `unshare(2)` (`nsexec.c:675-691`; user-ns `nsexec.c:1054-1055`). Go side joins
with `unix.Unshare(CLONE_FS)` (`libcontainer/process_linux.go:705`) and `unix.Setns(...CLONE_NEWNS)`
(`process_linux.go:718`). **Rootless is forced when `os.Geteuid() != 0` (`runc-src/rootless_linux.go:24`
— package `main`; note the first-pass citation `libcontainer/rootless_linux.go:24` is wrong, the file is
at the tree root), and rootless runc *requires* `CLONE_NEWUSER`** (`libcontainer/configs/config.go:220-228`:
"When RootlessEUID is set, runc creates a new userns for the container").

### 4.2 mount(2) (R)

Default OCI mounts (`runc-src/libcontainer/specconv/example.go`): `/proc` procfs `:61`; `/dev` tmpfs
`:67`; `/dev/pts` devpts `:73`; `/dev/shm` tmpfs `:79`; `/dev/mqueue` mqueue `:85`; `/sys` sysfs ro
`:91`; `/sys/fs/cgroup` cgroup ro `:97`. Dispatched by `mountToRootfs` (`rootfs_linux.go:610`, cases
`:621` proc/sysfs, `:660` mqueue, `:667` tmpfs, `:783` cgroup). The rootfs swap is
`unix.PivotRoot(".", ".")` (`rootfs_linux.go:1164`), with a `chroot` fallback (`:239`) and
`unix.Mount("", path, "", flags, "")` for propagation (`:1089`). **Every one needs `CAP_SYS_ADMIN` in
the owning user namespace.**

### 4.3 cgroups (R)

cgroupfs is `/sys/fs/cgroup` (`runc-src/vendor/github.com/opencontainers/cgroups/utils.go:23`), accessed
through `openat2` (`.../cgroups/file.go:110-112`). cgroup v2 files written: **`cgroup.subtree_control`**
(`.../cgroups/fs2/create.go:83,143`), **`cgroup.procs`** (`utils.go:380-410`), **`memory.max`**
(`fs2/memory.go:64`), **`cpu.max`** (`fs2/cpu.go:66`), **`pids.max`** (`fs2/pids.go:34`). cgroup v1:
`memory.limit_in_bytes` (`fs/memory.go:21,46`), `cpu.cfs_quota_us` (`fs/cpu.go:132`). **Rootless runc
concedes it cannot do this** — `specconv/spec_linux.go:832`: "In rootless containers, any attempt to
make cgroup changes is likely to fail… ignores the error"; `config.go:226-228` `RootlessCgroups ⇒
cgroups errors are ignored`.

### 4.4 What is IMPOSSIBLE under an Android shell UID

Requirements below are **R** (source-provable); the failure is **K** (Android platform, inherited):

| requirement | source (R) | failure under shell UID (K) |
|---|---|---|
| `unshare(CLONE_NEWUSER)` | `nsexec.c:1054`; needed by rootless `config.go:222` | `EINVAL` — unprivileged userns disabled on Android GKI |
| `unshare(CLONE_NEWNS/PID/NET/UTS/IPC/CGROUP/TIME)` | `namespaces_syscall.go:12-19` | `EPERM` — `CapEff=0` |
| `mount(2)`, `pivot_root(2)`, `chroot(2)` | `rootfs_linux.go:1164,239,1089` | `mount(2)` → `SIGSYS` (Bionic seccomp); `chroot` → `EPERM` |
| cgroup writes | `fs2/create.go:143`, `fs2/memory.go:64` | `/sys/fs/cgroup` read-only; enabling writes blocked |

**UNVERIFIED:** whether the ro cgroup mount alone blocks an already-per-UID-writable `cgroup.procs`
(vs only the enabling writes). What would settle it: `echo $$ > /sys/fs/cgroup/.../cgroup.procs` as the
shell UID on this device.

### 4.5 Consequence: what a translated root path still CANNOT provide (R for the requirement)

PRoot avoids all of §4.1–4.3 by never issuing `unshare`/`mount`/`pivot_root` and rewriting paths under
`ptrace`. What it structurally cannot fake:

- **Real PID isolation** — no `CLONE_NEWPID` (`namespaces_syscall.go:17`) ⇒ `/proc` is host `/proc`;
  PIDs rewritten for display, not renumbered; no PID-1, no `waitpid` reaping, no `NSpid`.
- **Real network isolation** — no `CLONE_NEWNET` (`:12`) ⇒ no per-container iface/IP; host ports collide
  (this is why §10.2 needs `--fix-low-ports`/`--redirect-ports` as *workarounds*).
- **Resource limits** — no cgroup writes (`fs2/*.go`) ⇒ no `memory.max`/`cpu.max`/`pids.max`, no OOM
  scoping. PDN can enforce quotas only at the `ptrace`/supervisor layer (best-effort).
- **Mount-namespace semantics** — no `CLONE_NEWNS`+`pivot_root` ⇒ host mounts stay visible; bind/ro
  semantics are emulated, not enforced.

**Verdict:** axis = **capability**. `proot-distro` has none of this either, so this is **not** a
v0.3.0 blocker against `proot-distro` — it is the reason a *full-fidelity* mode exists only behind the
UML backend (`roadmap.md` §4.1 `UmlBackend`). PDN's v0.3.0 does not need namespaces or cgroups.

**Acceptance test (if a full-fidelity claim is ever made):** run `unshare --user --pid --mount` as the
shell UID and show the errno; a claim of isolation without a UML backend is false and this is the probe
that shows it. For v0.3.0 the acceptance is the **negative** one: PDN's `login` must **not** advertise
isolation it cannot enforce.

### Dead ends

- Attempting namespaces/cgroups under a shell UID at all (§4.4) — `SIGSYS`/`EPERM`, not a policy choice.
- Rootless Docker recipes (`rootlesskit`, `slirp4netns`, `newuidmap`) — moot once `CLONE_NEWUSER` returns
  `EINVAL`; and dockerd documents the same wall: `daemon/internal/rootless/rootless_linux.go:52-55`
  (setns back to the host netns "fails with EPERM in rootless mode … the daemon lacks CAP_SYS_ADMIN").

---

## 5. Deliberately out of scope for PDN — with the reason

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

## 6. One-line verdicts (index)

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

**Bottom line.** Measured against `proot-distro`, PDN needs **none** of the engine machinery —
daemon API, seam, shim, namespaces, cgroups, mounts — because `proot-distro` has none of it and PDN
already beats or matches it there. The **only** dockerd/containerd material PDN needs for v0.3.0 is a
small subset of the **image/content store** (blobs, image-as-pointer, registry HEAD-skip, digest
validation, OCI-layout import), and it needs that because §10.1/§10.3 put it in the polygon — **not**
because a competitor has it. The capability axis (namespaces/cgroups/mounts) is a wall under a shell
UID and belongs to the UML backend, exactly as `roadmap.md` §1.2 and §4.1 already state; the honest
v0.3.0 obligation there is to **avoid claiming** isolation the PRoot path cannot deliver.
