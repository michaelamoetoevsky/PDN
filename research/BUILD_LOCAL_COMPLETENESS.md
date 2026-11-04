# Issue #30 — `podnext build local` Dockerfile completeness: reproduction, fixes, evidence

Department: Container Build Engineering (CBE). Role: Build Runtime Engineer.
Branch: `dept/build-completeness`. Base revision: `72ef9d6`.
Files owned/changed: `crates/podnext/**` only.

This closes the residual gaps recorded in `research/BUILD_LOCAL_FIX.md` §6
(items 1, 2, 4) and takes the wildcard slice of item 3. Everything below was run
against a real built binary (`cargo build -p podnext --locked`, shared
`CARGO_TARGET_DIR`, under the memory guard and the cargo mutex). The pre-fix
binary was frozen to `$HOME/.hermes/cache/scratch/podnext-before` before the
first rebuild, so before/after is the same fixture on two binaries.

Test fixture (`$HOME/.hermes/cache/scratch/c2/ctx1/Dockerfile`, throwaway tag
`c2env:latest`):

```
FROM alpine:3.19
ENV GREETING=hello-from-image
ENV APP_MODE=prod
WORKDIR /srv/app
RUN mkdir -p /srv/app && printf 'built\n' > /srv/app/marker.txt
COPY app.txt /srv/app/app.txt
EXPOSE 8080
USER nobody
CMD ["/bin/sh"]
```

---

## Item 1 — ENV/WORKDIR are not reinstated at `podnext run` (fixed)

### Reproduction (raw, pre-fix binary)

```
$ podnext-before run c2env:latest -- /bin/sh -c 'echo GREETING=[$GREETING]; echo APP_MODE=[$APP_MODE]; echo PWD=$(pwd); echo MARKER=$(cat /srv/app/marker.txt 2>&1); echo APP=$(cat app.txt 2>&1)'
GREETING=[]
APP_MODE=[]
PWD=/
MARKER=built
APP=cat: can't open 'app.txt': No such file or directory
RUN_EXIT=0
```

The image *has* the values — `metadata.json` written by the build:

```json
{ "env": { "APP_MODE": "prod", "GREETING": "hello-from-image" },
  "working_dir": "/srv/app", ... }
```

so the image did not behave as it was built: empty environment, `cwd=/`.

### Root cause

`ContainerManager::load_containers` (`crates/proot-distro-core/src/container.rs:105`)
builds each container's config from the directory name and `os-release` only:

```rust
let config = ContainerConfig {
    name: name.clone(),
    distro, version,
    arch: std::env::consts::ARCH.to_string(),
    rootfs_path: Some(rootfs_path.clone()),
    ..Default::default()          // env_vars = [], working_dir = None
};
```

The backend *does* read exactly those two fields when it builds the proot
command (`crates/proot-distro-core/src/runtime.rs:143` `config.cwd`,
`:151` `config.env`), so they were dropped only because nothing reloaded them
from the image metadata. `run_container_native`
(`crates/podnext/src/native.rs:626` at base) passed the empty config straight
through.

### Fix

`crates/podnext/src/native.rs:588` — new `apply_image_runtime_config`, called at
`native.rs:628` in `run_container_native` immediately after `ContainerManager::get`:

- reads `<rootfs>/../metadata.json`, applies `env` (object → `env_vars`, sorted
  for determinism) and `working_dir`;
- a value already set on the container wins, so a caller override is never
  clobbered;
- absent/unparseable metadata is a no-op (installed containers have none).

This is deliberately in the `run` path (owned file) rather than the shared
loader, so no other consumer of `ContainerManager` changes behaviour.

### After (raw)

```
$ podnext run c2env:latest -- /bin/sh -c 'echo GREETING=[$GREETING]; echo APP_MODE=[$APP_MODE]; echo PWD=$(pwd); echo MARKER=$(cat marker.txt); echo APP=$(cat app.txt)'
GREETING=[hello-from-image]
APP_MODE=[prod]
PWD=/srv/app
MARKER=built
APP=real app content
RUN_EXIT=0
```

Tests: `native::image_runtime_config_tests` — `run_reinstates_env_and_working_dir_from_metadata`,
`explicit_config_is_not_clobbered_by_metadata`, `missing_metadata_is_a_no_op`
(all offline). Build half asserted by
`commands::build_local_tests::env_and_workdir_are_recorded_for_the_run_path`.

---

## Item 2 — EXPOSE and USER were parsed and discarded (fixed: recorded)

### Reproduction (raw, pre-fix)

`metadata.json` of the pre-fix build (full file):

```json
{
  "cmd": ["/bin/sh"],
  "created_at": "2026-11-04T19:16:03.373493585+00:00",
  "env": { "APP_MODE": "prod", "GREETING": "hello-from-image" },
  "image": "alpine:3.19",
  "labels": {},
  "name": "c2env:latest",
  "rootfs": "/data/data/com.termux/files/home/.podnext/containers/c2env:latest/rootfs",
  "status": "Stopped",
  "tag": "latest",
  "working_dir": "/srv/app"
}
```

