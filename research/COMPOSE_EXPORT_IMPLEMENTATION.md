# Compose export implementation (#28, #6)

Department: Interop & Export Engineering (IEE). Branch: `dept/compose-export`
(worktree `~/.hermes/cache/scratch/wt-dept-iee`, base `5df9484`). HEAD after this
commit: see `git log`.

Every claim below is backed by a command run against the tree; raw logs are the
paths cited per section (`~/.hermes/cache/scratch/iee-*.log`, and the `heavy`
logs under `~/.hermes/logs/heavy/`).

---

## 1. Real field inventory

`ServiceConfig` and its nested structs, `crates/pn-next/src/supervisor.rs`.
Command (verbatim):

```sh
awk '
/^pub struct (ServiceConfig|ServiceMeta|ProcessConfig|ResourceLimits|RestartPolicy|HealthCheckConfig|StdioConfig|LogRotationConfig|SocketConfig) / {s=$3; print "\n== " s " =="; c=0; inb=1; next}
inb && /^}/ {print "   fields=" c; inb=0; next}
inb && /pub [a-z_]+:/ {match($0,/pub [a-z_]+:/); print "  " substr($0,RSTART+4,RLENGTH-5); c++}
' crates/pn-next/src/supervisor.rs
```

Raw output:

```
== ServiceConfig ==
  meta
  process
  resources
  restart
  health
  dependencies
  env
  working_dir
  user
  stdio
  sockets
   fields=11

== ServiceMeta ==
  name
  description
  version
  service_type
  tags
   fields=5

== ProcessConfig ==
  command
  args
  pre_start
  post_stop
  pre_reload
  cwd
  umask
  nice
  cpu_affinity
   fields=9

== ResourceLimits ==
  cpu_limit
  memory_limit
  memory_swap_limit
  pids_limit
  cpu_shares
  io_weight
  io_read_bps
  io_write_bps
   fields=8

== RestartPolicy ==
  condition
  max_restarts
  restart_window_secs
  base_delay_secs
  max_delay_secs
  backoff_multiplier
   fields=6

== HealthCheckConfig ==
  enabled
  interval_secs
  timeout_secs
  start_period_secs
  retries
  command
  http_url
  tcp_port
   fields=8

== StdioConfig ==
  stdout
  stderr
  combine
  rotation
   fields=4

== LogRotationConfig ==
  max_size
  max_files
  compress
   fields=3

== SocketConfig ==
  name
  socket_type
  listen
  backlog
  accept
   fields=5
```

**Re-derived count.** The audit's "~37 sub-fields across six nested structs" is
imprecise: the file has **nine** structs carrying fields (8 nested + the
top-level). Leaf fields total **48** (`5+9+8+6+8+4+3+5`), plus `ServiceConfig`'s
own five scalars (`dependencies`, `env`, `working_dir`, `user`, `sockets`) and
six nested-struct fields. The toy exporter mapped **one** leaf (`process.command`)
via `{:?}`. There is **no `image` field and no `build` field** anywhere.

---

## 2. Mapping: field -> emitted YAML key -> skipped-with-reason

| Source field | Emitted Compose key | Notes |
|---|---|---|
| `meta.name` | `services.<name>` | service key |
| `process.command` ++ `process.args` | `command` | omitted when the argv is empty (never `command: []`) |
| `env` | `environment` | keys sorted for deterministic output |
| `process.cwd` ?? `working_dir` | `working_dir` | `cwd` (process override) wins |
| `user` | `user` | |
| `restart.condition` | `restart` | `Never`->`no`, `OnFailure`->`on-failure`, `Always`->`always`, `UnlessStopped`->`unless-stopped`, `OnAbnormal`-> omitted |
| `dependencies` | `depends_on` | |
| `sockets` (Stream, numeric port) | `ports` | port extracted after the last `:` |
| `health.*` | `healthcheck.{test,interval,timeout,start_period,retries}` | only when a probe exists: `command`, else curl of `http_url`, else nc of `tcp_port` |
| `resources.cpu_limit` | `cpus` | |
| `resources.memory_limit` | `mem_limit` | bytes |
| `resources.memory_swap_limit` | `memswap_limit` | bytes |
| `resources.pids_limit` | `pids_limit` | |
| `resources.cpu_shares` | `cpu_shares` | |
| (external, CLI `--image`) | `image` | `ServiceConfig` has no image field |

