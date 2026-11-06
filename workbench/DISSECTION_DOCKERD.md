# Dissection: dockerd (moby) — what the daemon does, and what a proot-based tool (PDN) can take

**Status**: research dossier, 6 Nov 2026. Read-only dissection of the extracted moby source. Every claim in
this document carries a `file:line`, a `grep`/`find` result, or a command I ran; anything I could not check is
marked **UNVERIFIED**. No Go build, no `go` command, no git mutation was used. The source tree was not modified.

The purpose is PDN's question: PDN ships stub `docker` and `compose` subcommands. What does the real daemon do
that a proot-based tool *cannot* do, and what is still takeable?

---

## 1. Method and inventory

### 1.1 What was read, and how

All numbers below come from commands I ran in the extracted tree, not from the task description.

```
$ cd /data/data/com.termux/files/home/.hermes/cache/scratch/src/moby-src
$ find . -type f | wc -l          # 2473
$ find . -name '*.go' | wc -l     # 1714
$ for d in */; do printf "%-24s files=%s\n" "$d" "$(find "$d" -type f | wc -l)"; done
```

| Top-level dir | files | Go files | Go lines |
|---|---:|---:|---:|
| `daemon/` | 2127 | 1401 | 259,262 |
| `api/` | 127 | 126 | 7,913 |
| `pkg/` | 21 | 20 | 2,649 |
| `vendor/` | 160 | — | — |
| `cmd/` | 9 | 3 | 56 |
| `internal/` | 13 | — | — |
| `integration/` | 14 | — | — |
| `integration-cli/` | 2 | — | — |

Per-package counts (all from `find <dir> -name '*.go' | wc -l` and
`find <dir> -name '*.go' -exec cat {} + | wc -l`, one command batch):

| Package | Go files | Lines | What it is |
|---|---:|---:|---|
| `daemon/` (flat) | 1401 | 259,262 | the whole daemon, including every subdir below |
| `daemon/command/` | 34 | 3,833 | the real `dockerd` CLI (see §1.3) |
| `daemon/internal/libcontainerd/` | 18 | 2,950 | the daemon↔containerd client |
| `daemon/internal/libcontainerd/types/` | 4 | 344 | the `Client`/`Container`/`Task` interface |
| `daemon/internal/restartmanager/` | 2 | 168 | restart-policy state machine |
| `daemon/internal/runconfig/` | 7 | 540 | container config → exec config |
| `daemon/internal/containerd/server/{supervisor,embedded}/` | 14 | 1,442 | spawning/embedding containerd |
| `daemon/container/` | 24 | 4,350 | the daemon-side container object |
| `daemon/images/` | 25 | 3,124 | graphdriver-backed image service |
| `daemon/containerd/` | 55 | 13,275 | containerd/snapshotter-backed image service |
| `daemon/internal/image/` | 19 | 3,337 | image store |
| `daemon/internal/layer/` | 17 | 3,466 | layer store |
| `daemon/graphdriver/` | 50 | 7,030 | storage drivers (overlay2, vfs, zfs, btrfs, fuse-overlayfs) |
| `daemon/logger/` | 103 | 20,011 | 9 logging drivers + registry |
| `daemon/logger/jsonfilelog/` | 12 | 1,064 | the default driver |
| `daemon/pkg/plugin/` | 23 | 3,984 | plugin manager + executor |
| `pkg/plugins/` | 20 | 2,649 | v1 plugin discovery + HTTP client |
| `daemon/pkg/registry/` | 17 | 3,692 | registry/transfer service |
| `daemon/volume/` | 55 | 8,906 | volume service |
| `daemon/network/` | 7 | 872 | overlay network driver |
| `daemon/libnetwork/` | 341 | 81,001 | IPAM, bridges, veth, iptables |
| `daemon/server/` | 89 | 11,601 | HTTP/gRPC API server |
| `daemon/server/router/` | 55 | 8,234 | API routers |
| `daemon/events/` | 4 | 590 | in-memory event bus |
| `daemon/cluster/` | 57 | 13,638 | swarm |
| `daemon/builder/` | 58 | 9,939 | buildkit/build backend |
| `daemon/config/` | 10 | 3,256 | config types + defaults |
| `api/types/` | 126 | 7,913 | API wire types |
| `cmd/dockerd/` | 3 | **56** | 39-line `main.go` + Windows stub |
| `daemon/internal/rootless/` | 5 | 480 | rootless detection |
| `daemon/internal/extensionconfig/` | 1 | 28 | extension config conversion |

### 1.2 Tag and commit

* Tag: **`moby-docker-v29.9.3`**. Evidence: the sibling tarball is `../moby-docker-v29.9.3.tar.gz`, and
  `tar tzf moby-docker-v29.9.3.tar.gz | head -1` prints `moby-docker-v29.9.3/`.
* Module path: `github.com/moby/moby/v2` — seen in every import, e.g. `daemon/daemon.go:64`.
* **Commit hash: UNVERIFIED.** There is no `.git`, no `VERSION`, no `go.mod`, and no `vendor/modules.txt`
  in the tree (`ls -a` shows only the eight content dirs). What would settle it: the commit SHA recorded
  where the tarball was produced, or a re-extract that keeps `.git`.

### 1.3 Two inventory discrepancies against the task brief

Both were checked with `ls -d` and reported here because they affect how to read the rest:

1. **`libcontainerd/` and `runconfig/` are not top-level.** They are at
   `daemon/internal/libcontainerd/` and `daemon/internal/runconfig/`.
   `ls -d libcontainerd runconfig` → `No such file or directory` for both.
2. **`cmd/dockerd/main.go` is a 39-line stub.** The real CLI (flags, config load, TLS, listener setup,
   containerd supervision, shutdown) lives in `daemon/command/` (34 files, 3,833 lines). `main.go` only
   calls `reexec.Init()` and `command.NewDaemonRunner(...)` — `cmd/dockerd/main.go:16-27`.

Also present but outside the stated extract scope and **not read in depth**: `daemon/libnetwork` (81,001
lines), `daemon/cluster` (13,638), `daemon/builder` (9,939), `daemon/server/router` (8,234), `daemon/volume`
(8,906), `daemon/graphdriver` internals (7,030), `daemon/containerd` internals (13,275), `vendor/` (160
files). Their entry points were read; their bodies were not.

---

## 2. Architecture map

### 2.1 dockerd is a client; containerd is the runtime

The single most important structural fact: **dockerd owns no container process.** It owns metadata, policy,
the API, networking, volumes, images-as-metadata, plugins and events; containerd owns the task, the process,
the shim, the rootfs snapshot, and the content bytes. The daemon reaches containerd over gRPC through a narrow
interface.

```go
// daemon/internal/libcontainerd/types/types.go:56-63
// Client provides access to containerd features.
type Client interface {
	Version(ctx context.Context) (containerd.Version, error)
	// LoadContainer loads the metadata for a container from containerd.
	LoadContainer(ctx context.Context, containerID string) (Container, error)
	// NewContainer creates a new containerd container.
	NewContainer(ctx context.Context, containerID string, spec *specs.Spec, shim string, runtimeOptions any, opts ...containerd.NewContainerOpts) (Container, error)
}
```

