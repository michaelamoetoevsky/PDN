# Known Limitations

Measured against `podnext 0.1.8` on Termux/Android 16 (aarch64), 5 Nov 2026. Each entry names the
observation that produced it, so it can be re-checked rather than believed. The raw captures behind
these entries are in `research/v019_GAP_BASELINE.md`.

## Defects fixed on `master`, not yet released

`Cargo.toml` still reads `0.1.8` (`grep -m1 '^version' Cargo.toml`) and `podnext version` prints
`podnext 0.1.8`. The `v0.1.9` release was withdrawn before it was tagged (`5b479d1`, after the
reverted `be4919d` / `ed7dc3f`), so the fixes below carry no released version. Each entry names the
command that confirms it in the tree.

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
Both flags are now read and merged (`grep -n 'specs.extend(volume)' crates/podnext/src/commands/container.rs`,
line 189).

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

**`podnext doctor` did not exist.** It does now (see [CLI reference](cli-reference.md#podnext-doctor)).
Host-only it reports six host checks (proot binary and its version, `TMPDIR`, shared memory, DNS,
the kernel, cgroups) plus a seventh `guest` placeholder that says no container was named. With
`--container <name>` it reports twelve: the six host checks and six guest checks (os-release,
architecture, `/dev/shm`, mounts, resolv.conf, and the proot argv it captured from the real
invocation). It now prints the proot argv as the `guest proot argv` check; `PN_DEBUG=1 podnext
run ...` also emits it. Verified with `podnext doctor --container test-alpine --format json`
(12 checks) and `podnext doctor --format json` (7), against `crates/podnext/src/doctor.rs`.

**`login -u/--user` was accepted and ignored; it now refuses.** The [CLI reference](cli-reference.md)
listed `-u, --user <USER>` for `login` as "User to run as", but the value only reached
`LoginOptions.user` and nothing read it: `podnext login alpine --user 1000 -- id` printed
`uid=0(root)`, byte-identical to the same login without the flag (measured inside the guest, #66).
`run` already refused the same flag; `login` now refuses it the same way — exit 1 with the
mapped-root reason on stderr. Confirm: `podnext login alpine --user 1000 -- id` exits 1 and names
`--change-id=0:0:0`.

**`run`/`login` printed nothing about the user the guest runs as.** Roadmap section 9.3 asks that
the build/run output state it, and `build local` already does. Both commands now print one line to
**stderr** before the guest takes the terminal —

```
NOTE: the guest runs as root (uid 0): PRoot maps every guest id to the single host uid via `--change-id=0:0:0`, so there is no privilege separation
```

— so the machine-readable stream is untouched: `podnext run alpine -- true` exits 0 with stdout
still empty, and the note appears on stderr. Confirm: `podnext run alpine -- true` writes the note
to stderr and nothing to stdout.

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
- **`run --detach` + `attach` reconnects, with two declared simplifications.** The round trip is
  real (`9076b93`, #57): `run --detach` starts the guest under a supervised PTY and prints the
  supervisor pid, and `attach <container>` reconnects to the same live shell — the same guest pid
  answers two attaches, a marker written while attached survives a detach, and no `podnext` or
  `proot` process is left after the guest exits. Three things are deliberately **not** done, declared
  here rather than discovered: (1) a resize of the attaching terminal **is not re-forwarded** while
  attached — a client sends its size once when it connects and the supervisor has no `SIGWINCH`
  handler, so a full-screen program opened before a resize keeps the old geometry until the next
  attach; (2) output produced while detached is appended to the session log
  (`sessions_dir()/<pid>.log`) but **is not replayed** on reconnect, so lines printed while nobody
  was attached do not appear — read the log file for them. Both are simplifications of the current
  supervisor, not defects it reports. A third property is worth stating because a script will otherwise
  read it as a failure: **`attach` is interactive and its stdin must stay open.** When stdin reaches EOF —
  `printf 'cmd\n' | podnext attach c`, or any pipe that closes — attach reconnects, ends and exits **0 with
  no output**, which is correct and is not detectable from the exit status. Measured on this device:
  closed pipe 0 bytes / rc 0; open pipe (`( printf 'echo M\n'; sleep 6 ) | podnext attach c`) 33 bytes with
  the marker echoed and the `/ # ` prompt. `scripts/smoke_release.sh` drives it the second way and asserts
  the marker, which is also the only automated coverage the reattach path has.
- **`image pull` / `image push`** refuse with an explanation instead of reporting a success they did
  not achieve: there is no OCI blob store yet, and `podnext install <image>` is the working path.
- **`build submit|status|cancel|list|artifacts|logs`**, **`watchdog events`**, **`service logs`** and
  **`dbus monitor`** refuse and name the reason, with a working alternative where one exists. They
  used to print a not-implemented note on stdout and exit 0, which a script cannot tell from success.
- **Read-only mounts are not available.** `--volume HOST:GUEST:ro` is refused, not silently mounted
  writable: proot 5.1.107.101 has no read-only bind form, and a `:ro` suffix would become part of the
  guest path name (`GUEST:ro`), leaving the requested path unmounted. Restrict access inside the guest
  instead, or use a PRoot that supports read-only binds.
- **Storage and the host home are not mounted into a guest by default.** PDN binds `/dev`,
  `/proc`, `/sys`, the Android system directories, and a per-container `/tmp` and `/dev/shm`; it
  binds no `/sdcard` and no `/storage`. A guest sees the documented storage paths as **empty
  directories from its own rootfs**, not the phone's storage. Measured inside `alpine`
  (`podnext run alpine -- ls -ld /sdcard /storage/emulated/0 /storage/self/primary /storage`):

  ```text
  d---------    2 root     root          3452 Nov  4 00:30 /sdcard
  drwx------    4 root     root          3452 Nov  4 00:30 /storage
  d---------    2 root     root          3452 Nov  4 00:30 /storage/emulated/0
  d---------    2 root     root          3452 Nov  4 00:30 /storage/self/primary
  ```

  All four are on the rootfs device (`stat -c %d` matches `/`), while on the host `/sdcard` is a
  symlink to `/storage/self/primary` -> `/storage/emulated/0` on a different filesystem. The guest's
  `/proc/mounts` lists only `rootfs`, `proc`, `sysfs`, `devtmpfs /dev`, `tmpfs /dev/shm` and
  `tmpfs /tmp`. The host home is not visible either: a file written on the host under
  `/data/data/com.termux/files/home` does not appear at that path in the guest, though a directory of
  that name exists in the rootfs. Reach the phone's storage with an explicit mount —
  `podnext run alpine --volume /sdcard:/mnt/sdcard -- ls /mnt/sdcard` — and note that without it
  `/mnt/sdcard` is absent (it is the negative control in `scripts/check_bindings.py`).
- **`/tmp` and `/dev/shm` are per-container, and the guest `$HOME` is not the Termux home.** The
  guest's `/tmp` is `$HOME/.podnext/containers/<name>/tmp` and its `/dev/shm` is
  `.../<name>/shm`; neither is the host's `$PREFIX/tmp`. A file written in the guest at `/tmp/x` is
  readable on the host at `~/.podnext/containers/<name>/tmp/x`, and a host file written there is
  readable in the guest at `/tmp/x` (the same holds for `/root` <-> `.../rootfs/root`); the guest's
  `$HOME` is `/root`, not `/data/data/com.termux/files/home`. The "shared home/tmp" of the login
  polygon is not implemented (see Deferred to v0.3.0).
- **`TERMUX__PREFIX` cannot be relocated while the app runs.** It is not a PDN setting: Termux
  exports it beside `LD_PRELOAD=…/libtermux-exec-ld-preload.so`, whose job is to rewrite `/bin/*`
  into the prefix (`/bin` is a symlink to `/system/bin`). Pointing it elsewhere makes process
  startup fail, measured on this device — `TERMUX__HOME` relocated: suite passes; `PREFIX`
  relocated: passes; `TERMUX__PREFIX` relocated: `spawn /bin/sh: NotFound`. `PREFIX` is the
  portable override, and it takes precedence after `TERMUX__PREFIX`.
- **Temporary files live in `$PREFIX/tmp`.** `/tmp` exists on Android but is not writable by an app
  uid, and `std::env::temp_dir()` falls back to it when `TMPDIR` is unset.
- **Seccomp filtering is not available, and `--seccomp` says so instead of pretending.** Measured:
  the proot argv is token-for-token identical with and without the flag and contains no seccomp
  token (0 occurrences), and this PRoot build has no seccomp option — `proot --help` lists
  `--sysvipc` and `--kernel-release` and nothing else in that family. The flag was parsed and
  dropped; it now refuses with that measurement on stderr, like the other unsupported `run` flags
  (issue #62). The metadata field `security.seccomp_profile` refuses for the same reason (issue #60):
  measured, no BPF filter is ever built — `SeccompFilter::install` only sets `PR_SET_NO_NEW_PRIVS`
  and its `filter_program` is an empty buffer the compiler reports as never read — so a container
  that asks for a profile is told so instead of being started without one. A profile cannot be
  tailored to a distro while the name is read only as `.is_some()`; enforcing one would need a BPF
  program builder (`prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, ...)`) and a runtime that honours it.
- **`--no-kill-on-exit` is refused; `--kill-on-exit` is the default and cannot be turned off.**
  Measured in the direct-execution path: the generated proot argv is token-for-token identical with
  `--kill-on-exit`, with `--no-kill-on-exit` and with neither — 27 tokens, exactly one
  `--kill-on-exit`, because `proot-core` passes the token unconditionally rather than from the config
  field. The flag would be accepted and never enforced, so it refuses with that measurement
  (issue #63).

- **`/etc/ld.so.preload` is empty in every guest, and that is proot's default, not a PDN bind.**
  proot opens each tracee by binding `/dev/null` over `/etc/ld.so.preload`
  (`cli/proot.c:142`, `new_binding(tracee, "/dev/null", "/etc/ld.so.preload", false)`, quoted in
  `research/PROOT_AND_PROOTDISTRO_SOURCE_DISSECTION.md:113`), which is why that file is empty
  inside a guest and why a host carrying an `ld.so.preload` cannot inject a library into one. PDN
  inherits the bind rather than passing it: the only binds `proot-core` builds are the fixed
  `/dev`, `/proc`, `/sys` (`crates/proot-core/src/lib.rs:256-258`, `:395-397`), the Android system
  binds (`crates/proot-core/src/lib.rs:187-197`) and the caller's own `--bind`
  (`crates/proot-core/src/lib.rs:288-290`) — none of them is `ld.so.preload`.
  `crates/proot-core/tests/argv_facts.rs` asserts PDN's built argv carries **no**
  `ld.so.preload` bind, with `--bind=/dev` as the positive control that bind building works at
  all, so the assertion fails if such a bind is ever added.
- **What the built `proot` argv does and does not contain.** Three facts, each asserted against
  the argv `execute()` actually builds in `crates/proot-core/tests/argv_facts.rs`. First,
  `--link2symlink` is always present: the `ProotConfig` default is `true`
  (`crates/proot-core/src/cli.rs:91`) and both `start()` and `execute()` pass the token
  unconditionally (`crates/proot-core/src/lib.rs:212`, `:351`), which is why the direct-execution
  path refuses the flag as a no-op (`crates/podnext/src/direct.rs:299-306`). Second,
  `PROOT_NO_MOUNTINFO` never appears in the argv or in the guest environment: outside the test,
  no file under `crates/` mentions it, so PDN never disables proot's `mountinfo` extension and a
  guest's `/proc/mounts` is the fake table PDN binds over it
  (`crates/proot-distro-core/src/sysdata.rs:574-579`). Third, the guest's `/dev/shm` is a real
  bind, not a kernel `tmpfs`: PDN binds a per-container directory
  (`crates/proot-distro-core/src/guestdirs.rs:57-68`) and proot makes it report as a tmpfs by
  faking the statfs type `0x01021994` (`proot exit.c:747`, `tracee/seccomp.c:356` — that magic is
  the comparator's, not re-derived in this tree).
- **`dockerd` cannot run under proot, and the gate is the devices cgroup.** dockerd refuses to
  start without a usable devices cgroup (`daemon/daemon.go:1130-1132` in the upstream reference;
  no cgroup emulation exists in proot), so the daemon never gets past its own startup check.

## Not measured yet

These are claims the project cannot currently make, listed so that the absence is visible:

- `login --detach`: `login` takes no `--detach` flag (it exits 2, "unexpected argument"). The
  detached round trip is `run --detach` + `attach`, which **is** measured (#57).
- Terminal fidelity in the guest: `SIGWINCH` handling, `PS1`, `TERM`/`COLORTERM`, UTF-8 locale.
- The `podnext run … -- <cmd>` quoting table.
- A soak run proving no guest processes survive a forced stop.

## Deferred to v0.3.0

- **OCI image store and registry transport** (roadmap section 4.4): blob store, manifest cache,
  digest-addressed layers, pull/push transport. This is the spine of v0.3.0 — import/export and
  push/pull all depend on it.
- **Login polygon** (roadmap section 10.2): shared home, shared tmp, shared X11, bind presets, work dir,
  hostname, `detach` **as a `login` flag**, and the rest of the mobility-parity flag set. (The
  detached round trip itself exists — `run --detach` + `attach`, #57 — but `login` takes no
  `--detach`.)
- **Remaining `/proc` and `/sys` stub cases** for unusual distributions. The current stubs cover the
  common paths; the remainder is documented rather than guessed at.
- **No TUI** until the machine interface is stable (roadmap section 9.4).
