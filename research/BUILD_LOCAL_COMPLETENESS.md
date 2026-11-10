# Issue #30: Dockerfile completeness for `podnext build local`: reproduction, fixes, evidence

Department: Container Build Engineering. Short name: CBE. Role: Build Runtime Engineer.
Branch: `dept/build-completeness`. Base revision: `72ef9d6`.
Files owned/changed: `crates/podnext/**` only.

This closes the residual gaps recorded in `research/BUILD_LOCAL_FIX.md` section 6.
The gaps are items 1, 2, and 4. This also takes the wildcard slice of item 3.
All work below ran against a real built binary. The build command is
`cargo build -p podnext --locked`, with a shared `CARGO_TARGET_DIR`, under the
memory guard and the cargo mutex. I froze the pre-fix binary to
`$HOME/.hermes/cache/scratch/podnext-before` before the first rebuild. Thus the
before and after runs use the same fixture on two binaries.

Test fixture: `$HOME/.hermes/cache/scratch/c2/ctx1/Dockerfile` with throwaway tag
`c2env:latest`:

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

## Item 1: ENV/WORKDIR are not reinstated at `podnext run`. Status: fixed

### Reproduction, raw, pre-fix binary

```
$ podnext-before run c2env:latest -- /bin/sh -c 'echo GREETING=[$GREETING]; echo APP_MODE=[$APP_MODE]; echo PWD=$(pwd); echo MARKER=$(cat /srv/app/marker.txt 2>&1); echo APP=$(cat app.txt 2>&1)'
GREETING=[]
APP_MODE=[]
PWD=/
MARKER=built
APP=cat: can't open 'app.txt': No such file or directory
RUN_EXIT=0
```

The image *has* the values. The build wrote `metadata.json`:

```json
{ "env": { "APP_MODE": "prod", "GREETING": "hello-from-image" },
  "working_dir": "/srv/app", ... }
```

Thus the image did not behave as it was built. The environment was empty and
`cwd=/`.

### Root cause

`ContainerManager::load_containers` at `crates/proot-distro-core/src/container.rs:105`
builds each container config from the directory name and `os-release` only:

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
command. The fields are `config.cwd` at `crates/proot-distro-core/src/runtime.rs:143`
and `config.env` at `:151`. Nothing reloaded them from the image metadata. That
is why they were dropped. `run_container_native` passed the empty config straight
through. Its location is `crates/podnext/src/native.rs:626` at base.

### Fix

`crates/podnext/src/native.rs:588` adds `apply_image_runtime_config`.
`run_container_native` calls it at `native.rs:628` immediately after
`ContainerManager::get`:

- It reads `<rootfs>/../metadata.json`. It applies `env` and `working_dir`. It
  maps the `env` object to `env_vars` and sorts them for determinism.
- A value already set on the container wins. Thus nothing clobbers a caller
  override.
- Absent or unparseable metadata is a no-op. Installed containers have none.

This is deliberately in the `run` path, an owned file, rather than the shared
loader. Thus no other consumer of `ContainerManager` changes behaviour.

### After, raw

```
$ podnext run c2env:latest -- /bin/sh -c 'echo GREETING=[$GREETING]; echo APP_MODE=[$APP_MODE]; echo PWD=$(pwd); echo MARKER=$(cat marker.txt); echo APP=$(cat app.txt)'
GREETING=[hello-from-image]
APP_MODE=[prod]
PWD=/srv/app
MARKER=built
APP=real app content
RUN_EXIT=0
```

Tests: `native::image_runtime_config_tests` holds
`run_reinstates_env_and_working_dir_from_metadata`,
`explicit_config_is_not_clobbered_by_metadata`, and `missing_metadata_is_a_no_op`.
All tests are offline. `commands::build_local_tests::env_and_workdir_are_recorded_for_the_run_path`
asserts the build half.

---

## Item 2: EXPOSE and USER were parsed and discarded. Status: recorded

### Reproduction, raw, pre-fix

`metadata.json` of the pre-fix build, the full file:

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

`EXPOSE 8080` and `USER nobody` are absent. The build matched
`Instr::Expose(_) | Instr::User(_) => {}` at base `commands/mod.rs:2214`. It dropped them.

### Decision, which behaviour and why