Deliberately **skipped**, with reason:

| Source field | Reason skipped |
|---|---|
| `meta.description`, `meta.version`, `meta.service_type`, `meta.tags` | no native Compose service key (informational only) |
| `process.pre_start`, `process.post_stop`, `process.pre_reload` | lifecycle hooks; Compose has no equivalent |
| `process.umask` | no Compose key |
| `process.nice` | no Compose key |
| `process.cpu_affinity` | maps to `cpuset`; not emitted (kept the incremental surface small) |
| `restart.max_restarts`, `restart_window_secs`, `base_delay_secs`, `max_delay_secs`, `backoff_multiplier` | only `restart` (condition) has a top-level Compose key; `deploy.restart_policy.*` is swarm-only |
| `health.enabled` alone (no probe) | `enabled` with no command/url/port gives nothing to test with |
| `resources.io_weight`, `io_read_bps`, `io_write_bps` | map only onto a device-scoped `blkio_config` |
| `stdio.*` (incl. `rotation`) | logging-driver config; no faithful mapping |
| non-`Stream` sockets / unix `listen` paths | no numeric TCP port to publish |

---

## 3. Defects fixed (vs PR #26 / 5df9484)

1. **Empty command emitted a null body.** `name:` with no child -> Compose
   rejects (`services.<name> must be a mapping`). Now an input-less service
   serialises as `name: {}` (a valid mapping); a service with any default restart
   policy serialises as `name:` + `restart: …`. Empty argv never emits
   `command: []`.
2. **`{:?}` was used as a YAML serialiser.** Replaced with `serde_yaml`
   (`serde_yaml = "0.9"`, already a dependency at `crates/pn-next/Cargo.toml:67`).
   A control character that `{:?}` renders `\u{1}` is now emitted as a valid
   YAML escape, so a quoting-hostile value cannot corrupt the document.
3. **Only `command` was mapped (1 of 48 leaves).** Now 15 keys across command,
   environment, working_dir, user, restart, depends_on, ports, healthcheck and
   five resource limits.
