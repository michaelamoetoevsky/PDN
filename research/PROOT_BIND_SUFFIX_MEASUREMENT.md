# Why `--volume` and `--bind` did nothing, and what PRoot's bind syntax actually is

Measured on this device (Termux/Android 16, aarch64), `podnext 0.1.8`, `proot 5.1.107.101`.
Tool: the in-tree debug binary `target/debug/podnext`, container `test-alpine`.

## 1. The symptom

```
$ mkdir -p $PREFIX/tmp/pdn-mnt-probe && echo MARKER-OK > $PREFIX/tmp/pdn-mnt-probe/marker.txt
$ podnext run test-alpine --volume $PREFIX/tmp/pdn-mnt-probe:/mnt -- cat /mnt/marker.txt
cat: /mnt/marker.txt: No such file or directory
```

Same result for `--volume`, for the global `-b/--bind`, and via `login`. No warning on stderr.

## 2. Two independent defects

### 2.1 The flag never reached the runtime

`cmd_run` took its bind arguments as `_volume: Vec<String>` and never read them;
`ProotDistroCore::login` loaded the container and called `execute_container` without ever
folding `LoginOptions.bind_mounts` into `container.config.bind_mounts`, which is the field
the runtime renders (`crates/proot-distro-core/src/runtime.rs:115`).
So even a correct argument list would not have been passed to PRoot.

### 2.2 The rendered argument used a bind syntax this PRoot does not have

`crates/proot-distro-core/src/bindings.rs::argument` rendered
`<source>:<target>:<ro|rw>`. PRoot 5.1.107.101 splits a bind argument once and treats the
remainder as the guest path, so the suffix became part of the target name.

Direct evidence: with the mount requested at `/mnt`, the guest showed it at `/mnt:rw`.

```
$ podnext run test-alpine --volume $PREFIX/tmp/pdn-mnt-probe:/mnt -- ls -l "/mnt:rw"
total 4
-rw-------    1 root     root            10 Nov  5 10:30 marker.txt      <- the bind landed here

$ podnext run test-alpine --volume $PREFIX/tmp/pdn-mnt-probe:/mnt -- ls -l /mnt
total 0                                                              <- the requested path is empty
```

The argv PDN built (captured with `PN_DEBUG=1`, see §4):

```
proot --kill-on-exit --link2symlink --sysvipc -L --change-id=0:0:0 --rootfs=. --cwd=/ \
      --bind=/dev --bind=/proc --bind=/sys ... \
      "--bind=/data/data/com.termux/files/usr/tmp/pdn-mnt-probe:/mnt:rw" ...
```

Note the contrast inside the same argv: the `/proc` stubs use
`--bind=.../sysdata/mounts:/proc/mounts!`. The `!` suffix is a real PRoot feature (do not
resolve the final component) and is unaffected by this finding - it is not a read-only
marker.

## 3. Why the bug survived

The suffix was asserted by tests, never exercised against a guest:

- `crates/podnext/src/commands/mod.rs` asserted `to_argument() == "/srv/data:/data:ro"`.
- `crates/podnext/src/native.rs` asserted the rendered string ends with `:ro`.

A test that encodes the renderer's output can only prove the renderer agrees with itself.
Nothing asserted where the guest sees the mount, which is why a string that no PRoot has
ever accepted looked green.

## 4. Reproducing the capture

```
podnext run test-alpine --volume $PREFIX/tmp/pdn-mnt-probe:/mnt -- cat /mnt/marker.txt   # see the argv
```

Requires `PN_DEBUG=1`. While chasing this, the argv print in `crates/proot-core/src/lib.rs`
was reachable only when `config.verbose > 0`, which the container paths never set, so the
one diagnostic that would have answered the question in seconds could not be printed at
all. It now also fires when `PN_DEBUG` is set.

## 5. Fix and its evidence

- The two flags are parsed, validated, and folded into `container.config.bind_mounts`; a
  malformed spec is a hard error instead of a dropped mount.
- `argument()` renders exactly `<source>:<target>`, and a read-only request is an error
  naming where the mount would have landed. Read-only binds cannot be expressed by this
  PRoot, so pretending otherwise would move the mount and silently make it writable.
- New tests pin the rendering (one `:` only) and the refusal; the behavioural proof is the
  run in `docs/src/known-limitations.md` and in the PoC for this round.

## 6. Open item

Read-only mounts need either a PRoot that supports them or a different mechanism (a
read-only overlay, or an ownership/mode convention inside the guest). Until then
`--volume HOST:GUEST:ro` is refused with a reason rather than mounted writable elsewhere.