- **EXPOSE to recorded.** In Docker `EXPOSE` is metadata. It documents a port and
  never binds or publishes one. This runtime has no host port-forwarding daemon.
  Thus the faithful behaviour is to record the port. It is also the only
  non-lying one. Duplicate ports are collapsed. `EXPOSE 8080/tcp` keeps the port only.
- **USER to recorded, not enforced.** To enforce it, the guest must run as a
  specific uid/gid. Proot maps every guest id to the single host uid.
  `ContainerConfig` has no user/uid field. Thus to plumb it through
  `runtime.rs`/`proot-core` is a change to shared crates outside this branch's
  owned files. The honest state is therefore "recorded". A `USER` in a Dockerfile
  still has no effect on execution here. This is a known, named limitation.

### Fix

`crates/podnext/src/commands/mod.rs:2222` holds `Expose` and `:2228` holds `User`.
The build accumulates both and writes them at `:2250` as `"exposed_ports"` and
`"user"`. The build also prints `Expose: 8080` and `User: nobody`.

### After, raw

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

Test: `commands::build_local_tests::expose_and_user_are_recorded_in_metadata`.
It is offline and uses `FROM scratch`. It asserts dedup of `EXPOSE 8080` and
`EXPOSE 8080/tcp`.

---

## Item 3, wildcard slice only: `COPY src/*.rs`. Status: fixed

### Reproduction, raw, pre-fix

```
$ podnext-before build local -t c2glob:latest ctx3      # Dockerfile: FROM scratch / COPY src/*.rs /opt/app/
Copy src/*.rs -> /opt/app/
Error: COPY/ADD source 'src/*.rs' not found in build context
EXIT=1
```

### Root cause

`copy_into_rootfs` at base `commands/mod.rs:2250` did `ctx.join(src).exists()` on
the literal pattern. The `*` is not a real path character. Thus it always failed.

### Fix

At `commands/mod.rs:2273`, `copy_into_rootfs` now expands `*`/`?` within a path
component. The helpers are `expand_glob` at `:2341` and `glob_match` at `:2372`.
It copies every match with the extracted `copy_one` at `:2311`. A pattern that
matches nothing is a hard error. Multiple matches require a directory destination
that ends in `/`, as Docker requires. `--from=` and multi-stage are still
unsupported. See below.

### After, raw

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

## Item 4: tag that contains `:`. Characterised precisely and NOT fixed. Root cause is outside this branch's owned files

### Reproduction, raw

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

### It is NOT cosmetic. Controlled A/B on the same binary and rootfs

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

Thus for a tag that contains `:`, the `/dev/shm` sibling and every `/proc` stub
are **silently not bound**. The `/dev/shm` sibling belongs to `guestdirs` and the
`/proc` stub belongs to `sysdata`. Those two modules exist to provide exactly
these two features. Execution still works. Thus #29 noticed only the warning.

### Root cause, exact

The bind specs are strings of the form `"<host>:<guest>"`:

- `crates/proot-distro-core/src/sysdata.rs:278` `bindings()` returns
  `format!("{}:{}", stub.display(), guest_path)`.
- `crates/proot-distro-core/src/guestdirs.rs:47` `bindings()` returns
  `format!("{}:/dev/shm", dir.display())`.
- The code joins them into `engine_guard.config.bind` at `runtime.rs:113` and
  passes them to the real `proot` binary. That binary parses `host:guest:flags`
  and splits on `:`. With a host path
  `.../containers/c2env:latest/sysdata/loadavg`, the first `:` ends the host path.
  The source becomes `.../containers/c2env`, which does not exist. That causes
  `can't sanitize binding`.

The message itself confirms this. The path it reports is truncated exactly at the
colon, `.../containers/c2env`.

### Why not fixed here

The two fix sites are `sysdata.rs` and `guestdirs.rs`, or the store key. All are
in `crates/proot-distro-core/**`, which this branch does **not** own. The only
in-scope alternative is to store the image under a colon-free directory name.
That alternative breaks the `run <tag>` lookup, `image ls`, `login`, and `rm`. It
also regresses the `containers/<tag>/rootfs` layout that #29 established. Thus it
was rejected.

