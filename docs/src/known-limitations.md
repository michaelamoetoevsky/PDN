# Known Limitations

Measured against `podnext 0.1.8` on Termux/Android 16 (aarch64), 5 Nov 2026. Each entry names the
observation that produced it, so it can be re-checked rather than believed. The raw captures behind
these entries are in `research/v019_GAP_BASELINE.md`.

## Defects fixed in v0.1.9

**`--bind` and `--volume` were accepted and did nothing.** Two independent causes, both
measured: `cmd_run` took the argument as `_volume` and never read it, and the rendered bind
argument carried a `:rw` suffix that this PRoot turns into part of the guest path name. The
guest evidence that separated them:

```sh
$ podnext run test-alpine --volume $PREFIX/tmp/bindtest:/mnt -- ls -l "/mnt:rw"   # old build
-rw-------    1 root     root            10 Nov  5 10:30 marker.txt               # landed here
$ podnext run test-alpine --volume $PREFIX/tmp/bindtest:/mnt -- ls -l /mnt        # old build
total 0                                                                           # not here
```

The full trace, including the captured PRoot argv, is in
`research/PROOT_BIND_SUFFIX_MEASUREMENT.md`. The documented short form was wrong as well:
`podnext run ubuntu -v /sdcard:/mnt/sdcard` exited 2 because `-v` is `--verbose`, not `--volume`.

**`image ls` printed a meaningless size and called the container name an `IMAGE ID`.** The size came
from `du -sb` on the rootfs; when `du` exited non-zero the code fell back to the directory's inode
size and printed it as if it were a measurement:

```sh
$ podnext image ls
| debian | latest | deb-pdn | ... | 3.4 KB |          # the rootfs is 402.1 MB
$ du -sb ~/.podnext/containers/deb-pdn/rootfs ; echo rc=$?
du: cannot read directory '.../rootfs/apex': Permission denied
rc=1
$ stat -c '%s bytes' ~/.podnext/containers/deb-pdn/rootfs
3452 bytes
```

The container carries Android system directories inside its rootfs, an app uid cannot read them, and
the failure was invisible in the output. Now `du` is the measurement of record, a walk over the
readable files supplies a figure marked `>=` when subtrees could not be read, and a rootfs that cannot
be measured at all prints `unknown`. The column that held the container name is now `CONTAINER`; no
image ID exists to print, because the metadata carries the image name and tag and no digest.

**`podnext doctor` did not exist.** It does now (see [CLI reference](cli-reference.md#podnext-doctor)):
host checks for the proot binary and its version, `TMPDIR`, shared memory, DNS, the kernel and cgroups,
plus guest checks (os-release, architecture, `/dev/shm`, mounts, resolv.conf) when a container is named.
It does not print the proot argv; `PN_DEBUG=1 podnext run ...` does.

## Deliberate limitations

- **Exit codes**: `0` success, `1` general error, `2` invalid arguments. A command run inside a
  container propagates the guest's own status unchanged, including `128 + signal`. There are no
  distinct codes for "container not found", "image not found" and so on; earlier releases documented
  codes 3–12, none of which any code path returns.
- **`kill`** runs a fixed teardown sequence (SIGQUIT to the traced root, then a sweep for inherited
  session descriptors). Selecting the signal and the timeout is not exposed.
- **`volume create`** takes a driver and driver options only. There is no size or storage-pool
  selection: a volume is a directory under `$HOME/.podnext/volumes`.
- **`network create`** takes driver, subnet and gateway. Carving an IP range inside the subnet is not
  implemented.
- **`run --detach`** refuses: a detached session cannot be re-entered yet, so the command would
  otherwise run in the foreground while reporting that it detached. Run in the foreground, or under
  `podnext service` supervision.
- **`image pull` / `image push`** refuse with an explanation instead of reporting a success they did
  not achieve: there is no OCI blob store yet, and `podnext install <image>` is the working path.
- **`build submit|status|cancel|list|artifacts|logs`**, **`watchdog events`**, **`service logs`** and
  **`dbus monitor`** refuse and name the reason, with a working alternative where one exists. They
  used to print a not-implemented note on stdout and exit 0, which a script cannot tell from success.
- **Read-only mounts are not available.** `--volume HOST:GUEST:ro` is refused, not silently mounted
  writable: proot 5.1.107.101 has no read-only bind form, and a `:ro` suffix would become part of the
  guest path name (`GUEST:ro`), leaving the requested path unmounted. Restrict access inside the guest
  instead, or use a PRoot that supports read-only binds.
- **Temporary files live in `$PREFIX/tmp`.** `/tmp` exists on Android but is not writable by an app
  uid, and `std::env::temp_dir()` falls back to it when `TMPDIR` is unset.

## Not measured yet

These are claims the project cannot currently make, listed so that the absence is visible:

- `login --detach` (the command refuses today; the round trip is unmeasured).
- Terminal fidelity in the guest: `SIGWINCH` handling, `PS1`, `TERM`/`COLORTERM`, UTF-8 locale.
- The `podnext run … -- <cmd>` quoting table.
- A soak run proving no guest processes survive a forced stop.

## Deferred to v0.2.0

- **OCI image store and registry transport** (roadmap section 4.4): blob store, manifest cache,
  digest-addressed layers, pull/push transport. This is the spine of v0.2.0 — import/export and
  push/pull all depend on it.
- **Login polygon** (roadmap section 10.2): shared home, shared tmp, shared X11, bind presets, work dir,
  hostname, detach, and the rest of the mobility-parity flag set.
- **Remaining `/proc` and `/sys` stub cases** for unusual distributions. The current stubs cover the
  common paths; the remainder is documented rather than guessed at.
- **No TUI** until the machine interface is stable (roadmap section 9.4).
