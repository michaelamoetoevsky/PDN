# Issue #29 — `podnext build local` produced no image: reproduction, root cause, fix

Department: Build Systems Engineering (BSE). Branch: `dept/build-local`.
Base revision: `5df9484`. Scope: `crates/podnext/src/commands/mod.rs`, build-local region only.

---

## 1. Reproduction (pre-fix) — raw

Fixture (throwaway, under scratch):

```
$ cat ctx/Dockerfile
FROM alpine:3.19
RUN echo hello > marker.txt
COPY app.txt /app.txt
```

Run the real binary built from `5df9484` (`env -u LD_LIBRARY_PATH target/debug/podnext`):

```
$ podnext build local -t bse-issue29:latest ctx
Parsing Dockerfile: ctx/Dockerfile
Parsed 3 instruction(s):
  [1/3] From { image: "alpine", tag: Some("3.19"), as_name: None }
  [2/3] Run("echo hello > marker.txt")
  [3/3] Copy { src: "app.txt", dest: "/app.txt" }

Target image tag: bse-issue29:latest
Base image: alpine:3.19
Running: echo hello > marker.txt
Generated layer blob: digest=sha256:3c7718154fe02e91f981bfe3e322526fa72a5fb2a2c375fd9e5d64612534d850, size=4096 bytes

Successfully built image 'bse-issue29:latest'
EXIT_CODE=0
```

On-disk state before and after — nothing was written:

```
$ ls -la ~/.podnext/containers/bse-issue29   # before
ls: cannot access '.../containers/bse-issue29': No such file or directory

$ find ~/.podnext -name '*bse-issue29*'      # after
(no match — only permission-denied noise from unrelated containers)
```

And the follow-up command the success message implies:

```
$ podnext run bse-issue29:latest
Error: Container 'bse-issue29:latest' not found
RUN_EXIT_CODE=1
```

This is the silent false success: exit 0, "Successfully built image", no image.

Raw transcript: `$HOME/.hermes/cache/scratch/bse-repro.log`.

### 1b. The stronger case — missing context

`cmd_build_local` guarded its only real work with `if path.exists()` and printed
the success line unconditionally afterwards. With a non-existent context the old
code therefore did *no work at all* and still reported a built image. The new
code fails hard (section 3, test `missing_context_is_a_hard_error_and_writes_nothing`).

---

## 2. Root cause — exact location

`crates/podnext/src/commands/mod.rs` at `5df9484`:

- `cmd_build_local`, line **2036**.
- Layer diff guarded by `if path.exists()`, line **2093**; computes
  `compute_layer_diff(<empty tempdir>, &path, out_tar)` — a tar of the raw build
  context against an empty base — then deletes the tarball (lines 2104–2106).
- Unconditional success, line **2109**:

```rust
println!("\nSuccessfully built image '{}'", target_tag);
```

No step anywhere in the function writes to the runtime's image store. That store
is `~/.podnext/containers/<name>/rootfs` + `metadata.json` — what
`ContainerManager::load_containers` (`crates/proot-distro-core/src/container.rs:45`)
scans and what `podnext run` (`native.rs:577` → `ContainerManager::get`,
`container.rs:153`) and `podnext image ls` read. The `FROM` base was printed but
never pulled, and the context was tarred in isolation rather than assembled on a
rootfs. Result: exit 0 with no runnable artifact.

---

## 3. Fix

`cmd_build_local` now delegates to `build_local_image`, which returns only after
the image is on disk; the success line is unreachable on failure.

`build_local_image` (mod.rs, new):

1. Hard-errors if the build context does not exist, or if there is no `FROM`.
2. Resolves the base from the last `FROM`. `FROM scratch` → empty rootfs (Docker's
   empty base, no pull). Otherwise pulls the base with the existing
   `native::install_image_native` into `~/.podnext/containers/<tag>/rootfs`.
3. Applies the Dockerfile in order: `COPY`/`ADD` copied into the rootfs;
   `RUN` executed **inside the rootfs** through the proot backend
   (`ContainerManager::execute_container`), non-zero → `bail!`; `ENV`/`WORKDIR`
   carried into the run environment; `CMD`/`ENTRYPOINT`/`LABEL` recorded.
4. Writes `metadata.json` and only returns `Ok` once the rootfs exists.

The tag is the store key, so `podnext run <tag>` finds the result directly.

Removed as dead code: `execute_run` (RUN no longer runs in the build context) and
its two tests.

---

## 4. After evidence — raw

### 4a. Missing context now refuses (offline)

```
$ podnext build local -t bse-missing:latest .../does-not-exist
Error: build context '.../does-not-exist' does not exist; pass a directory containing the Dockerfile
EXIT_CODE=1
store entry? NONE
```

### 4b. Real base build: pull + RUN-in-rootfs + COPY + registration

```
$ podnext build local -t bse-real:latest .../realctx
Pulling base image 'alpine:3.19'...
Installing 'alpine:3.19' as 'bse-real:latest'...
[1/4] Authenticating with Docker Hub registry...
[2/4] Fetching image manifest (linux/aarch64)...
[3/4] Downloading and extracting OCI layers (Android-safe)...
[4/4] Configuring guest environment, DNS, and /dev/shm...
Container 'bse-real:latest' installed successfully!
Rootfs: /data/data/com.termux/files/home/.podnext/containers/bse-real:latest/rootfs
Running: echo hello > marker.txt
Copy app.txt -> /app.txt

Successfully built image 'bse-real:latest'
  Image stored at /data/data/com.termux/files/home/.podnext/containers/bse-real:latest
  Run it with: podnext run bse-real:latest
EXIT_CODE=0
```