`EXPOSE 8080` and `USER nobody` are absent: the build matched
`Instr::Expose(_) | Instr::User(_) => {}` (base `commands/mod.rs:2214`) and dropped them.

### Decision (which behaviour, and why)

- **EXPOSE → recorded.** In Docker `EXPOSE` is metadata: it documents a port and
  never binds or publishes one. This runtime has no host port-forwarding daemon,
  so recording is the faithful behaviour (and the only non-lying one). Duplicate
  ports are collapsed; `EXPOSE 8080/tcp` keeps the port only.
- **USER → recorded, not enforced.** Enforcing it means running the guest as a
  specific uid/gid, and proot maps every guest id to the single host uid;
  `ContainerConfig` has no user/uid field, so plumbing it through
  `runtime.rs`/`proot-core` is a change to shared crates outside this branch's
  owned files. The honest state is therefore "recorded"; a `USER` in a Dockerfile
  still has no effect on execution here. This is a known, named limitation.

### Fix

`crates/podnext/src/commands/mod.rs:2222` (`Expose`), `:2228` (`User`), both
accumulated and written at `:2250` (`"exposed_ports"`, `"user"`); the build also
prints `Expose: 8080` / `User: nobody`.

### After (raw)

```
$ podnext build local -t c2env:latest ctx1
...
Running: mkdir -p /srv/app && printf 'built\n' > /srv/app/marker.txt
Copy app.txt -> /srv/app/app.txt
Expose: 8080
User: nobody
Successfully built image 'c2env:latest'
```

```json
{ ..., "exposed_ports": [ 8080 ], ..., "user": "nobody", ... }
```

Test: `commands::build_local_tests::expose_and_user_are_recorded_in_metadata`
(offline, `FROM scratch`; asserts dedup of `EXPOSE 8080` + `EXPOSE 8080/tcp`).

---

## Item 3 (wildcard slice only) — `COPY src/*.rs` (fixed)

### Reproduction (raw, pre-fix)

```
$ podnext-before build local -t c2glob:latest ctx3      # Dockerfile: FROM scratch / COPY src/*.rs /opt/app/
Copy src/*.rs -> /opt/app/
Error: COPY/ADD source 'src/*.rs' not found in build context
EXIT=1
```

### Root cause

`copy_into_rootfs` (base `commands/mod.rs:2250`) did `ctx.join(src).exists()` on
the literal pattern; `*` is not a real path character, so it always failed.

### Fix

`commands/mod.rs:2273` `copy_into_rootfs` now expands `*`/`?` within a path
component (`expand_glob` `:2341`, `glob_match` `:2372`) and copies every match
via the extracted `copy_one` (`:2311`). A pattern matching nothing is a hard
error; multiple matches require a directory destination ending in `/`, as Docker
requires. `--from=` and multi-stage are still unsupported (see below).

### After (raw)

```
$ podnext build local -t c2glob:latest ctx3
Copy src/*.rs -> /opt/app/
Successfully built image 'c2glob:latest'
EXIT=0

$ find ~/.podnext/containers/c2glob:latest -type f | sort
metadata.json
rootfs/opt/app/lib.rs
rootfs/opt/app/main.rs          # notes.txt correctly excluded
```

Tests: `glob_match_handles_star_and_question`,
`copy_wildcard_copies_every_match_into_a_directory`,
`copy_wildcard_with_no_match_is_an_error`.

---

## Item 4 — tag containing `:` (characterised precisely; NOT fixed — root cause is outside this branch's owned files)

### Reproduction (raw)

Same build as item 1, same binary as item 3:

```
$ podnext build local -t c2env:latest ctx1
...
Running: mkdir -p /srv/app && printf 'built\n' > /srv/app/marker.txt
proot warning: can't sanitize binding "/data/data/com.termux/files/home/.podnext/containers/c2env": No such file or directory
   (x11)
Copy app.txt -> /srv/app/app.txt
Successfully built image 'c2env:latest'
```

`podnext run c2env:latest ...` prints the same 11 warnings.

### It is NOT cosmetic — controlled A/B on the same binary and rootfs

```
### A: colon tag c2env:latest ###
proot warning: can't sanitize binding ".../containers/c2env": No such file or directory   (x11)
ls: /dev/shm: No such file or directory
touch: /dev/shm/probe: No such file or directory
SHM_FAIL
UPTIME=[head: /proc/uptime: Permission denied]
EXIT=0
[sanitize warnings: 11]

### B: colon-free tag alpine (identical command) ###
drwxrwxrwx ... /dev/shm
SHM_OK
UPTIME=[124.08 932.76]
EXIT=0
[sanitize warnings: 0]
```

So for a tag containing `:` the `/dev/shm` sibling (`guestdirs`) and every
`/proc` stub (`sysdata`) are **silently not bound** — exactly the two features
those modules exist to provide. Execution still works, which is why #29 only
noticed the warning.

### Root cause (exact)