**Precise fix, not applied:** make the host path of a bind spec colon-free.
Option a: build `config.bind` from a colon-safe runtime directory,
`containers/<sanitised>/`. Option b: escape `:` in the host path and teach
`proot-core` bind parsing the same escape. The bind parsing is at
`crates/proot-core/src/cli.rs:105`, `parse_bind_mounts`. It belongs to
`proot-distro-core`.

---

## Tests and suite state

`cargo test -p podnext --locked` ran under the memory guard and the cargo mutex:

```
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   (unittests src/main.rs)
test result: ok.  2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   (unittests src/daemon.rs)
```

Base was 11 + 2 for this crate, per `BUILD_LOCAL_FIX.md` section 5. That is 13 to
21. Eight new tests were added and none were removed:

- `native::image_runtime_config_tests` × 3
- `commands::build_local_tests::expose_and_user_are_recorded_in_metadata`
- `commands::build_local_tests::env_and_workdir_are_recorded_for_the_run_path`
- `commands::build_local_tests::glob_match_handles_star_and_question`
- `commands::build_local_tests::copy_wildcard_copies_every_match_into_a_directory`
- `commands::build_local_tests::copy_wildcard_with_no_match_is_an_error`

The regression guard for #29 is kept and still green. The tests are
`missing_context_is_a_hard_error_and_writes_nothing` and
`successful_build_registers_a_runnable_image`. The image is written to
`containers/<tag>/rootfs` and `metadata.json`. A bad build causes a hard
non-zero exit.

## What I did NOT do, explicit

1. **Multi-stage builds / `FROM ... AS` + `--from=` stage copies**: not
   implemented. Only the last `FROM` is used, pre-existing. No per-stage rootfs.
2. **`**`, `[...]` character classes, brace expansion in COPY/ADD**: not
   implemented. `*`/`?` per path component only. See the `ponytail:` note in the code.
3. **USER enforcement**: recorded only. A Dockerfile `USER` does not change the
   uid the guest runs as. See item 2 rationale.
4. **EXPOSE port publishing**: recorded only. No host forwarding exists.
5. **The `:` tag warning**: characterised with functional A/B evidence, not
   fixed. The fix site is `sysdata.rs`/`guestdirs.rs`, or the store key. It is in
   `crates/proot-distro-core/**`, outside this branch's owned files.
6. **OCI manifest / `~/.podnext/images` entry**: unchanged. The runnable store
   remains the container directory, as established by #29.
7. `podnext image inspect` still reads `ImageManager` at `~/.podnext/images`, not
   these `metadata.json` files. Thus `exposed_ports`/`user` are not surfaced there.

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

## Follow-up, this increment

Items 2-5 of the list above changed. All items were re-measured against commit `b818e3b`:

| # | Before | Now |
|---|---|---|
| 1 | multi-stage / `--from=` silently ignored | **refused** by name and line before any work: the parser drops `--` flags, so `COPY --from=` would have copied from the context |
| 2 | only `*` and `?`; `**` matched nothing | `[...]` classes and ranges implemented (`[abc]`, `[a-z]`, `[!0-9]`); `**` refused with the token named |
| 3 | USER recorded, applied silently | recorded, and the build prints that it is **not enforced** (PRoot maps every guest id to one host uid) |
| 4 | EXPOSE recorded, applied silently | recorded, and the build prints that no host port forwarding exists |
| 5 | `:` tag characterised only | **fixed**: store keys are colon-free (`names::store_key`), a different tag mapping onto the same key is refused rather than overwriting an image, and a colon bind source is a hard error. Reported as #35, closed by `546bf24` + `b818e3b` |
| 6 | no OCI manifest / `~/.podnext/images` entry | unchanged (the runnable store is still the container directory, #29) |
| 7 | `image inspect` reads `ImageManager`, not `metadata.json` | unchanged |

A defect introduced by item 5's fix was found and closed in the same commit. The
base-image install and the `RUN` container lookup still used the raw tag. That tag
cannot resolve a colon tag.

Verification: `cargo test -p podnext --locked` gives `26 passed; 0 failed` plus
`2 passed`. The two new tests are `glob_match_handles_character_classes`,
`double_star_is_refused_by_name`, and `unsupported_dockerfile_shapes_are_refused`.
End-to-end `RUN` with a colon tag is **not** covered by an offline test, because
it needs a real base image. The two call sites now share the single `key`
variable. The tests pin that.