On-disk result (all real):

```
containers/bse-real:latest/rootfs                     <- pulled alpine rootfs
containers/bse-real:latest/rootfs/app.txt       => "real app content"   (COPY applied)
containers/bse-real:latest/rootfs/marker.txt    => "hello"              (RUN applied in rootfs)
containers/bse-real:latest/rootfs/etc/alpine-release => "3.19.9"
containers/bse-real:latest/metadata.json        -> written
```

`podnext image ls` now lists it:

```
| alpine | 3.19.9 | bse-real:latest | 2026-11-04 16:16 | 3.4 KB |
```

And `podnext run` finds and executes it:

```
$ podnext run bse-real:latest -- /bin/sh -c 'cat /app.txt; cat /marker.txt; echo OS=$(cat /etc/alpine-release)'
real app content
hello
OS=3.19.9
RUN_EXIT=0
```

### 4c. Offline deterministic case (`FROM scratch`)

```
$ podnext build local -t bse-scratch:latest .../scratchctx
Copy app.txt -> /app.txt
Successfully built image 'bse-scratch:latest'
EXIT_CODE=0
--- store contents ---
containers/bse-scratch:latest/rootfs/app.txt   => "static app content"
containers/bse-scratch:latest/metadata.json
```

Raw transcripts: `bse-postfix.log`, `bse-postfix-real.log`, `bse-postfix-run.log`,
`bse-postfix-final.log` (all under `$HOME/.hermes/cache/scratch/`).

---

## 5. Tests

`cargo test -p podnext --locked` (via memory guard + flock):

```
running 11 tests ... test result: ok. 11 passed; 0 failed; 0 ignored
     Running unittests src/daemon.rs
test result: ok. 2 passed; 0 failed; 0 ignored
```

New regression tests (`commands::build_local_tests`):
- `missing_context_is_a_hard_error_and_writes_nothing` — asserts `Err` is returned
  and the image store is untouched (contract, not one exact string).
- `successful_build_registers_a_runnable_image` — asserts the store contains
  `<containers_dir>/<tag>/rootfs/<file>` with the copied bytes and a
  `metadata.json` naming the tag. Offline via `FROM scratch`.

### Test-count delta (13 → 11) — named

Two tests were **removed as dead-code coverage**, not to force green:
- `build_local_tests::run_instructions_execute_in_order_in_context`
- `build_local_tests::failing_run_propagates`

Both exercised `execute_run`, which ran RUN with `sh -c` in the *build context* —
exactly the defective behaviour #29 removes (RUN must run inside the assembled
rootfs). `execute_run` is no longer called by any product path, so it and its
tests were deleted. RUN-in-rootfs is now covered by the integration evidence in
section 4b (RUN wrote `marker.txt` inside the pulled rootfs). The two `ARG` tests
(`arg_is_accepted_and_substituted`, `arg_without_default_is_declared_empty`) were
kept unchanged. Net: −2 dead-code tests, +2 regression tests.

---

## 6. Residual gaps (honest)

1. **`ENV`/`WORKDIR` are not persisted for later `podnext run`.** They are applied
   to the RUN environment during the build and recorded in `metadata.json`, but
   `ContainerManager::load_containers` ignores those keys, so a subsequent
   `podnext run` does not reinstate them. Fixing that touches the shared loader
   (out of this scope).
2. **Wildcards / `--from=` stage copies / multi-stage builds unsupported.**
   `COPY`/`ADD` handle one source path; the parser already collapses multi-source
   COPY, and multi-stage `FROM … AS` is ignored beyond the last `FROM`.
3. **`FROM scratch` produces an image with no shell**, so `RUN` cannot execute in
   it (Docker behaves the same); such a build is metadata+filesystem only.
4. **`EXPOSE`/`USER` are parsed and ignored** (recorded nowhere); `CMD`/`ENTRYPOINT`
   are stored in `metadata.json` but the runtime does not yet consume them.
5. **A tag containing `:` as a container-directory name** (e.g. `bse-real:latest`)
   makes proot print `can't sanitize binding ".../containers/bse-real"` for the
   sysdata/guestdirs bind mounts (proot bind specs are `host:guest:flags`, so a
   `:` in the host path confuses them). Execution still works (section 4b), but the fake
   `/proc` stubs are not bound for such names. A sanitised on-disk name with a
   `run <tag>` alias would fix this; not done here.
6. No OCI manifest/config or `~/.podnext/images` entry is produced — the runtime's
   runnable image store is the container directory, which is what is written.

---

## 7. Reproduction commands

```sh
cd <worktree>
env -u LD_LIBRARY_PATH cargo build -p podnext --locked
BIN=target/debug/podnext   # CARGO_TARGET_DIR/debug/podnext

# pre-fix false success (on 5df9484):
$BIN build local -t bse-issue29:latest <ctx> ; echo $?    # 0 + "Successfully built"
find ~/.podnext -name '*bse-issue29*'                     # no match
$BIN run bse-issue29:latest                               # Container not found

# post-fix:
$BIN build local -t bse-real:latest <ctx-with-FROM-alpine> ; echo $?   # 0, image written
$BIN image ls | grep bse-real
$BIN run bse-real:latest -- /bin/sh -c 'cat /app.txt'
$BIN build local -t bse-missing:latest <does-not-exist> ; echo $?      # 1, no store write

# tests:
CARGO_TARGET_DIR=<shared> env -u LD_LIBRARY_PATH cargo test -p podnext --locked
```
