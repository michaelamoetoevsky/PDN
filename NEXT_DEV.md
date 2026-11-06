# NEXT_DEV.md

What the comparators actually do, measured from their sources and from the binaries on this device, and
what PDN should take from them next.

This document exists because "we know how containers work" is a claim, not a finding. Every statement below
carries an **evidence marker** and, wherever it matters, the command or `file:line` that settles it. Where
something could not be verified, it says so instead of rounding up.

**Markers used throughout**

| marker | meaning |
|---|---|
| **M** | measured on this device - the command and its output are cited |
| **R** | read from source - `file:line` or `file:function` is cited |
| **K** | documented knowledge, not verified here - must not be used as evidence for a decision |
| **UNVERIFIED** | checked and could not be settled; names what would settle it |

**Status of this document**: sections 1-6 are complete and evidence-backed. Section 7 is the round-2
dissection in progress - five source readings, each landing as its own file under `research/`, whose
findings are folded in here as they arrive. Section 8 is the consolidated backlog and is the part a reader
should act on.

---

## 1. The components, measured (M)

Every scale number below was produced by walking the extracted tree with `find`/`wc`, not from a README.
The fetch commands are reproducible in full at the end of this section.

| component | version | source form | scale (M) |
|---|---|---|---|
| **proot** | `5.1.107.101` installed; fork source at `termux/proot` | C, cloned `--depth 1` (**no upstream sync** - its own `AGENTS.md` says so) | **138 `.c` files / 32,638 lines**, 71 headers, **113 test scripts**, 8 subsystems under `src/` |
| **proot-distro** | `5.10.3` installed | Python, read in place from the installed package | **77 files / 29,015 lines** + bash, zsh and fish completions |
| **dockerd (moby)** | `docker-v29.9.3` | Go, codeload tarball, selectively extracted | 2,473 files, **1,714 Go files / 289,080 Go lines**, 19,075 KiB on disk |
| **containerd** | `v2.4.2` | Go, codeload tarball, selectively extracted | 2,115 files, **1,979 Go files / 531,855 Go lines**, 25,309 KiB |
| **runc** | `v1.5.2` | Go, codeload tarball, selectively extracted | 944 files, **930 Go files / 346,450 Go lines**, 14,172 KiB |
| **Docker CLI** (installed here) | reports `v24.0.6-ce` (**M**, `docker --version`) | ELF aarch64 binary on the device | **26,683,472 bytes** (**M**, REA, section 2) |

```sh
# reproduce the fetches (POSIX sh; no /tmp on this device - everything under $HOME/.hermes/cache/scratch)
S=$HOME/.hermes/cache/scratch/src; mkdir -p $S; cd $S
for r in moby/moby containerd/containerd opencontainers/runc; do
  t=$(gh api repos/$r/releases/latest --jq .tag_name)
  curl -sL -o "$(basename $r)-$t.tar.gz" "https://codeload.github.com/$r/tar.gz/refs/tags/$t"
done
# proot, the fork that builds the binary on this device
git clone --depth 1 https://github.com/termux/proot $HOME/.hermes/cache/scratch/proot-src
# the two mechanical inventories of proot-distro and proot are in
# research/PROOT_AND_PROOTDISTRO_SOURCE_DISSECTION.md, with the commands that regenerate them
```

## 2. Binary-level evidence, from REA (M)

Source reading says what a project *intends*; the shipped binary says what it *is*. REA's Go build-info
reader was used on the one Docker binary installed on this device:

```
subject : /data/data/com.termux/files/usr/bin/docker
          sha256 ced88beff59410c2adaf2898f376c4bb9b01abcafa4b53bd5d1a4dca15d80f0d
format  : elf, arm64, 64-bit, little-endian, 26,683,472 bytes
build   : go1.27.3  (inline build info at offset 26,228,832; version field at 26,228,865, 8 bytes)
module  : github.com/docker/cli/cmd/docker   main module: null
          decoded dependencies: 0   build settings: 13   unparsed lines: 0
```

**What that establishes**: what the installed CLI is built from - a Go 1.27.3 build of `docker/cli`'s
`cmd/docker`, 26.7 MB of aarch64. **What it does not**: the tool's own limitations say dependency coverage
is *unknown* when no main-module record is decoded (a GOPATH-mode build), and that embedded build metadata
"is observed file content, not proof of compiler identity, dependency authenticity or runtime behavior". So
"0 dependencies" is not a finding about dependencies - it is the absence of a record. This is the same
discipline the rest of this document is held to.