The bind specs are strings of the form `"<host>:<guest>"`:

- `crates/proot-distro-core/src/sysdata.rs:278` `bindings()` →
  `format!("{}:{}", stub.display(), guest_path)`;
- `crates/proot-distro-core/src/guestdirs.rs:47` `bindings()` →
  `format!("{}:/dev/shm", dir.display())`;
- joined into `engine_guard.config.bind` (`runtime.rs:113`) and passed to the
  real `proot` binary, which parses `host:guest:flags` and splits on `:`.
  With a host path `.../containers/c2env:latest/sysdata/loadavg`, the first `:`
  ends the host path → source becomes `.../containers/c2env`, which does not
  exist → `can't sanitize binding`.

Confirmed by the message itself: the path it reports is truncated exactly at the
colon (`.../containers/c2env`).

### Why not fixed here

The two fix sites are `sysdata.rs` and `guestdirs.rs` (or the store key), all in
`crates/proot-distro-core/**`, which this branch does **not** own. The only
in-scope alternative — storing the image under a colon-free directory name —
would break the `run <tag>` lookup, `image ls`, `login`, `rm` and regress the
`containers/<tag>/rootfs` layout that #29 established, so it was rejected.

**Precise fix (not applied):** make the host path of a bind spec colon-free.
Either (a) build `config.bind` from a colon-safe runtime directory
(`containers/<sanitised>/`), or (b) escape `:` in the host path and teach
`proot-core`'s bind parsing (`crates/proot-core/src/cli.rs:105`,
`parse_bind_mounts`) the same escape. It belongs to `proot-distro-core`.

---

## Tests and suite state

`cargo test -p podnext --locked`, run under the memory guard + cargo mutex:

```
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   (unittests src/main.rs)
test result: ok.  2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   (unittests src/daemon.rs)
```

Base was 11 + 2 for this crate (per `BUILD_LOCAL_FIX.md` §5), i.e. 13 → 21.
Eight new tests added, none removed:

- `native::image_runtime_config_tests` × 3
- `commands::build_local_tests::expose_and_user_are_recorded_in_metadata`
- `commands::build_local_tests::env_and_workdir_are_recorded_for_the_run_path`
- `commands::build_local_tests::glob_match_handles_star_and_question`
- `commands::build_local_tests::copy_wildcard_copies_every_match_into_a_directory`
- `commands::build_local_tests::copy_wildcard_with_no_match_is_an_error`

Regression guard for #29 kept and still green:
`missing_context_is_a_hard_error_and_writes_nothing`,
`successful_build_registers_a_runnable_image` (image written to
`containers/<tag>/rootfs` + `metadata.json`; hard non-zero exit on a bad build).

## What I did NOT do (explicit)

1. **Multi-stage builds / `FROM ... AS` + `--from=` stage copies** — not
   implemented. Only the last `FROM` is used (pre-existing). No per-stage rootfs.
2. **`**`, `[...]` character classes, brace expansion in COPY/ADD** — not
   implemented; `*`/`?` per path component only (`ponytail:` note in the code).
3. **USER enforcement** — recorded only; a Dockerfile `USER` does not change the
   uid the guest runs as (see item 2 rationale).
4. **EXPOSE port publishing** — recorded only; no host forwarding exists.
5. **The `:` tag warning** — characterised with functional A/B evidence, not
   fixed; the fix site (`sysdata.rs`/`guestdirs.rs`, or the store key) is in
   `crates/proot-distro-core/**`, outside this branch's owned files.
6. **OCI manifest / `~/.podnext/images` entry** — unchanged; the runnable store
   remains the container directory (as established by #29).
7. `podnext image inspect` still reads `ImageManager` (`~/.podnext/images`), not
   these `metadata.json` files, so `exposed_ports`/`user` are not surfaced there.

## Reproduce it yourself

```sh
cd <worktree>
export PATH="$HOME/.hermes/bin:$PATH"
flock $HOME/.hermes/cache/scratch/cargo-mutex.lock -c \
  'CARGO_TARGET_DIR=/data/data/com.termux/files/home/workspace/PDN/target \
   heavy --label c2 --budget-mb 2900 --min-avail-mb 600 -- \
   env -u LD_LIBRARY_PATH cargo build -p podnext --locked'
BIN=/data/data/com.termux/files/home/workspace/PDN/target/debug/podnext

$BIN build local -t c2env:latest ctx1          # Expose:/User: lines; metadata has both
$BIN run c2env:latest -- /bin/sh -c 'echo $GREETING; pwd'   # hello-from-image / /srv/app
$BIN build local -t c2glob:latest ctx3         # COPY src/*.rs expands
find ~/.podnext/containers/c2glob:latest -type f
# tests
flock ... -c '... heavy ... cargo test -p podnext --locked'
```

Raw transcripts: `$HOME/.hermes/cache/scratch/c2/{before-build,before-run,after-build,after-run,after-glob}.log`
and the memory-guard logs under `$HOME/.hermes/logs/heavy/`.