The Linux constructor is a two-line delegation to the remote (gRPC) client —
`daemon/internal/libcontainerd/libcontainerd_linux.go:12-14`:

```go
func NewClient(ctx context.Context, cli *containerd.Client, stateDir, ns string, b libcontainerdtypes.Backend) (libcontainerdtypes.Client, error) {
	return remote.NewClient(ctx, cli, stateDir, ns, b)
}
```

**Direction of the callbacks is inverted for events.** The daemon implements `Backend` and containerd's event
stream calls *into* it (`daemon/internal/libcontainerd/types/types.go:39-42`); the daemon never polls.

Anchors:

| Concern | Anchor |
|---|---|
| Daemon struct holds both clients | `daemon/daemon.go:138-139` (`containerdClient *containerd.Client`, `containerd libcontainerdtypes.Client`) |
| gRPC dial | `daemon/daemon.go:1052-1066` |
| Plugin-exec client uses a second namespace | `daemon/daemon.go:1068-1081` |
| libcontainerd client constructed last, after image store | `daemon/daemon.go:1393` |
| `NewDaemon` signature | `daemon/daemon.go:872` |
| Task creation (containerd call) | `daemon/internal/libcontainerd/remote/client.go:216-224` |
| Bundle path label | `daemon/internal/libcontainerd/remote/client.go:42` (`"com.docker/engine.bundle.path"`) |
| Bundle dir = `<ExecRoot>/containerd/<id>` | `daemon/internal/libcontainerd/remote/client.go:725-727` |
| Event subscription, filtered by namespace+topic | `daemon/internal/libcontainerd/remote/client.go:605` |
| Event→callback mapping | `daemon/internal/libcontainerd/remote/client.go:642-697` |
| Backend callback entry point | `daemon/monitor.go:184` (`ProcessEvent`) |

### 2.2 Three ways containerd comes into existence

`daemon/command/daemon_unix.go:132-149` chooses, in this order:

1. **embedded** — feature flag `embedded-containerd` → `initEmbeddedContainerd`
   (`daemon/command/daemon_embedded_containerd.go:18-35`), which calls
   `daemon/internal/containerd/server/embedded.Start` (`server.go:56-86`) and hands the daemon an in-memory
   dialer.
2. **external** — `--containerd` / config address given → use as-is (`nopWaitFunc`).
3. **supervised** — `initializeContainerd` (`daemon/command/daemon.go:1167-1195`) checks for a running
   system containerd at the default address (`daemon/command/daemon.go:1091-1102`), and if none, spawns one as
   a supervised child process with root `<daemon-root>/containerd/daemon`
   (`daemon/command/daemon.go:1201-1213`) and waits on it.
4. If the containerd **binary is missing** (`exec.ErrNotFound`), it falls back to embedded —
   `daemon/command/daemon_unix.go:144-147`.

When embedded, the daemon skips socket syscalls entirely:

```go
// daemon/daemon.go:103-108
// ContainerdDialer dials the in-process containerd over an in-memory pipe.
//
// It is set in embedded mode, so the daemon's own client can skip socket
// syscalls, and is nil otherwise. The signature matches grpc.WithContextDialer,
// and the address argument is ignored.
type ContainerdDialer = func(ctx context.Context, addr string) (net.Conn, error)
```

### 2.3 Startup sequence (the daemon's spine)

`daemon/command/daemon.go:121` is `func (cli *daemonCLI) start(ctx)`. In order:

| Step | Anchor |
|---|---|
| refuse rootless-without-RootlessKit | `daemon/command/daemon.go:142-144` |
| refuse non-root on Linux | `daemon/command/daemon.go:146-149` |
| `CreateDaemonRoot` before any other file | `daemon/command/daemon.go:155-158` |
| `mkdir ExecRoot 0700` | `daemon/command/daemon.go:162-164` |
| pidfile write (and unlink on exit) | `daemon/command/daemon.go:167-183` |
| listeners | `daemon/command/daemon.go:195-199`, `loadListeners` at `:979`, `listeners.Init` at `:1043` |
| containerd bring-up | `daemon/command/daemon.go:200-204` → `initContainerd` |
| plugin store, CDI, middlewares | `daemon/command/daemon.go:279-300` |
| `daemon.NewDaemon(...)` | `daemon/command/daemon.go:302` |
| validate authz plugins **after** restore | `daemon/command/daemon.go:309-312` |
| cluster (swarm) + restart swarm containers | `daemon/command/daemon.go:320-330` |
| buildkit | `daemon/command/daemon.go:332-335` |
| routers, gRPC, extension proxy, HTTP handler | `daemon/command/daemon.go:350-364` |
| config reload trap | `daemon/command/daemon.go:368` |

