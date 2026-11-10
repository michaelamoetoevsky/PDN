# Why `--volume` and `--bind` did nothing, and what PRoot's bind syntax actually is

Measured on this device, Termux/Android 16, aarch64, with `podnext 0.1.8` and `proot 5.1.107.101`.
Tool: the in-tree debug binary `target/debug/podnext`, container `test-alpine`.

## 1. The symptom

```
$ mkdir -p $PREFIX/tmp/pdn-mnt-probe && echo MARKER-OK > $PREFIX/tmp/pdn-mnt-probe/marker.txt
$ podnext run test-alpine --volume $PREFIX/tmp/pdn-mnt-probe:/mnt -- cat /mnt/marker.txt
cat: /mnt/marker.txt: No such file or directory
```

The result is the same for `--volume`, for the global `-b/--bind`, and for `login`. No warning appears on stderr.

## 2. Two independent defects

### 2.1 The flag never reached the runtime

`cmd_run` took its bind arguments as `_volume: Vec<String>` and never read them.
`ProotDistroCore::login` loaded the container and called `execute_container`. That call never
folded `LoginOptions.bind_mounts` into `container.config.bind_mounts`. The runtime renders that field.
See `crates/proot-distro-core/src/runtime.rs:115`.
Even a correct argument list did not reach PRoot.

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

The argv that PDN built, captured with `PN_DEBUG=1`. See section 4:

```
proot --kill-on-exit --link2symlink --sysvipc -L --change-id=0:0:0 --rootfs=. --cwd=/ \
      --bind=/dev --bind=/proc --bind=/sys ... \
      "--bind=/data/data/com.termux/files/usr/tmp/pdn-mnt-probe:/mnt:rw" ...
```

Note the contrast inside the same argv. The `/proc` stubs use
`--bind=.../sysdata/mounts:/proc/mounts!`. The `!` suffix is a real PRoot feature. It tells PRoot
not to resolve the final component. This finding does not affect that suffix.
The `!` suffix is not a read-only marker.

## 3. Why the bug survived

The suffix was asserted by tests, never exercised against a guest:

- `crates/podnext/src/commands/mod.rs` asserted `to_argument() == "/srv/data:/data:ro"`.
- `crates/podnext/src/native.rs` asserted the rendered string ends with `:ro`.

A test that encodes the renderer's output can only prove the renderer agrees with itself.
The suite asserted nothing about where the guest sees the mount. That is why a string that no PRoot has
ever accepted looked green.

## 4. Reproducing the capture

```
podnext run test-alpine --volume $PREFIX/tmp/pdn-mnt-probe:/mnt -- cat /mnt/marker.txt   # see the argv
```

This step needs `PN_DEBUG=1`. During this work, the argv print in `crates/proot-core/src/lib.rs`
was reachable only when `config.verbose > 0`. The container paths never set that value.
The one diagnostic that answered the question in seconds was unreachable.
It now also fires when `PN_DEBUG` is set.

## 5. Fix and its evidence

- The two flags are parsed, validated, and folded into `container.config.bind_mounts`.
  A malformed spec is a hard error, not a dropped mount.
- `argument()` renders exactly `<source>:<target>`. A read-only request is an error, and the
error names the mount point. This PRoot cannot express a read-only bind. To pretend otherwise
moves the mount and makes it writable without a warning.
- New tests pin the rendering to one `:` only. They also pin the refusal.
The behavioural proof is the run in `docs/src/known-limitations.md` and in the PoC for this round.

## 6. Open item

Read-only mounts need a PRoot that supports them, or a different mechanism.
Two options are a read-only overlay, or an ownership and mode convention inside the guest.
Until then, `--volume HOST:GUEST:ro` is refused with a reason.
The tool never mounts it writable at another path.