4. **Deprecated `version: '3.8'`** removed; modern files start at `services:`.
5. **No test.** Six exact-string unit tests added.
6. **Unreachable dead code.** `export_to_compose` had zero callers; it is now
   reachable through `podnext compose` (#6).

---

## 4. Raw evidence

### 4.1 Unit tests — `~/.hermes/cache/scratch/iee-02-tests.log`

```
test supervisor::compose_export_tests::args_merged_and_cwd_precedence ... ok
test supervisor::compose_export_tests::empty_command_with_default_restart_has_restart_only ... ok
test supervisor::compose_export_tests::empty_service_is_an_empty_mapping_not_a_null_body ... ok
test supervisor::compose_export_tests::image_is_emitted_when_supplied ... ok
test supervisor::compose_export_tests::hostile_values_are_quoted ... ok
test supervisor::compose_export_tests::standard_service_exact ... ok
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 0.00s
```

### 4.2 Full suite — `~/.hermes/cache/scratch/iee-04-fulltest.log`

Command: `env -u LD_LIBRARY_PATH cargo test -p pn-next -p podnext --locked`
(under `heavy --budget-mb 2200 --min-avail-mb 700`, serialised by `flock`).
`.status`: `exit_code=0 peak_rss_mb=2156 duration_s=207`.

```
Running unittests src/lib.rs (pn_next)          -> ok. 58 passed; 0 failed
Running tests/supervisor_e2e.rs (pn_next)       -> ok. 19 passed; 0 failed
Running unittests src/main.rs (podnext)         -> ok. 11 passed; 0 failed
Running unittests src/daemon.rs (podnext_daemon)-> ok.  2 passed; 0 failed
```

### 4.3 End-to-end CLI — `~/.hermes/cache/scratch/iee-03-e2e.log`

`podnext compose --help` then, against real TOML:

```
### compose db.toml web.toml   (multi-service)
# Generated by podnext
services:
  db:
    command:
    - postgres
    - -D
    - /var/lib/pg
    restart: on-failure
    ports:
    - '5432'
  web:
    command:
    - nginx
    - -g
    - daemon off;
    working_dir: /srv/web
    user: 1000:1000
    environment:
      HOST: 0.0.0.0
      PORT: '8080'
    restart: always
    depends_on:
    - db
    healthcheck:
      test: [CMD, curl, -f, http://localhost/health]
      interval: 30s ; timeout: 10s ; start_period: 10s ; retries: 3
    cpus: '1.5' ; mem_limit: '67108864' ; pids_limit: 256
```

`--image my-app:latest` prepends `image: my-app:latest`; `-o out.yml` writes the
file and prints `Wrote 1 service(s) to out.yml`; a file with no command is
rejected by the loader (`Error: loading service config nocmd.toml`, exit 1).

Quoting-hostile command survives a round trip (PyYAML 6.0.3):

```
### compose hostile.toml
  hostile:
    command:
    - 'a: b'
    - '#notacomment'
    - he said "hi"
    - |-
      line1
      line2
    - -leading-dash
    - 'true'
```
```
yaml.safe_load(...)["services"]["hostile"]["command"]
 == ["a: b", "#notacomment", 'he said "hi"', "line1\nline2", "-leading-dash", "true"]
ALL PYYAML ASSERTIONS PASS
```

### 4.4 serde_yaml exact-output probe

The exact expected strings in the tests were taken from a real run
(`iee-01-probe.log`), not hand-written. Observed: block sequences are emitted at
the parent key's indentation; `8080`/`1.5`/`67108864` are single-quoted to keep
their string type; `he said "hi"` and `-leading-dash` stay plain (valid); a
newline becomes a `|-` literal block.

---

## 5. Residual gaps / not verified

- **`docker compose` is not present on this host** (`docker: 'compose' is not a
  docker command`), so the output was validated with PyYAML and shape
  assertions, **not** by `docker compose config`. The empty-service fix removes
  the *parse* rejection; a service still needs an image or a build context to be
  *runnable*, and `ServiceConfig` has neither — the CLI's `--image` is the path
  to that. This semantic limitation is real and documented, not papered over.
- **`ports` from sockets** are emitted as the bare container port (e.g. `'5432'`),
  which loses any host-side binding in the `listen` string. Faithful for the
  common `host:port` case only in the container-port sense.
- **`stdin_open`/`tty`, networks, extra hosts, capabilities, ulimits** are not
  modelled because `ServiceConfig` has no corresponding fields.
- The exporter is not wired to `podnext service export` or the daemon; only the
  new top-level `compose` command uses it.

---

## 6. Reproduction

```sh
cd ~/workspace/PDN
git worktree add --detach ~/.hermes/cache/scratch/wt-dept-iee 5df9484
cd ~/.hermes/cache/scratch/wt-dept-iee && git switch -c dept/compose-export

# unit tests
export PATH="$HOME/.hermes/bin:$PATH"
flock $HOME/.hermes/cache/scratch/cargo-mutex.lock -c \
 'CARGO_TARGET_DIR=/data/data/com.termux/files/home/workspace/PDN/target \
  heavy --budget-mb 2200 --min-avail-mb 700 --label iee-tests -- \
  env -u LD_LIBRARY_PATH cargo test -p pn-next --lib --locked compose_export_tests'

# full suite
flock $HOME/.hermes/cache/scratch/cargo-mutex.lock -c \
 'CARGO_TARGET_DIR=/data/data/com.termux/files/home/workspace/PDN/target \
  heavy --budget-mb 2200 --min-avail-mb 700 --label iee-fulltest -- \
  env -u LD_LIBRARY_PATH cargo test -p pn-next -p podnext --locked'

# e2e
env -u LD_LIBRARY_PATH cargo build -p podnext --bin podnext --locked
./target/debug/podnext compose /path/web.toml /path/db.toml | \
  env -u LD_LIBRARY_PATH python3 -c 'import sys,yaml; print(yaml.safe_load(sys.stdin))'
```