Inside `NewDaemon` (`daemon/daemon.go:872`): plugin manager is created **before** container restore and the
comment says so — `daemon/daemon.go:1096-1109` ("Plugin system initialization should happen before restore.
Do not change order."). The extension host is built at `:968-976`; containers are loaded by reading
`daemon.repository` (`:238`) and restored in parallel (`:304`, `:1418`).

### 2.4 Runtime-directory and socket invariants the daemon enforces on itself

These are self-imposed, testable invariants — and the ones a proot tool can cheaply copy.

```go
// daemon/daemon_unix.go:1375-1393 (abridged to the enforced bits)
func setupDaemonRoot(config *config.Config, rootDir string, uid, gid int) error {
	config.Root = rootDir
	// the docker root metadata directory needs to have execute permissions for all users (g+x,o+x)
	// so that syscalls executing as non-root, operating on subdirectories of the graph root
	// (e.g. mounted layers of a container) can traverse this path.
	if _, err := os.Stat(rootDir); err == nil {
		// root current exists; verify the access bits are correct by setting them
		if err = os.Chmod(rootDir, 0o711); err != nil {
			return err
		}
	} else if os.IsNotExist(err) {
		// no root exists yet, create it 0711 with root:root ownership
		if err := os.MkdirAll(rootDir, 0o711); err != nil {
			return err
		}
	}
```

| Invariant | Anchor |
|---|---|
| daemon root is `0711`, then `0710` owned by the (possibly remapped) uid/gid | `daemon/daemon_unix.go:1383-1399` |
| `ExecRoot` is `0700` | `daemon/command/daemon.go:162-164` |
| extension runtime dir `ExecRoot/extensions` is `0700`, re-chmod'd | `daemon/extensions.go:23-29` |
| default API socket is `unix:///var/run/docker.sock` | `daemon/pkg/opts/hosts_unix.go:10` (`DefaultHost = "unix://" + DefaultUnixSocket`) |
| under RootlessKit the socket moves to `$XDG_RUNTIME_DIR/docker.sock` | `daemon/command/daemon.go:806-817` |
| unix socket is chowned to group `docker` (falls back to the process gid) | `daemon/listeners/listeners_linux.go:35-54`, group name at `daemon/listeners/group_unix.go:12` |
| inside `$XDG_RUNTIME_DIR` the socket gets the sticky bit | `daemon/listeners/listeners_linux.go:50-53` |
| daemon root mount propagation is fixed up | `daemon/daemon_unix.go:1462` (`setupDaemonRootPropagation`) |
| root in-kernel key quota is raised at boot | `daemon/keys.go:30-44` |
| container state dirs derive from root + id | `daemon/container.go:74`, `daemon/container/container.go:406-411` |

The API socket is a **unix socket or a TCP listener, nothing exotic** — `daemon/listeners/listeners_linux.go:19-60`
handles `fd` (systemd socket activation), `tcp`, `unix`. There is no privileged-port or namespace requirement
in the socket path itself.

### 2.5 Where the boundary actually falls

| Owned by dockerd | Owned by containerd |
|---|---|
| API server + routers (`daemon/command/daemon.go:350-364`) | task/process/shim lifecycle (`remote/client.go:216-238`) |
| container metadata (`config.v2.json`, `hostconfig.json`) and a replica view DB — `daemon/container/container.go:53-54`, `daemon/daemon.go:114-115` | OCI runtime invocation, namespaces, cgroups |
| restart policy (`daemon/internal/restartmanager/`) | rootfs snapshot/mount (`daemon/daemon.go:134-135` introspection of `io.containerd.snapshotter.v1`) |
| health-check scheduling (`daemon/health.go`) | content store bytes (`daemon/internal/libcontainerd/remote/client.go:702-723`) |
| log-driver fan-out (`daemon/logger/`) | image metadata + leases (when the containerd image store is used — `daemon/daemon.go:1244-1246`) |
| networking (`daemon/libnetwork/`) | event stream (`remote/client.go:595-700`) |
| volumes, plugins, events, swarm | checkpoints written into the content store (`remote/client.go:153-176`) |

---

## 3. Mechanisms worth taking for a proot-based tool

Each item: the mechanic, why it survives proot, and the verbatim proof.

### 3.1 Restart policy as a self-contained, testable state machine

This is 132 lines of pure Go with no syscall anywhere. It is the single highest-value lift: PDN can adopt it
verbatim, including the backoff constants and the `ShouldRestart` contract.

```go
// daemon/internal/restartmanager/restartmanager.go:11-15, 46-49
const (
	backoffMultiplier = 2
	defaultTimeout    = 100 * time.Millisecond
	maxRestartTimeout = 1 * time.Minute
)

// ShouldRestart returns whether the container should be restarted.
func (rm *RestartManager) ShouldRestart(exitCode uint32, hasBeenManuallyStopped bool, executionDuration time.Duration) (bool, chan error, error) {
	if rm.policy.IsNone() {
		return false, nil, nil
	}
```

The policy switch — the whole semantic content of `always` / `unless-stopped` / `on-failure:N` — is
`restartmanager.go:80-91`; the "ran ≥10s resets the timeout" rule is `:65-69`.

### 3.2 The exit pipeline: reset → decide → checkpoint → event → delayed restart

`daemon/monitor.go:35-181` is the whole lifecycle driver, in one function. The load-bearing order
(delete task → wait streams → `c.Reset()` → `ShouldRestart` → set state → `CheckpointTo` → emit
`ActionDie` → goroutine waits the backoff channel then starts) is worth copying literally, because the
ordering is what makes crash-restart idempotent.

```go
// daemon/monitor.go:98-101, 125-141 (abridged)
daemonShutdown := daemon.IsShuttingDown()
execDuration := time.Since(c.State.StartedAt)
restart, wait, err := c.RestartManager().ShouldRestart(uint32(ctrExitStatus.ExitCode), daemonShutdown || c.HasBeenManuallyStopped, execDuration)
...
	if restart {
		c.RestartCount++
		...
		c.State.SetRestarting(&ctrExitStatus)
	} else {
		c.State.SetStopped(&ctrExitStatus)
		if !c.HasBeenManuallyRestarted {
			defer daemon.autoRemove(&cfg.Config, c)
		}
	}
```

Note the two details PDN will want: the **networking-error guard** at `monitor.go:48-51` (a container whose
start failed in network setup stays `created` and is never restarted), and
`shouldIgnoreExitEventWithLock` at `monitor.go:359` (duplicate exit events are dropped — moby#46212).

### 3.3 Container state on disk: one directory per container, JSON, plus a replica DB

Metadata is plain JSON under `<root>/containers/<id>/`, addressed through a symlink-scoped resolver:

```go
// daemon/container/container.go:406-411
func (container *Container) GetRootResourcePath(path string) (string, error) {
	// IMPORTANT - These are paths on the OS where the daemon is running, hence
	// any filepath operations must be done in an OS-agnostic way.
	cleanPath := filepath.Join(string(os.PathSeparator), path)
	return symlink.FollowSymlinkInScope(filepath.Join(container.Root, cleanPath), container.Root)
}
```

File names: `config.v2.json`, `hostconfig.json` (`daemon/container/container.go:53-54`), the JSON log at
`<root>/<id>/<id>-json.log` (`container.go:464`), local-driver logs at `<root>/<id>/local-logs/container.log`
(`container.go:474-481`).

Read-only queries are served from a separate in-memory **view DB** so the API can list containers without
deserialising every directory — `daemon/daemon.go:115` (`containersReplica *container.ViewDB`), written via
`CheckpointTo` / `CommitInMemory`. For a proot tool this split (authoritative JSON + cheap replica) is a
cheap win: it removes fsync from the list path.

### 3.4 Log-driver registry: a map, a validator, and a plugin fallback

```go
// daemon/logger/factory.go:15-25
// Creator builds a logging driver instance with given context.
type Creator func(Info) (Logger, error)

// LogOptValidator checks the options specific to the underlying
// logging implementation.
type LogOptValidator func(cfg map[string]string) error

type logdriverFactory struct {
	registry     map[string]Creator
	optValidator map[string]LogOptValidator
	m            sync.Mutex
}
```

Registration is by blank import (`daemon/logdrivers_linux.go:6-15`), and lookup falls through to an external
plugin if the name is unknown — `daemon/logger/factory.go:87-98`:

```go
func (lf *logdriverFactory) get(name string) (Creator, error) {
	lf.m.Lock()
	defer lf.m.Unlock()

	c, ok := lf.registry[name]
	if ok {
		return c, nil
	}

	c, err := getPlugin(name, plugingetter.Acquire)
	return c, errors.Wrapf(err, "logger: no log driver named '%s' is registered", name)
}
```

Three option semantics are enforced centrally and are worth copying exactly (`factory.go:127-152`): the
built-in options are only `mode` and `max-buffer-size`; `max-buffer-size` is only legal with
`mode=non-blocking`; and `mode` must be one of `blocking`/`non-blocking`/unset.

The Logger contract is three methods — `Log(*Message) error`, `Name() string`, `Close() error`
(`daemon/logger/logger.go:68-74`). Default driver is `json-file` (`daemon/config/config.go:48`).

```go
// daemon/logger/logger.go:68-74
type Logger interface {
	// The logger takes ownership of the Message on success. The caller
	// retains ownership of the Message if an error is returned.
	Log(*Message) error
	Name() string
	Close() error
}
```

### 3.5 Health checks: a probe interface plus one monitor goroutine per container

Portable end to end — a probe is just an exec plus a timeout and a retry counter.

```go
// daemon/health.go:19-45 (defaults)
const (
	// Longest healthcheck probe output message to store. Longer messages will be truncated.
	maxOutputLen = 4096
	// Default interval between probe runs (from the end of the first to the start of the second).
	// Also the time before the first probe.
	defaultProbeInterval = 30 * time.Second
	// The maximum length of time a single probe run should take. If the probe takes longer
	// than this, the check is considered to have failed.
	defaultProbeTimeout = 30 * time.Second
	// ...
	defaultProbeRetries = 3
	// Maximum number of entries to record
	maxLogEntries = 5
)
```

The health model has four states and a `FailingStreak`, with the start-period suppressing streak increments
while the container is `starting` — `daemon/health.go:209-236`. Probe types are exactly three: `CMD`,
`CMD-SHELL`, `NONE` (`health.go:338-354`). Exactly one monitor goroutine exists per container
(`health.go:254-256` comment: "There is never more than one monitor thread running per container at a
time"), started/stopped by two functions called from start, stop, pause, unpause and restore —
`updateHealthMonitor` (`:359`), `initHealthMonitor` (`:380`), `stopHealthchecks` (`:403`); call sites at
`daemon/start.go:261`, `daemon/pause.go:50`, `daemon/unpause.go:41`, `daemon/monitor.go:64,200,297,313,327`,
`daemon/daemon.go:507,516`.

Two details worth stealing: the **exec-start timeout is separate from the probe timeout** (30s vs the
configured `Timeout`, `health.go:111-131`) because dockerd→containerd exec start is itself slow; and the
probe output buffer is a `limitedBuffer` capped at 4096 bytes with a `"..."` suffix (`health.go:410-444`).

### 3.6 Plugin discovery is a directory scan, and the extension surface is small and explicit

Legacy (v1) plugins are discovered by **filesystem scan**, not registration: a socket under
`/run/docker/plugins`, or a `.spec`/`.json` file under `/etc/docker/plugins` and
`/usr/lib/docker/plugins`.

```go
// pkg/plugins/discovery.go:20, 36-56 (abridged)
const defaultSocketsPath = "/run/docker/plugins"
...
	for _, entry := range dirEntries {
		if entry.IsDir() {
			fi, err := os.Stat(filepath.Join(l.socketsPath, entry.Name(), entry.Name()+".sock"))
			if err != nil {
				continue
			}
			entry = fs.FileInfoToDirEntry(fi)
		}

		if entry.Type()&os.ModeSocket != 0 {
			names = append(names, strings.TrimSuffix(filepath.Base(entry.Name()), filepath.Ext(entry.Name())))
		}
	}
```

The protocol is HTTP over that unix socket, versioned as `moby.plugins.http/v1`
(`pkg/plugins/plugins.go:38`), and the handshake is a `Plugin.Activate` returning a `Manifest` of
subsystems (`pkg/plugins/plugins.go:58-62`). A plugin must declare which subsystem it implements, and
mismatch is a hard error — `pkg/plugins/plugins.go:256-269`.

The **modern** extension surface (v2.2+ internal plugins and out-of-process extensions) is separate and much
more constrained: the daemon builds an extension host with a `:700` runtime dir under `ExecRoot/extensions`,
scans `--extension-dir` or the fixed default `/usr/libexec/docker/moby-extensions`, and exposes extension
gRPC services on the **same API socket**, failing startup on service-name collision:

```go
// daemon/extensions.go:91-108 (abridged)
func (daemon *Daemon) ExposeExtensionServices(gs *grpc.Server) (*grpcproxy.Proxy, error) {
	if daemon.extensionHost == nil {
		return nil, nil
	}
	reserved := make(map[string]struct{})
	for name := range gs.GetServiceInfo() {
		reserved[name] = struct{}{}
	}

	inproc, err := servicegrpcv0.Collect(daemon.extensionHost)
	...
	for _, svc := range inproc {
		if _, taken := reserved[svc.Name]; taken {
			return nil, fmt.Errorf("in-process extension cannot expose gRPC service %q: it is already served", svc.Name)
		}
```

Runtime plugins (the `docker plugin` v2 kind) are managed by `daemon/pkg/plugin/`: `Manager` at
`daemon/pkg/plugin/manager.go:78`, reload at `:171`, `Get` at `:262`, `GC` at `:291`; enable/disable/restore
at `daemon/pkg/plugin/manager_linux.go:26,182,120`; they are stored under `<root>/plugins`
(`daemon/daemon.go:1098`) and executed via a `plugin.Executor` (`daemon/daemon.go:1068-1093`).

### 3.7 Content-store layout, for anyone who needs a real blob store without containerd

If the daemon has no containerd (Windows path today), it builds its own content store: a bbolt metadata DB
plus a local file store.

```go
// daemon/content.go:19-34 (abridged)
func (daemon *Daemon) configureLocalContentStore(ns string) (*namespacedContent, *namespacedLeases, error) {
	if err := os.MkdirAll(filepath.Join(daemon.root, "content"), 0o700); err != nil {
		return nil, nil, errors.Wrap(err, "error creating dir for content store")
	}
	db, err := bolt.Open(filepath.Join(daemon.root, "content", "metadata.db"), 0o600, &bolt.Options{
		FreelistType: bolt.FreelistMapType,
	})
	if err != nil {
		return nil, nil, errors.Wrap(err, "error opening bolt db for content metadata store")
	}
	cs, err := local.NewStore(filepath.Join(daemon.root, "content", "data"))
	...
	md := metadata.NewDB(db, cs, nil)
```

The namespace wrapper (`daemon/content.go:48-53`, `namespacedContent`) is a ~100-line adapter that injects a
namespace into every context before delegating — a clean pattern for sharing one store between the daemon
namespace and the plugin namespace, and pure Go.

### 3.8 Image store ↔ content store: two implementations behind one interface

The daemon deliberately keeps a **single interface** over two very different backends, selected at startup:

```go
// daemon/image_store_choice.go:14-31 (the choice enum)
const (
	// containerd by default
	imageStoreChoiceContainerd imageStoreChoice = "containerd-by-default"
	// containerd explicitly chosen by user
	imageStoreChoiceContainerdExplicit imageStoreChoice = "containerd-explicit"
	// graphdriver by default
	imageStoreChoiceGraphdriver imageStoreChoice = "graphdriver-by-default"
	// graphdrivers explicitly chosen by user
	imageStoreChoiceGraphdriverExplicit imageStoreChoice = "graphdriver-explicit"
	// would be containerd, but the system has already been running with a graphdriver
	imageStoreChoiceGraphdriverPrior imageStoreChoice = "graphdriver-prior"
)
```

Selection logic is pure and testable — `daemon/image_store_choice.go:94-152` — including the
`hasPriorDriver(root)` rule that pins an existing install to its old driver, and the env override
`DOCKER_DRIVER` / `TEST_INTEGRATION_USE_GRAPHDRIVER`.

The interface both implement is `daemon/image_service.go:27-78`; the two implementations are
`daemon/images` (graphdriver: layer + image store, built at `daemon/daemon.go:1174-1217`) and
`daemon/containerd` (snapshotter + containerd content store, wired at `daemon/daemon.go:1244-1246`,
`NewService` at `daemon/containerd/service.go:93`). When the snapshotter is enabled the daemon
introspects containerd for available snapshotters before proceeding — `daemon/daemon.go:1314-1340`.

The relationship to state plainly: **the content store holds bytes keyed by digest; the image store holds
the mapping from names/configs to a graph of layer digests; the snapshotter turns a layer chain into a
mountable filesystem.** `daemon/internal/layer` (3,466 lines) and `daemon/internal/image` (3,337) are the
graphdriver half of that triple.

---

## 4. What is impossible or meaningless under proot

Reasoning first, then one line per item. The dockerd side of each claim is anchored in source; the proot
side is stated as a constraint of the mechanism (ptrace-based syscall translation, fake root, no kernel
capability), and where I did not verify it on this device it is marked.

**The one-sentence reason**: almost everything dockerd does below the API layer is a kernel-namespace
operation — `clone(CLONE_NEW*)`, `mount`, `pivot_root`, cgroup writes, LSM label application, `iptables`,
`veth`. proot cannot create namespaces and has no `CAP_SYS_ADMIN`; it emulates the filesystem view by
translating paths, so any code path that depends on an *actual* mount or an *actual* namespace is
unreachable, not slow.

| # | Impossible under proot | Why, and the dockerd anchor that shows it needs a real kernel facility |
|---|---|---|
| 1 | **Running a container as containerd does** | `NewTask` hands containerd a bundle and lets the shim create namespaces (`daemon/internal/libcontainerd/remote/client.go:216-224`); the daemon builds the OCI spec with `WithNamespaces`/`WithCapabilities`/`WithSeccomp`/`WithCgroups` (`daemon/oci_linux.go:1025-1078`). Namespace creation is the shim's job and proot cannot do it. |
| 2 | **Any custom seccomp/AppArmor/SELinux confinement** | Applied to the OCI spec at `daemon/oci_linux.go:1037-1039` (`WithSeccomp`, `WithApparmor`, `WithSelinux`); profiles set up at `daemon/daemon.go:996-1000`. No LSM, no labelling. |
| 3 | **User-namespace remapping (`userns-remap`)** | `setupRemappedRoot` (`daemon/daemon_unix.go:1349-1367`) needs `/proc/self/{uid,gid}_map` and subuid/subgid ranges; `daemon/internal/unshare/unshare_linux.go:61-71` enumerates the `CLONE_NEW*` flags. proot's fake-root is not a uid map. |
| 4 | **cgroup limits (`--cpus`, `--memory`)** | `withCgroups` writes `Linux.CgroupsPath` (`daemon/oci_linux.go:782-811`) and reads `/sys/fs/cgroup/...` (`:821-838`). On Android non-root, `/sys/fs/cgroup` is read-only. |
| 5 | **Starting at all if the Devices cgroup isn't mounted** | Hard failure: `daemon/daemon.go:1130-1132` returns `"Devices cgroup isn't mounted"` when `!CgroupDevicesEnabled`. This gate is unconditional (modulo userns), so a faithful port fails before doing anything. |
| 6 | **overlay2 / btrfs / zfs / fuse-overlayfs storage drivers** | `daemon/graphdriver/register/` registers `overlay2`, `btrfs`, `zfs`, `fuseoverlayfs`, `vfs`; every driver but `vfs` mounts. Under proot only a copy-on-write-by-copy driver (`vfs`) is even conceptually available, and the driver registry itself is fine — the mounts are not. |
| 7 | **containerd snapshotter mounts** | The snapshotter is probed at `daemon/daemon.go:1314`; a snapshot's whole point is a mount. Nothing to probe for under proot. |
| 8 | **Networking: bridges, veth, iptables, port publish** | `daemon/container_operations.go:427-450` (`allocateNetwork`, `initializeNetworking`) and the 81,001-line `daemon/libnetwork`. Requires `NET_ADMIN` and network namespaces. |
| 9 | **Raising the root key quota** | `modifyRootKeyLimit` writes `/proc/sys/kernel/keys/root_maxkeys` (`daemon/keys.go:30-44`); this is best-effort (warn-only at `daemon/daemon.go:877-880`) so it is *survivable*, but the write itself will not land. |
| 10 | **Chowning the API socket to group `docker`** | `daemon/listeners/listeners_linux.go:35-46` + `group_unix.go:12`. `chgrp` only works to groups the calling user belongs to; on Android that group does not exist. The fallback (`gid = os.Getgid()`) is exactly what a proot tool should take. |
| 11 | **Live-restore as designed** | It is defined as "containers keep running while the daemon restarts" (`daemon/daemon.go:1558`, `:1791`) — which presupposes a containerd/shim that outlives the daemon. Under proot there is no separate process to outlive anything. |
| 12 | **`dockerd`'s root check is meaningless under proot** | `daemon/command/daemon.go:146-149` rejects `os.Geteuid() != 0`. Under proot `geteuid()` is faked to 0, so the check *passes* while the underlying operations it guards will still fail — a false-positive gate, not a usable safety net. |

**Still takeable / portable under proot** (no kernel facility required):

* restart policy (`daemon/internal/restartmanager/`) — pure logic;
* the exit pipeline and checkpointing (`daemon/monitor.go`, `container.CheckpointTo`) — file writes;
* the whole API server + unix-socket listener (`daemon/listeners/`) — sockets work; only the `docker` group
  chown must be dropped or degraded;
* the event bus (`daemon/events/events.go:19-156`) — in-memory pub/sub;
* log drivers `json-file` and `local` — plain file writes (`daemon/logger/jsonfilelog`, `daemon/logger/local`);
  `journald` is **not** portable (needs `/run/systemd/journal/socket`), and `syslog`/`fluentd`/`gelf`/
  `awslogs`/`gcplogs`/`splunk` need network + an external service but no kernel privilege;
* health checks — a probe is an exec + timer; the machinery is portable even if the exec target is not a
  containerd task;
* plugin discovery, if the search roots are relocated: `/run/docker/plugins` is not writable on Android, but
  `SpecsPaths()`-style config-dir scanning (`pkg/plugins/discovery.go:137-139`) is the portable half;
* the content-store layout (`daemon/content.go`) — bbolt + a directory of blobs, no mount;
* the namespace-wrapper pattern (`daemon/content.go:48-53`).

**UNVERIFIED (proot side, not tested on this device by me)**: that a proot guest can `bind()` a unix socket
inside its guest filesystem (it very likely can — the socket syscall is not namespace-gated — but I did not
run it). What would settle it: inside a proot guest, `python3 -c "import socket;s=socket.socket(socket.AF_UNIX);s.bind('/tmp/x.sock')"` and check for success. Similarly untested: whether `vfs`-style copying is fast enough on Android storage to be a usable image model.

---

## 5. Claim → evidence table

Every claim in this document, with its support. `file:line` means I read that file and those lines.

### 5.1 Inventory and provenance

| Claim | Evidence |
|---|---|
| 2473 files, 1714 Go files | `find . -type f \| wc -l` → 2473; `find . -name '*.go' \| wc -l` → 1714 |
| tag `moby-docker-v29.9.3` | `tar tzf moby-docker-v29.9.3.tar.gz \| head -1` → `moby-docker-v29.9.3/`; tarball filename |
| module path `github.com/moby/moby/v2` | `daemon/daemon.go:64` (and every import in the tree) |
| no git/commit, no go.mod, no VERSION, no vendor/modules.txt | `ls -a` at repo root; `find . -maxdepth 2 -name go.mod` → empty; `head vendor/modules.txt` → `No such file or directory` |
| `libcontainerd`/`runconfig` are at `daemon/internal/…` | `ls -d libcontainerd runconfig` → No such file or directory; `find . -type d -name 'libcontainerd'` → `./daemon/internal/libcontainerd` |
| `cmd/dockerd` is 3 Go files / 56 lines; `main.go` is 39 | `wc -l cmd/dockerd/main.go` → 39; per-dir count above |
| all per-package line counts in §1.1 | `find <dir> -name '*.go' \| wc -l` and `find <dir> -name '*.go' -exec cat {} + \| wc -l` |

### 5.2 Architecture / boundary

| Claim | Evidence |
|---|---|
| daemon holds both a containerd client and a libcontainerd client | `daemon/daemon.go:138-139` |
| libcontainerd Client interface is 3 methods | `daemon/internal/libcontainerd/types/types.go:56-63` |
| Linux constructor delegates to `remote.NewClient` | `daemon/internal/libcontainerd/libcontainerd_linux.go:12-14` |
| events flow containerd→daemon via `Backend.ProcessEvent` | `daemon/internal/libcontainerd/types/types.go:39-42`; `daemon/monitor.go:184` |
| event stream filter string | `daemon/internal/libcontainerd/remote/client.go:605` |
| container bundle lives under `<ExecRoot>/containerd/<id>` | `daemon/daemon.go:1393`; `remote/client.go:725-727` |
| embedded containerd uses an in-memory pipe dialer | `daemon/daemon.go:103-108`, `:1045-1050` |
| four-way containerd selection incl. `exec.ErrNotFound` fallback | `daemon/command/daemon_unix.go:132-149` |
| supervised containerd root dir | `daemon/command/daemon.go:1201-1213`, `:1184` |
| startup order incl. "plugin init before restore" | `daemon/daemon.go:1096-1109`; `daemon/command/daemon.go:295-364` |
| API routers list | `daemon/command/daemon.go:840-874` |
| `dockerd` main is a reexec + runner stub | `cmd/dockerd/main.go:16-27` |

### 5.3 Invariants

| Claim | Evidence |
|---|---|
| daemon root `0711` then `0710` | `daemon/daemon_unix.go:1383-1399` |
| ExecRoot `0700` | `daemon/command/daemon.go:162-164` |
| extension runtime dir `0700` | `daemon/extensions.go:23-29` |
| default socket `unix:///var/run/docker.sock` | `daemon/pkg/opts/hosts_unix.go:10` |
| RootlessKit socket in `$XDG_RUNTIME_DIR` | `daemon/command/daemon.go:806-817` |
| socket chowned to group `docker`, fallback to process gid | `daemon/listeners/listeners_linux.go:35-46`; `daemon/listeners/group_unix.go:12` |
| Devices-cgroup hard gate | `daemon/daemon.go:1130-1132` |
| root key quota write | `daemon/keys.go:30-44` |
| pidfile written then unlinked | `daemon/command/daemon.go:167-183` |

### 5.4 Mechanisms

| Claim | Evidence |
|---|---|
| restart backoff 100ms ×2 capped 1min; ≥10s resets | `daemon/internal/restartmanager/restartmanager.go:11-15`, `:65-78` |
| policy semantics always/unless-stopped/on-failure:N | `restartmanager.go:80-91` |
| `ErrRestartCanceled` on daemon shutdown | `restartmanager.go:17-19`, `:125-132`; `daemon/container/container.go:421` |
| exit pipeline ordering | `daemon/monitor.go:35-181` |
| network-setup-error guard | `daemon/monitor.go:48-51` |
| duplicate-exit guard (moby#46212) | `daemon/monitor.go:53-58`, `:359` |
| auto-remove on stop unless manually restarted | `daemon/monitor.go:136-141`; `daemon/monitor.go:341` |
| container metadata files | `daemon/container/container.go:53-54`; resolver at `:406-411` |
| json log path `<id>/<id>-json.log` | `daemon/container/container.go:464` |
| local log dir `<id>/local-logs/container.log` | `daemon/container/container.go:474-481` |
| replica view DB for cheap reads | `daemon/daemon.go:115`; `daemon/health.go:242` (`CommitInMemory`) |
| log-driver registry + plugin fallback | `daemon/logger/factory.go:15-25`, `:87-98` |
| built-in log opts `mode`, `max-buffer-size` only | `daemon/logger/factory.go:127-152` |
| Logger interface is 3 methods | `daemon/logger/logger.go:68-74` |
| default log driver `json-file` | `daemon/config/config.go:48` |
| log drivers registered by blank import | `daemon/logdrivers_linux.go:6-15` |
| health defaults 30s/30s/3/5 entries, 4096-byte cap | `daemon/health.go:19-45`, `:410-444` |
| health states + start-period streak suppression | `daemon/health.go:209-236` |
| probe types CMD / CMD-SHELL / NONE | `daemon/health.go:338-354` |
| one monitor goroutine per container | `daemon/health.go:254-256`; call sites `daemon/start.go:261`, `daemon/pause.go:50`, `daemon/unpause.go:41`, `daemon/monitor.go:64,200,297` |
| separate 30s exec-start timeout | `daemon/health.go:111-131` |
| plugin discovery by socket/spec dir scan | `pkg/plugins/discovery.go:20`, `:36-94`, `:97-120` |
| plugin spec paths `/etc/docker/plugins`, `/usr/lib/docker/plugins` | `pkg/plugins/discovery.go:122-139` |
| plugin protocol `moby.plugins.http/v1` + `Plugin.Activate` + Manifest | `pkg/plugins/plugins.go:1-23`, `:38`, `:58-62` |
| missing subsystem is a hard error | `pkg/plugins/plugins.go:256-269` |
| extension dirs + reserved-service collision | `daemon/extensions.go:59-86`, `:91-128` |
| runtime plugin manager root `<root>/plugins` | `daemon/daemon.go:1098`; `daemon/pkg/plugin/manager.go:78`, `:171`, `:262`, `:291` |
| content store = bolt metadata + local data dir | `daemon/content.go:19-34` |
| namespace-injecting wrapper | `daemon/content.go:48-53`, `:55-116`, `:118-151` |
| one ImageService interface over two backends | `daemon/image_service.go:27-78` |
| image-store choice enum + selection rules | `daemon/image_store_choice.go:14-31`, `:94-152` |
| graphdriver image service construction | `daemon/daemon.go:1174-1217` |
| containerd image service wiring | `daemon/daemon.go:1244-1246`; `daemon/containerd/service.go:34`, `:93` |
| snapshotter introspection before start | `daemon/daemon.go:1314-1340` |
| user-namespace remap | `daemon/daemon_unix.go:1349-1367` |
| OCI spec security options | `daemon/oci_linux.go:1025-1078` |
| graphdrivers registered: overlay2/btrfs/zfs/fuseoverlayfs/vfs | `ls daemon/graphdriver/register/` |

---

## 6. Open questions and what I could NOT verify

1. **Commit hash.** No `.git` in the extract; only the tag from the tarball name is known. *Would settle it*:
   the SHA recorded at tarball creation, or a re-extract preserving `.git`.
2. **No runtime observation at all.** Every claim here is source-read. I ran no `dockerd`, no `containerd`,
   no `docker` binary — none is installed, and no Go toolchain exists on this device. So all statements
   about *behaviour* are claims about the code, not about observed execution. *Would settle it*: build and
   run moby locally and capture `dockerd --debug` output plus `/var/run/docker.sock` traffic.
3. **Vendored dependency versions.** `vendor/` is present (160 files) but has no `modules.txt` and there is
   no `go.mod`, so the containerd API surface the daemon compiles against is not pinned in this extract. The
   sibling `containerd-v2.4.2.tar.gz` suggests 2.4.2, but I did **not** cross-check it. **UNVERIFIED.**
   *Would settle it*: the moby `go.mod` / `vendor/modules.txt` for this tag.
4. **`daemon/internal/libcontainerd/local/` is Windows-only in this extract**
   (`ls` shows only `libcontainerd_windows.go`, `process_windows.go`, `utils_windows.go`). The Linux path is
   remote-only by design (`libcontainerd_linux.go:12-14`). Whether an in-process Linux local client existed
   at this tag and was merely not needed, I did **not** determine. **UNVERIFIED.**
5. **The health-check exec path is only conditionally portable.** `cmdProbe.run` calls
   `d.ContainerExecStart` (`daemon/health.go:109`) and reads the exit code back from
   `d.getExecConfig(...).ExitCode` (`health.go:159-170`). I did not trace `ContainerExecStart` far enough to
   say whether anything below it is containerd-specific; a proot port would have to supply its own exec.
6. **The daemon's shutdown path was not read.** `daemon/daemon.go:1544 Shutdown`, `daemon/command/daemon.go:593 stop`,
   `:607 shutdownDaemon`, and the live-restore maths at `daemon/daemon.go:1558,1791` were seen in `grep`
   output only. Anything about "how dockerd shuts down cleanly" is **UNVERIFIED**.
7. **Not read in depth** (listed so the next reader knows the gap): `daemon/libnetwork` (81,001 lines),
   `daemon/cluster` (13,638), `daemon/builder` (9,939), `daemon/server/router` (8,234), `daemon/volume`
   (8,906), `daemon/graphdriver` driver bodies (7,030), `daemon/containerd` bodies (13,275),
   `daemon/logger` driver bodies beyond the factory (20,011), `api/types` bodies (7,913), `vendor/`.
   Entry points and interfaces were read; implementations were not.
8. **proot capability claims are stated from the mechanism, not measured here.** I ran nothing inside a proot
   guest. The specific untested items are the unix-socket bind and the throughput of a copy-based image
   model. *Would settle it*: the two probes named in §4.
9. **Swarm, buildkit and the `--tlsverify` warning delay** (`daemon/command/daemon.go:1026-1033`: a 15-second
   startup sleep when binding TCP without TLS verification) were observed but are out of scope for PDN.

---

## Appendix A — commands run

Every command behind a conclusion in this document, in order:

```sh
cd /data/data/com.termux/files/home/.hermes/cache/scratch/src/moby-src
pwd; ls
ls -la /data/data/com.termux/files/home/workspace/PDN/; ls -la .../research/
find . -type f | wc -l
find . -name '*.go' | wc -l
for d in daemon cmd/dockerd libcontainerd api/types runconfig pkg/plugins; do \
  printf "%s files=%s gofiles=%s lines=%s\n" "$d" "$(find $d -type f | wc -l)" \
  "$(find $d -name '*.go' | wc -l)" "$(find $d -name '*.go' -exec cat {} + | wc -l)"; done
ls daemon/; ls cmd/dockerd/; ls libcontainerd/; ls pkg/plugins/
for d in */; do printf "%-24s files=%s\n" "$d" "$(find "$d" -type f | wc -l)"; done
find . -type d \( -name 'libcontainerd' -o -name 'runconfig' \) | head -20
wc -l cmd/dockerd/main.go; cat cmd/dockerd/main.go; ls daemon/containerd/; ls daemon/internal/
for d in daemon/command daemon/internal/libcontainerd daemon/images daemon/logger \
  daemon/internal/plugin pkg/plugins daemon/internal/restartmanager daemon/internal/containerfs \
  daemon/graphdriver daemon/snapshotter daemon/events daemon/internal/image daemon/internal/layer \
  daemon/internal/rootless daemon/internal/stream; do printf ... ; done
cat vendor/github.com/... ; grep -rn "version" --include=VERSION . ; find . -name VERSION
head -5 vendor/modules.txt; grep -m1 "moby/moby" vendor/modules.txt
ls -la ../; ls -la ../moby-src; ls -a; find . -maxdepth 2 -name 'go.mod'
grep -rn "Version = \|DefaultAPIVersion\|minAPIVersion" api/*.go daemon/*.go
tar tzf moby-docker-v29.9.3.tar.gz | head -5
grep -rn "19.03\|ApiVersion =\|MinVersion" api/types/version*.go api/*.go
wc -l daemon/daemon.go daemon/command/daemon.go daemon/container.go daemon/start.go daemon/stop.go \
  daemon/restart.go daemon/monitor.go daemon/health.go daemon/logs.go daemon/logger/factory.go \
  daemon/content.go daemon/image_service.go daemon/image_store_choice.go daemon/create.go \
  daemon/container_operations.go daemon/internal/restartmanager/restartmanager.go \
  daemon/internal/libcontainerd/local/local.go daemon/internal/libcontainerd/remote/*.go
grep -rn "func NewDaemon\|type Daemon struct" daemon/daemon.go
sed -n '/^func NewDaemon(/,/^) (/{p' daemon/daemon.go
grep -n "containerdClient\|containerd.New\|libcontainerd\|ContainerdDialer\|supervisor\|remote.NewClient" daemon/daemon.go
grep -rn "docker.sock\|DefaultDockerHost\|prepareTempDir\|daemon.repository\|daemon.root =" daemon/*.go daemon/command/*.go
wc -l daemon/internal/libcontainerd/*.go daemon/internal/libcontainerd/types/*.go
cat -n daemon/internal/libcontainerd/libcontainerd_linux.go
ls daemon/internal/libcontainerd/types/ .../local/ .../remote/; cat -n .../types/types.go
grep -n "^func \|^type \|^const \|^var " daemon/internal/libcontainerd/remote/client.go
grep -n "^func \|^type " daemon/internal/libcontainerd/remote/client_linux.go daemon/internal/libcontainerd/replace.go
grep -n "^func \|^type \|RestartPolicy\|restartManager\|ShouldRestart\|nextRestart\|MaximumRetryCount\|IsNone()\|IsAlways()\|IsUnlessStopped()" \
  daemon/monitor.go daemon/start.go daemon/stop.go daemon/internal/restartmanager/restartmanager.go daemon/container.go
grep -n "^func \|^type \|^const \|^var " daemon/pkg/plugin/manager.go
wc -l daemon/pkg/plugin/*.go
grep -rn "func (p \*plugin)\|func (pm \*Manager)\|containerd.NewContainer\|NewTask\|shim" daemon/pkg/plugin/manager_linux.go
sed -n '140,210p' daemon/command/daemon.go
ls daemon/listeners/; grep -n "^func \|Chown\|SocketGroup\|umask\|MkdirAll\|os.Chmod" daemon/listeners/listeners_unix.go
grep -rn "func CreateDaemonRoot" -A30 daemon/daemon_unix.go daemon/*.go
grep -rn "func (cli \*daemonCLI) initContainerd" -A30 daemon/command/daemon.go
grep -rn "embedded-containerd\|initEmbeddedContainerd" daemon/command/*.go
cat -n daemon/command/daemon_embedded_containerd.go; sed -n '1,40p' daemon/command/daemon_no_embedded_containerd.go
grep -rn "func getPluginExecRoot" daemon/ --include=*.go
grep -rn "func systemContainerdRunning" -A25 daemon/command/*.go
cat -n daemon/internal/rootless/rootless.go
wc -l daemon/internal/unshare/*.go; grep -rn "CLONE_NEW\|unix.Unshare" daemon/internal/unshare/*.go
grep -rn "DeviceCgroup\|cgroup\|selinux\|apparmor\|seccomp\|SELinux" daemon/oci_linux.go
grep -rn "func modifyRootKeyLimit" -A14 daemon/daemon_linux.go daemon/daemon_unix.go daemon/*.go
cat -n daemon/logdrivers_linux.go; sed -n '1,60p' daemon/logger/plugin.go
grep -rn "func defaultLogConfig" -A25 daemon/config/config.go daemon/*.go
ls daemon/images/; grep -n "^func \|^type \|^const " daemon/containerd/service.go
sed -n '1230,1310p' daemon/daemon.go
grep -rn "RestartManager\|NewRestartManager\|restartmanager.New" daemon/container/container.go daemon/container/*.go
grep -rn "func NewLogFile\|GetLogPath\|LogFileName\|createLogFile" daemon/logger/jsonfilelog/jsonfilelog.go daemon/container/container.go
find . -path '*containerd/supervisor*'; ls daemon/internal/containerd/
wc -l daemon/internal/containerd/server/embedded/*.go; grep -rn "func Start" -A30 .../server.go
grep -rn "supervisor" daemon/command/daemon.go; ls internal/; find . -path '*supervisor*' -name '*.go'
grep -rn "func (container \*Container) GetRootResourcePath" -A10 daemon/container/container.go
sed -n '1,60p' daemon/container/container.go
grep -rn "updateHealthMonitor\|initHealthMonitor\|stopHealthchecks" daemon/*.go daemon/container/*.go
grep -n "^type Logger interface" -A25 daemon/logger/logger.go
grep -rn "DefaultHost\s*=\|defaultSocketGroup" daemon/pkg/opts/*.go daemon/listeners/*.go pkg/*/*.go
grep -rn "StartLogging\|initializeLoggingDriver\|getLogger(" daemon/container_operations.go daemon/monitor.go daemon/logs.go daemon/daemon.go
grep -rn "DefaultUnixSocket" daemon/pkg/opts/hosts_unix.go
grep -n "func (daemon \*Daemon) initializeNetworking\|func (daemon \*Daemon) allocateNetwork\|... " daemon/container_operations.go ...
grep -rn "func (daemon \*Daemon) setupMounts\|... createSpec\|... Cleanup\|... conditionalMountOnStart\|... containerRm" daemon/*.go daemon/*/*.go
grep -n "func (container \*Container) StartLogger" -A55 daemon/container/container.go
sed -n '1025,1120p' daemon/oci_linux.go
grep -n "^func \|^type " daemon/events/events.go
grep -rn "func (cli \*daemonCLI) start\|NewServer\|apiServer" daemon/command/daemon.go
grep -n "^func " daemon/command/daemon.go
sed -n '240,370p' daemon/command/daemon.go
sed -n '485,545p' daemon/daemon.go
wc -l daemon/extensions.go; grep -n "^func \|^type " daemon/extensions.go
ls daemon/internal/extensionconfig daemon/internal/extensionstorage
grep -rn "LiveRestoreEnabled\|liveRestore" daemon/daemon.go daemon/container_operations.go daemon/monitor.go
sed -n '1,60p' /data/data/com.termux/files/home/workspace/PDN/research/PROOT_AND_PROOTDISTRO_SOURCE_DISSECTION.md
grep -n "ptrace\|CAP_SYS_ADMIN\|cannot\|CLONE_NEW\|fake root\|uid 0" .../PROOT_AND_PROOTDISTRO_SOURCE_DISSECTION.md
grep -n "func Get\b" -A40 pkg/plugins/plugins.go
grep -rn "plugins.Get\|GetDriver\|VolumeDriver" daemon/volume/*.go
grep -rn "func setupRemappedRoot" -A18 daemon/daemon_unix.go
ls daemon/graphdriver/; grep -rn "RemappedRoot\|userns-remap" daemon/config/config.go daemon/daemon.go
grep -n "imageStoreChoice\|layerStore\|graphdriver\|snapshotter\|usesSnapshotter" daemon/daemon.go
grep -rn "Register(" daemon/graphdriver/register/register_*.go; sed -n '1,80p' daemon/graphdriver/driver.go
sed -n '960,1000p' daemon/daemon.go
ls daemon/graphdriver/register/; grep -rn "json-file\|DefaultLogConfig\|LogConfig = " daemon/config/config.go daemon/command/config.go
sed -n '55,110p' daemon/daemon.go
grep -n "^func (daemon \*Daemon) Reload\|func (daemon \*Daemon) Shutdown" daemon/daemon.go daemon/reload.go
for d in daemon/internal/runconfig daemon/internal/containerd/server/supervisor ... ; do printf ... ; done
ls -d libcontainerd runconfig; ls ../containerd-src | head -5; date -u
```

All `read_file` calls targeted: `daemon/daemon.go` (111-320, 485-544, 960-1000, 1040-1139, 1230-1310,
1386-1430), `daemon/command/daemon.go` (240-369, 780-899, 1020-1089, 1147-1226),
`daemon/command/daemon_unix.go` (120-174), `daemon/monitor.go` (35-184, 184-343),
`daemon/internal/restartmanager/restartmanager.go` (all), `daemon/restart.go` (all),
`daemon/stop.go` (45-131), `daemon/start.go` (82-231), `daemon/health.go` (all),
`daemon/logs.go` (1-60, 140-194), `daemon/logger/factory.go` (all), `daemon/content.go` (all),
`daemon/image_service.go` (all), `daemon/image_store_choice.go` (all), `daemon/extensions.go` (all),
`daemon/internal/libcontainerd/remote/client.go` (119-258, 595-727),
`daemon/internal/libcontainerd/types/types.go` (1-120), `pkg/plugins/discovery.go` (all),
`pkg/plugins/plugins.go` (1-80), `daemon/internal/rootless/rootless.go` (1-80).
