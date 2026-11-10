# v0.3.0 component confirmation - second dissection round

Sources re-read raw: proot, the Termux fork, with 32.638 lines of C. proot-distro v5.10.3, with 29.015 lines of Python. moby/dockerd,
with 289.273 lines of Go. containerd plus the shim, with 531.855 lines of Go. runc, with 348.020 lines of Go. shroot, with 86.018 lines of C. The evidence
files are `research/R2-REFERENCE.md` with 664 lines and about 180 file:line citations, `research/R2-ENGINE.md` with 427 lines, and
`research/R2-EXECUTION.md` with 554 lines. Every verdict below uses one competitive test. The briefs state the test and this document applies it
to every item: **can PDN lose to Termux's proot-distro without this?** The answer "no" appears as often as the
answer "yes". That result is the finding.

## The answer in one table

| axis | what the round confirmed PDN needs for v0.3.0 | what it confirmed PDN does NOT need |
|---|---|---|
| **Mobility** | the whole login/run polygon: `--bind`, `--shared-tmp/home/termux-home`, `--work-dir`, `--hostname`, `--redirect-ports`, `--no-sysvipc/--no-link2symlink`, `--detach`, `--env`, `--get-proot-cmd`, `--user` (`parser.py:116-158`); archive and OCI-layout import; portable export; a checksummed URL install; the ecosystem surface (short alias, completions, curated names, per-command help) | nothing here is optional - this axis IS the competition, and 23 of the mechanisms in `R2-REFERENCE.md` are YES |
| **Capability** | the image/content store, five mechanisms: digest-addressed blobs + Info (`core/content/content.go:41,90`), image as name->descriptor (`core/images/image.go:35`), registry HEAD-skip + PUT (`core/remotes/docker/pusher.go:118-192`), digest validation, OCI-layout import (`core/images/archive/importer.go:248`) | the containerd daemon API (16 gRPC services; dockerd's seam is 3 methods, `daemon/internal/libcontainerd/types/types.go:57-63`), the shim's 17-RPC Task service, namespaces/mount(2)/cgroups, and cgroup policy enforcement - proot-distro has none of them either, so omitting them costs PDN nothing *against proot-distro*; they belong to the UML backend (roadmap section 4.1) |
| **Stability** | a real subreaper (proot-distro reaps orphans, PDN reaps nothing), an honest cgroupfs view, and the three PDN defects below | runc's SIGKILL-without-pidns path (`cgroup.kill`, `container_linux.go:407`) is a dead end on Android: read-only cgroupfs and CapEff=0 |
| **Optimization** | nothing from this round | selective seccomp interception and SVC rewriting buy 3.6x-12.3x on metadata syscalls but cost 1.53x on fork/exec - a different execution model, not a v0.3.0 patch |

## Three defects found in PDN itself, from reading the comparators

1. `crates/pn-next/src/supervisor.rs:1694` writes `cpu.max`, `memory.max` and `pids.max` into a private directory.
   It binds nothing and returns `Ok()`. That is the same lie A10 and A14 removed from the daemon surface, one layer
   down: a limit is reported as applied when no kernel object exists. New item **A20**.
2. `crates/proot-core/src/seccomp.rs:72-76` - `install()` sets `no_new_privs` and then installs no filter at all.
   A function named `install` that installs nothing is worse than an absent one: callers believe a security
   boundary exists. New item **A21**.
3. PDN has **no subreaper anywhere**, while proot-distro reaps orphans. Children of a dead session survive with
   PPID 1 and are measured in the campaign logs. New item **A22**, the one stability mechanism the round marked YES.

## Corrections this round forces on earlier work

| where | what was wrong | what the source says |
|---|---|---|
| B6's paragraph (docs) | the `/dev/null` -> `/etc/ld.so.preload` bind was described as an inherited default | it is inside `handle_option_q` (`cli/proot.c:73`), so it fires only with `-q/--qemu`; the doc must say so or it invents a behaviour |
| roadmap section 10.1 | the reference installs from a URL "with checksum" | there is no `--checksum` anywhere (`grep checksum` in `install*`/`download` -> 0 hits); the reference checks the declared length only. This is a row PDN can BEAT, not match |
| roadmap section 10.3 | "export"/"import" as reference capabilities | the reference has no such subcommands: portable export is `build -o FILE` (`oci_writer.py:155`) and import is `install <archive>`, out of 18 commands (`pages.py:1061`) |
| earlier dissection, shroot | `--probe` was said to test selective ptrace | ptrace is a baseline, not a probe step |
| earlier dissection, shroot | "19 identity syscalls" | 18 |
| earlier dissection, proot | a quoted grep for read-only bind support | the conclusion holds (`grep MS_RDONLY|readonly|rdonly src/` -> 0) but the quoted output was wrong |

## What stays exactly as it was

The seven `S` items stand, with one refinement. **S8, the cgroupfs view, is now confirmed as one of only two YES
verdicts on the stability axis**. **S9, enforcement, is confirmed as correctly deferred**. The round-2 evidence
is stronger than the evidence of round 1. In shroot, `grep sys/fs/cgroup src/` returns zero. The README of shroot
states the limits at lines 214-216. The `B` group stays closed. The dockerd citation in B15 is unaffected. `C1-C10` need new wording against this
table, not replacement. The two roadmap cells above must change in the same commit.

## Unknowns carried forward, deliberately

`src/syscall/*` and `src/tracee/*` of proot were read at surface level only. The file `proot.h` of proot is generated, and its
`doc/` was not diffed. Help-page rendering was not exercised. The containerd source is a pruned subset. The shim internals,
and with them ttrpc fd-passing, are UNVERIFIED. One question remains unproven: does a read-only `/sys/fs/cgroup` alone block an already-writable
`cgroup.procs`? The grammar of go-digest is not vendored. Each item is labelled UNVERIFIED in its own file,
not smoothed over.