Note the version split, which is the sort of thing that misleads: `docker --version` reports `v24.0.6-ce`,
the binary's embedded build says `go1.27.3` and `docker/cli`. Two different facts, two different sources,
both measured.

## 3. What each component owns (R/M)

The point of the table is the boundary in the third column, because that is where the design decisions are.

| component | owns | delegates to | PDN's counterpart today |
|---|---|---|---|
| **proot** | the guest↔host path mapping (**string prefix rewriting**, `path/path.c:substitute_path_prefix` **R**), the ptrace/syscall interception, `-b` bindings, nine behaviour extensions | nothing - it is the substrate | PDN *drives* it, and builds its argv |
| **proot-distro** | container lifecycle, the download/extract/install path, the login argv, a Dockerfile-driven build engine, an OCI pull/push client | proot (for every execution), the registry (for images) | the closest analogue; PDN reimplements most of it in Rust |
| **dockerd** | the API, the container lifecycle *policy*, image store, networks, logging drivers, health checks, plugins | containerd (task lifecycle), runc (through containerd) | `podnext` CLI + `podnext-daemon`; `docker`/`compose` subcommands are stubs |
| **containerd** | content store, snapshotters, leases and GC, the supervisor that starts one **shim per container**, the event/task services | runc (or another runtime) for the actual container | PDN's daemon + supervisor + session registry |
| **containerd-shim** | survives a containerd restart, holds the container's stdio and exit status, answers ttrpc | the runtime | PDN's `run --detach` supervisor |
| **runc** | creating the container from the OCI spec: namespaces, cgroups, mounts, seccomp, capabilities, the `runc init` re-exec, `state.json` | nothing below it | PDN under proot: **no** own namespaces; it fakes the contracts |

## 4. Doctrines carried forward from round 1 (R)

The first pass over proot and proot-distro produced eight doctrines, each with a `file:function` anchor;
the full text is `research/PROOT_AND_PROOTDISTRO_SOURCE_DISSECTION.md`. Condensed, because they are the
reason the backlog in section 8 is ordered the way it is:

1. **Everything goes through a descriptor.** `dirfd.py` (1,361 lines) has `open_regular_at` (O_NOFOLLOW
   *plus* an `fstat` type check - "O_NOFOLLOW alone is not enough for an endpoint a container can
   prepare"), `open_new_at` (O_EXCL), `copy_metadata` ("expressed against file descriptors, so no path -
   and therefore no TOCTOU"), `rmtree_at` ("a symlink is unlinked, never traversed"), and descriptor-stack
   cleanup on interruption.
2. **The runtime directory is a trust root and it is guest-writable.** `_open_cache_root` says so in the
   source: "on Termux the cache lives *inside* RUNTIME_DIR, whose components are guest-writable". Hence
   `_open_lock_file`'s O_NOFOLLOW + type check, and the typed handling of a *planted* lock path.
3. **Locks carry a hint; "cannot measure" is a state.** `_lock_info_at` writes PID + command name into the
   lock file; `LockStateUnknown` exists as an outcome rather than being collapsed into "busy".
4. **An untrusted digest is validated before it becomes a path.** `validate_digest` - "a choke point before
   any conversion of an untrusted digest into a filesystem path" - and `_oci_blob_path` validates first so
   "a crafted index.json cannot route the lookup through a member name".
5. **Credentials are bound to an origin.** `same_auth_origin` records the hole it closed: comparing netlocs
   let a same-host redirect that changed only the scheme keep the Authorization header, "https://reg.example
   -> http://reg.example put the Bearer token ... on the wire in clear". `download.py:check_redirect`
   refuses an HTTPS-to-HTTP redirect outright.
6. **Retry is a policy, not a loop.** Retry transient, never deterministic ("an HTTP client error (4xx)")
   which "cannot succeed on a repeat request", honour both forms of `Retry-After`, and check the declared
   body length (`require_complete_body`: a cut chunked body is "the quiet" half of a truncated response).
7. **Builds are content-addressed and reap their own children.** `run_step.do_run` looks in a recipe-hash
   cache first; `_become_subreaper` - "ask the kernel to reparent orphaned descendants onto this process.
   This is what makes a step's leftovers findable at all".
8. **The session is a file held open, and the image config is read at every start.** `register_session`
   clears O_CLOEXEC so proot and its descendants keep the fd, and liveness is "some process still holds
   it"; `paths.py:container_manifest` is read at every `login`/`run` because "its Env goes into the child
   environment".

### Round-2 additions, from the deep readings (R), parent-verified where marked

The parent re-checked these against the trees on this device rather than accepting the report - six claims
confirmed at the cited line, one refuted (below).

**proot, the parts round 1 had not read**

- **Exactly one extension is enabled by default: `mountinfo`,** gated only by the environment variable
  `PROOT_NO_MOUNTINFO` - `src/cli/cli.c:481-482`. **verified**: those two lines are the `getenv` guard and
  the `initialize_extension(tracee, mountinfo_callback, NULL)` call. Every other extension is opt-in:
  `kompat` `-k`, `fake_id0` `-0/-i/--change-id/-S`, `link2symlink` `-l`, `sysvipc` `--sysvipc`,
  `ashmem_memfd` `--ashmem-memfd` (Android/bionic builds only), `fix_symlink_size` `-L`, `hidden_files`
  `-H`, `port_switch` `-p`. `src/extension/extension.c:103` is the only registration path, and there are
  exactly **9 call sites** - **verified** by counting them.
- **Two facts about PDN's own argv that follow from it, measured in PDN's source.** PDN passes
  `--link2symlink` by default (`crates/proot-core/src/lib.rs:208`, `:347`,
  `crates/podnext/src/native/image.rs:117`, default `true` at `crates/proot-core/src/cli.rs:91`), so L2S is
  live in every session - which is why `crates/podnext/src/commands/sync.rs:58-67` must rewrite L2S
  intermediate targets. And PDN **never** sets `PROOT_NO_MOUNTINFO` (no occurrence under `crates/`), so the
  one default-on extension is always active in a PDN guest. Neither fact is recorded in PDN's own docs.
- **Seccomp acceleration comes from a static list**, not from observation: `proot_sysnums[]` at
  `src/syscall/seccomp.c:331` carries **exactly 108 entries** - **verified** by counting the unique `PR_`
  symbols that follow it - and an extension appends to the filter, so the filter is a function of the
  extension set. The gate is `PROOT_NO_SECCOMP`.
- **`/dev/shm` is faked at the syscall layer, not mounted**: `src/syscall/exit.c:747` writes `TMPFS_MAGIC`
  into the returned `statfs`, and `src/tracee/seccomp.c:356` sets `f_type = 0x01021994` - **verified**, both
  lines. PDN's per-container `/dev/shm` therefore stands on a fiction, which belongs in what PDN documents
  about shared memory.
- **`kill-on-exit` is off unless asked**: `src/cli/proot.c:195-199` sets `tracee->killall_on_exit = true`
  only inside the option handler - **verified**.
- **The guest never runs its own program directly**: proot substitutes its embedded loader for `SYSARG_1`
  (`src/execve/enter.c:718-728`), extracts that loader to a temporary file (overridable with
  `PROOT_LOADER`; directory via `PROOT_TMP_DIR`), and pushes a serialized load script below the tracee's
  stack pointer at exit (`src/execve/exit.c:174-403`). Two consequences PDN lives with: exec needs a
  writable temp directory, and `PROOT_TMP_DIR` names a host path the guest can influence.
- **Many syscalls are emulated or voided rather than passed through**: `getcwd`, `chdir`/`fchdir`,
  `mount`/`umount`/`pivot_root`/`unshare`/`setns`, `CLONE_NEW*` stripping in `clone`/`clone3`, the
  `fsopen` family (`-ENOSYS`), `execveat` without `AT_FDCWD` (`-ENOSYS`), `brk` rebuilt on `mmap`/`mremap`,
  `statx` rebuilt on `stat`. The void trick is `set_sysnum(tracee, PR_void)`, with `PR_void` mapped to a
  "SYSCALL_AVOIDER" number that differs per architecture. For PDN the question is which of these a Rust
  reimplementation must reproduce to keep a guest working and which it can ignore precisely because it
  drives proot rather than replacing it.
- **An outer seccomp policy is a supported path**: a `SIGSYS` carrying `si_code == SYS_SECCOMP` is caught
  and rewritten (`src/tracee/event.c:837-891`), so a guest that installs its own filter gets emulated
  syscalls rather than a killed process. This is why "it worked on my machine" is not evidence for a guest
  under proot.

**proot-distro, the parts round 1 had not read**

9. **stdout is a data channel; every message goes to stderr.** `message.py:187` writes to `stderr` and
   erases the progress line only on a TTY; the only `print()`s in the whole package are the pipeable
   identifiers in `list.py`/`ps.py`/`search.py` plus the `backup` archive stream and `--get-proot-cmd`.
   **verified**: a grep for `print(` across all 77 modules produced exactly one other hit,
   `message.py:207: print(*args, file=sys.stderr)`, which is stderr. PDN records the same rule for its own
   output in its 0.2.0 changelog (`41768a6`, with stdout reserved for data and `--json` purity asserted at
   `f6329c5`); the reference's version is stricter because it is enforced across the whole package rather
   than per command.
10. **Writes are atomic and the rename is identity-checked.** `atomic.py:92 atomic_replace` writes through a
   descriptor obtained by `statedir.py:156 open_state_dir` (a per-component O_NOFOLLOW walk from a trust
   root), and `atomic.py:232` refuses with `ESTALE` if the temporary file's `(st_dev, st_ino)` changed
   underneath it. `statedir.py:44-50` names the one case it cannot close - a name re-pointed after a
   validated descriptor has been handed out.
11. **Session liveness is an inherited flock, never a PID.** `session.py:174` clears O_CLOEXEC so proot and
   every descendant hold the descriptor; `session.py:472` probes with a shared, non-blocking `flock`;
   `session.py:392 session_holders()` finds survivors by scanning `/proc/*/fd` for the inode. A PID can be
   reused; a held descriptor cannot.
12. **`sync --delete` refuses to guess, and `kill` escalates for a stated reason.** `sync.py:1093` declines
   to prune when the root was unreadable, `sync.py:565` refills the source list for a second pass so a file
   written between passes is not pruned, and `copy.py:198` falls back from `renameat` on `EXDEV` in a way
   that removes nothing when the copy was incomplete. `kill.py:236` escalates TERM then QUIT then KILL
   because, in its own words, "proot ignores SIGTERM/SIGINT/SIGHUP outright, so a graceful request never
   reached the container root" - **verified** at those lines, along with a
   `_NON_TERMINATING_SIGNALS` set (`SIGSTOP`, `SIGCONT`, `SIGTSTP`, `SIGTTIN`, `SIGTTOU`, `SIGCHLD`,
   `SIGURG`, `SIGWINCH`, `SIGUSR1`, `SIGUSR2`) that is delivered as asked and never escalated, so
   `kill -s STOP` cannot become a teardown by surprise. PDN's `kill` has the same problem and its own
   answer; the reference's is the one to compare against.
13. **A run lower in the stack is not a run lower in the argument order.** `cli.py:204` dispatches
   `HELP_COMMANDS` before argparse ever runs, so `help` works with no dependency resolution, and
   `parser.py:49` renders a command's own page on a parse error rather than a usage blob. Help is data:
   `pages.py:43 HELP_PAGES` rendered by `render.py:290`.

**One claim did not survive verification, and the reverse is true.** The proot-distro report stated that
`names.py:40` "allows a space/newline in a container name". The line is
`_NAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.\-]*\Z")` - a strict rule, with `NAME_RULE_HINT` spelling
it out for users. The reference is *stricter* here than the report claimed, and stricter than PDN needs to
be; the claim is recorded as refuted rather than folded into this document's findings. Every other claim
the parent checked held at the cited line, which is the only reason the rest are quoted as findings rather
than as a summary of a summary.

## 5. The round-1 adoption list, with its acceptance tests (condensed)

Full table with each test in `research/PROOT_AND_PROOTDISTRO_SOURCE_DISSECTION.md` section 3. Status is
current as of this document:

| # | take | from | acceptance test | status |
|---|---|---|---|---|
| 1 | digest validation before any path is composed from it | `docker/cache.py:validate_digest` | a crafted digest with `..` is refused with the offending value quoted | open |
| 2 | origin-bound credentials; refuse HTTPS→HTTP redirects | `docker/transport.py`, `download.py:check_redirect` | a same-host downgrade drops the header and the transfer fails | open |
| 3 | retry policy: transient yes, deterministic no, `Retry-After`, declared length | `helpers/download.py` | a 404 is not retried; a truncated body fails with both lengths | open |
| 4 | lock hints: the refusal names the holder | `locking.py:_lock_info_at` | a second `install` of the same name prints the first's PID | open |
| 5 | check whether `login` loses exported variables to `/etc/profile` | `login/env.py:inject_termux_profile` | **measured and closed**: PDN forwards no host variables at all (guest env is exactly `COLORTERM HOME LANG PATH TERM`), so the reference's bug class does not apply | **done (measured)** |
| 5b | retain and apply the image's own config (`Env`, `WorkingDir`, `Entrypoint`, `Cmd`) | `paths.py:container_image_config`, `login/env.py:image_env_pairs` | **measured gap**: no container metadata carries `Env`, and nothing in `crates/podnext/src` reads `image_config` | **filed as #76** |
| 6 | descriptor-based operations for everything that writes into a container tree | `dirfd.py` | a container with a symlink/FIFO planted at a target name makes the operation refuse, not follow | open (v0.3.0) |
| 7 | derive the alias from a reference; accept tarball / OCI layout / URL as install sources | `install.py`, `install_local.py`, `derive_alias` | §10.1's four acceptance rows | open (v0.3.0) |
| 8 | the Android bind set on `login`, with the termux-type exclusion | `login/bindings.py`, `login/proot_cmd.py` | `/sdcard` in the guest is the host's storage; a termux-type guest does not see the app's private dirs | open (v0.3.0) |
| 9 | reap instead of count: the launcher is a subreaper | `run_step.py:_become_subreaper` | the soak harness reports zero leftovers, and an explicit list when it does not | open (v0.3.0) |
| 10 | a build emits the thin layer its WORKDIR needs; RUN steps are recipe-hash cached | `handlers.py:do_workdir`, `run_step.py:do_run` | a built image's WORKDIR exists at run time; a repeat build hits the cache | open (v0.3.0) |
| 11 | cache eviction by reference count | `clear_cache.py` | `clear-cache --orphan` removes exactly the unreferenced blobs, counted before and after | open (v0.3.0) |
| 12 | a syscall-level probe corpus mined from proot's own tests | `tests/*.c` | probes for `open_nofollow`, `execveat`, `fchmodat2`, `getdents` run against PDN's guest | open (v0.3.0) |
| 13 | document the inherited default bind `/dev/null` → `/etc/ld.so.preload` | `cli/proot.c:142` | `known-limitations.md` explains why that file is empty in a guest | open |

## 6. Defects this round found in PDN's own infrastructure (M)

Not dissection findings, but they came out of reading the comparators' release machinery and checking ours
against it, and each was verified at byte level rather than by reading a rendered page.

| defect | evidence | state |
|---|---|---|
| Release bodies flattened to one line (literal `\` `n` where the breaks were) | v0.2.0 body began `5c 6e 54 61 67` instead of `0a 54 61 67`; one real line for 8 KB | **fixed** (#75), published body repaired and re-read |
| Six releases carried the same defect, and the pattern was not uniform | v0.1.0-0.1.2, **v0.1.5**, **v0.1.8** escaped; v0.1.3/v0.1.4 clean because a *different* publisher used a YAML literal block | **v0.1.5 and v0.1.8 repaired**; v0.1.0-0.1.2 left, `research/RELEASE_NOTES_REPAIR.md` |
| A release whose body did not describe its tag | v0.1.7: 285 bytes claiming a completed roadmap, tag `a86aee2` = *Fix release workflow: revert to nttld/setup-ndk@v1* | **withdrawn** (release object deleted, tag kept) |
| A second tag-triggered publisher that could never succeed | `aarch64-linux-android-ranlib: not found` (exit 101) because `$GITHUB_PATH` was set in the same step as the build; the green cross-compile job does it in a preceding step | **removed** (#77), 124 deletions, one publisher remains, run `42125000536` green with 7 jobs |
| A false claim carried in three research documents | "`ci.yml` has no `workflow_dispatch`" - it has had one since `d35ec30`, and at `v0.1.8` | **corrected in place**, visibly |

## 7. Round 2: the deep dissections (status)

Each is an independent source reading with its own file, evidence markers and command log. Their findings
are folded into sections 4-5 and the backlog in section 8 as they land.

| dissection | source | file | status |
|---|---|---|---|
| proot, deep | `src/syscall` (25 files, `enter.c` 3,069 lines), `src/tracee` (13 files), the extension enable logic, `src/execve`, `src/loader` | `research/DISSECTION_PROOT_DEEP.md` | **landed, parent-verified** (6 claims checked at their cited lines) |
| proot-distro, deep | the argv parser, message/progress layer, state-dir and atomic/compress/shm helpers, and the commands `copy`, `sync`, `remove`, `kill`, `list`, `run`, `build`, `help` + completions | `research/DISSECTION_PROOTDISTRO_DEEP.md` | **landed, parent-verified** (one claim refuted; see section 4) |
| dockerd | `daemon/`, `cmd/dockerd/`, `libcontainerd/`, `api/types/`, `pkg/plugins/` | `research/DISSECTION_DOCKERD.md` | running |
| containerd | the supervisor/shim split, shim v2 (ttrpc, fds, stdio, exit status), content store + snapshotters with GC and leases, events and task lifecycle | `research/DISSECTION_CONTAINERD.md` | landed, verification pending |
| runc | the `runc init` re-exec, `state.json`, cgroup v2 layout, the namespace/mount/seccomp/capability order, exit-status propagation | `research/DISSECTION_RUNC.md` | running |

## 8. The consolidated next-dev list

Ordered by what unblocks the most, with the test that settles each. Round-1 items keep their numbers;
`N`-items are added as round 2 reports.

**Now (small, verifiable, no new subsystem)**

1. **Digest validation before path composition** (§5.1) - a choke point, one function, and it guards the
   store PDN will need for `push`/`export`.
2. **Origin-bound credentials and a refused downgrade** (§5.2) - the installer and the registry client both
   sit on this link; #74 already records the gap from the other side.
3. **A real retry policy** (§5.3) - replaces the hand-rolled three-attempt loop that was misdiagnosed as a
   missing artifact.
4. **Lock hints** (§5.4) - the refusal names the holder, which is the difference between a dead end and a
   diagnosis.
5. **Image config retention and `Env` application** (#76) - the smallest useful half of the config story.
6. **The inherited default bind documented** (§5.13) - one paragraph in `known-limitations.md`.

**Added by round 2 (small, each verifiable without a new subsystem)**

- **R1 - the three argv facts, documented.** The one default-on extension (`mountinfo`, and PDN never
  disables it), `--link2symlink` on by default, and `/dev/shm` as a syscall-level fiction with magic
  `0x01021994`. Acceptance: `known-limitations.md` states all three, and a test asserts the built argv
  contains `--link2symlink` and never `PROOT_NO_MOUNTINFO`.
- **R2 - a kill ladder, and a set of signals that are never escalated** (§4.12). Acceptance: a guest that
  ignores `TERM` is escalated `TERM` → `QUIT` → `KILL`, and `kill -s STOP` is delivered exactly as asked.
- **R3 - stdout discipline as a gate** (§4.9). Acceptance: on the JSON path stdout parses as JSON; on the
  human path stdout carries only the data channels, with every prompt and progress line on stderr.
- **R4 - help that stands alone** (§4.13). Acceptance: `podnext help <cmd>` works with an unreadable config,
  and a bad flag prints that command's own page rather than a generic usage blob.
- **R5 - atomic state writes with an identity check** (§4.10). PDN writes state files; the reference's
  `atomic_replace` refuses with `ESTALE` when the temporary file's `(st_dev, st_ino)` changed underneath it,
  and names the one race it cannot close. Acceptance: a write whose temporary file is replaced before the
  rename fails with `ESTALE` and leaves the previous content intact.

**v0.3.0 (each needs a subsystem or a rewrite)**

7. **The image store that keeps layer blobs** - the precondition for `push`/`pull`/`export` (#37, #39), with
   containerd's content store, leases and GC as the reference design (round 2, section 7).
8. **Descriptor-based file operations** (§5.6) - a rewrite of the paths that touch a container tree.
9. **Install from a tarball, an OCI layout and a URL, with a derived alias** (§5.7).
10. **The Android bind set on `login`** (§5.8).
11. **Reap instead of count** (§5.9) and **the WORKDIR thin layer with a recipe cache** (§5.10).
12. **Cache eviction by reference count** (§5.11).
13. **A syscall-level probe corpus** (§5.12).
14. **Session liveness from an inherited descriptor rather than a PID** (§4.11): the reference's session is
    live exactly while some process holds the descriptor it registered, and survivors are found by scanning
    `/proc/*/fd` for the inode. A PID can be reused; a held descriptor cannot.

**Explicitly not taken**

- **`checkpoint`** (Docker CLI has it, PDN has no counterpart): it needs daemon-level state capture that
  proot cannot provide. Recorded as refused with a reason, not as a maybe.
- **`--user` semantics**, refused today with a measurement behind the refusal (#59). The reference
  implements the lookup (`login/passwd.py:read_passwd_entry`, `find_passwd_by_uid`); whether PDN wants
  per-user containers is a product decision, not a gap.
