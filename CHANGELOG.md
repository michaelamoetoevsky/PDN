# Changelog

All notable changes to PDN (`podnext`) are recorded here. The format is
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The release of record is the version in `Cargo.toml` (currently `0.2.0`). The `unstable` tag names the
archived state of the project: everything that landed on `master` after the `v0.2.0` release. Entries under
**Unreleased** are not tagged at all. Every entry names the commit that carries it, so it can be re-derived
with `git show <sha>`.

## [Unreleased]

_Nothing yet._

## [unstable] - 2026-11-10

The archived state of the project. This tag carries everything that landed on `master` after the `v0.2.0`
release, and the owner stopped development on this date.

### Added

- The in-tree engine is the default for containers, and `proot` is optional. `--engine <proot|native>` selects either one, `doctor` and the installer no longer require `proot`, and a decoy arm proves that the native path starts no `proot` binary (`b07cd18`, `1fd91b0`, `6c19ae1`, `scripts/own_engine_acceptance.py`).
- `scripts/e2e_parity_battery.sh` runs 30 guest commands on a real Debian rootfs in both engines and prints one verdict per case: EQUAL, DIFFERENT, BLOCKED or SUSPECT. It never touches the operator store, and each arm has a 30 second ceiling (`7d72edb`, `f1f0e9c`).
- `scripts/engine_differential.py` and `scripts/engine_write_parity.py` compare one workload in two arms and report each field separately (`36ff688`, `b8d3abf`).
- The engine translates the two-path calls, one scratch slot per argument, so `mv` across two guest paths works (`292a4c1`).
- The engine tracks the guest working directory for each tracee and stores its guest form, so `cd` inside the guest works and `PWD` shows a guest path (`e4a617b`, `65237ae`, `a4da56c`).
- `--link2symlink` is implemented. A guest hardlink that the kernel refuses becomes a symlink and reports success, the way `proot` behaves (`244da09`).
- The engine maps `st_uid` and `st_gid` in stat results and answers `getuid`, `geteuid`, `getgid` and `getegid` with the mapped identity. `PDN_NATIVE_NO_ID_MAP=1` turns the mapping off for tests (`ee99ce4`, `dec84bd`).
- The engine forwards the guest's signals, excluding the four that belong to the trace loop (`364922e`).
- The engine records the exit status of each descendant it reaps and replays the `wait4` syscall (`b85cda2`, partial).
- `scripts/check_ste.py` measures a Markdown file against the mechanical part of ASD-STE100, and it refuses to report a pass when it measured nothing.
- `research/ARCHIVE_HANDOVER.md` records the archived state, the defects that are closed, and the work that remains.

- `scripts/check_release_notes.sh` checks the file a release body is read from - real line breaks, and no
  literal backslash-n token - and `--self-test` proves the check discriminates: a clean copy passes, a copy
  carrying the escape that actually shipped fails, and when both answer the same way it exits 2 instead of
  claiming a verdict.
- `research/PROOT_AND_PROOTDISTRO_SOURCE_DISSECTION.md`, the dissection of both comparators' sources read
  in full: `proot` 138 C files / 32,638 lines with a mechanically generated symbol index, `proot-distro` 77
  files / 29,015 lines with every module and function docstring. It states eight doctrines with a
  `file:function` anchor for each, thirteen items for PDN with the acceptance test that settles each, what
  PDN already does as well or better, and what was not read at statement level (`3870792`).

### Fixed

- The first syscall of a fork child was labelled as an exit, so the engine misread every `execve` of a child. `dpkg-query -W` counted 0 packages where `proot` counts 78; the fix sets the entry flag at the birth stop (`3f858c3`).
- Path translation ran at exit stops, where a register holds a result instead of an argument. `unlinkat` and `utimensat` were refused for that reason (`246d027`).
- A NULL or an empty path was treated as a failed translation, which broke `futimens` (`9a5787a`).
- A descriptor target was matched against one spelling of the rootfs path only, and it was not resolved against the bind sources. `rm -rf` of a directory with content now removes it (`fd1466d`, `56aa7f7`, `af56dfa`).
- `getcwd` was read at the exit stop, where the return register holds a length, and the guest saw the host rootfs path in `PWD`. The pointer is read at the entry stop and answered in guest form (`a4da56c`).
- The bootstrap `chdir` exemption was keyed on a flag that is false on the engine's own child. The flag is now set at the exec event. This change did not close `mkdir -p` and did not change the battery, so it stands as harmless and insufficient (`6b11aa1`).

- The release body is no longer flattened on the way out. The workflow submitted the notes through an
  Actions output, which is line-oriented, so every newline became the two characters `\` `n`: the published
  `v0.2.0` notes arrived as 8,152 bytes on **one line**, beginning `5c 6e` instead of `0a`. The notes now
  go to `release-notes.md` and the release reads that file, the step no longer overwrites the repository's
  `CHANGELOG.md`, and two guards refuse a flattened or escaped body before the release is created
  (`03b7ddb`; #75). The already-published body was repaired in place and re-read to confirm it.
- The release notes for **v0.1.5** and **v0.1.8** were the same defect - one line, 3,854 and 4,021 bytes
  with 49 literal backslash-n tokens each - and now describe what those tags are instead of listing commit
  subjects on a single line. **v0.1.7 has been withdrawn**: its 285-byte body claimed a completed roadmap
  while its tag (`a86aee2`) is a CI repair commit, *Fix release workflow: revert to nttld/setup-ndk@v1*; the
  tag is kept and only the release object is gone. Before-and-after byte counts, the two publishers that
  produced the difference (a `body: |` literal block in `ci.yml` versus the escaped Actions output in
  `release.yml`), and the four items deliberately left alone are in `research/RELEASE_NOTES_REPAIR.md`.

### Changed

- Two `proot` facts that PDN had only inferred are now source-level, and one correction of this project's
  own earlier reasoning: the guest-to-host mapping is string prefix rewriting
  (`path/path.c:substitute_path_prefix`, reached from `path/binding.c:substitute_binding`), **no read-only
  bind exists** anywhere in the option surface, and `add_induced_bindings()` is reconfiguration-only - so
  the lookalike directory tree a guest sees is PDN's own making, as #66 measured, not proot's (`3870792`).

- Three research documents carried a **false** claim that `ci.yml` has no `workflow_dispatch`. It has carried
  one since its first commit (`d35ec30`), and still did at `v0.1.8` (`git show v0.1.8:.github/workflows/ci.yml`,
  line 26). The path filter alone explains why a documentation-only push runs nothing, and it supplies a better
  remedy than the one those documents recommended: dispatch the workflow by hand on the release commit, so its
  own artifact exists. Each correction is visible in place rather than rewritten silently.

- **Containers now run on PodNext's own engine by default.** `--engine` changed from `proot` to `native`, so a
  run needs no `proot` binary at all; `--engine proot` keeps the previous path byte for byte. The acceptance
  harness `scripts/own_engine_acceptance.py` flipped from PARTIAL to PASS and now also proves the claim the way
  the requirement words it: `PROOT_BIN` points at a decoy that writes a marker if anything ever executes it,
  and PASS requires the marker to stay absent. Measured on a real Debian 13 rootfs with the same fixture and
  the same binary, `ls /etc`, `test -x /bin/ls`, `command -v ls`, `exec /bin/ls`, a pipeline,
  `apt-get --version` and `ls /usr/bin | wc -l` (257 = 257) all match the proot arm (`b07cd18`, `1fd91b0`).

- **`proot` is optional and is no longer installed behind the operator's back.** `podnext doctor` warns instead
  of failing when the binary is missing, because only `--engine proot` needs it, and `install.sh` installs
  neither `proot` nor `proot-distro` unless `PDN_WITH_PROOT=1` is set. `proot-distro` is the reference
  implementation - a different program with its own container store - which is why a command that looks like
  PDN's answered "container is not installed" about a store PDN never reads (`1fd91b0`).

- **`podnext login` refuses the unimplemented proot flags by name, with a remedy,** instead of answering clap's
  `error: unexpected argument ... found`: `--shared-home`, `--shared-tmp`, `--shared-x11`, `--termux-home`,
  `--detach`, `--fix-low-ports`, `--no-link2symlink`. Each message names the flag, the reason and a command
  that works; a genuine typo still fails as a parse error, and `--kill-on-exit`, `--no-kill-on-exit` and
  `--no-sysvipc` are not duplicated because they are already global flags refused by name (`00cb46f`).

- `docs/src/engines.md` documents the engine choice in four parts and states plainly what each engine does not
  do, with a "two programs, two stores" table, and `docs/src/known-limitations.md` now lists the measured
  engine gaps (identity mapped to zero and switchable with `PDN_NATIVE_NO_ID_MAP`, `st_uid` not rewritten,
  `argv[0]` of spawned programs showing the guest interpreter, the five unimplemented proot argv features, and
  the guest `/` listing being the rootfs itself) (`052a08a`).

### Removed

- The second, tag-triggered release job in `ci.yml`. The tag commit's `PDN CI/CD` run was **red** because of
  it - `aarch64-linux-android-ranlib: not found` in the vendored OpenSSL build, exit 101 - and since
  `scripts/preflight_release.sh` refuses a run whose conclusion is not `success`, a permanently red job inside
  the tag run blocked the intended release path. It was a duplicate publisher (it ends in
  `softprops/action-gh-release`, as `release.yml` does), its body was hard-coded boilerplate that would have
  overwritten the curated notes had it ever run, its binaries were a second build path that preflight does not
  verify, and nothing in the repository referenced it or the tarballs it built. 124 lines removed, nothing
  added, one publisher remains (`3ae1d37`; #77).

### Archived

- The owner stopped the project on 2026-11-10 and tagged this state `unstable`. The documentation of this repository was rewritten to ASD-STE100 Simplified Technical English in the same change.
- The engine is not a complete replacement for `proot` when this tag was made. Two ordinary guest workloads still differ: a nested `mkdir -p` loses a component, and `wait` does not return a background job. Section 5 of `research/ARCHIVE_HANDOVER.md` records the measured mechanism of each one.
- The binary that the owner installs on this device is still version `0.1.7` and still runs the external `proot` binary. The archive note in `research/ARCHIVE_HANDOVER.md` gives the command that installs the release build.

## [0.2.0] - 2026-11-06

Tag `v0.2.0`. 123 commits since `v0.1.8` (`git log --oneline v0.1.8..v0.2.0 | wc -l`) across 138 files, and
the scope is roadmap section 9.

This is a stability, honesty and shape release rather than a feature release: exactly **one** command was
added (`doctor`), and the substance is that the commands already present now do what they say, say what they
do, and are held to it by gates that can fail. Where measurement replaced a claim, the number is recorded —
including the one criterion that ends the release as a **recorded exception** rather than met.

**Renumbering note.** The mobility polygon used to carry the `v0.2.0` number; it is now roadmap section 10 =
`v0.3.0`, because `v0.1.9` was tagged and withdrawn before any release existed and this work needed a number
of its own. Nothing in the polygon changed except its number.

### Added

- `podnext doctor` reports host checks (proot binary, temp directory, shared memory, DNS, kernel, cgroups)
  and, with `--container <name>`, the guest side including the proot argv; its assertions are about the
  verdict-to-status relationship, not a fixed exit status (`285720a`, `a6c4fa6`, `d1df756`; #45, #46).
- `run --detach` and `attach` are a real round trip: the same shell pid answers two successive attaches, a
  marker written while detached survives the reconnection, and a SIGKILL of the supervisor leaves no orphan
  (`9076b93`; #57). Two simplifications are declared rather than hidden: no `SIGWINCH` re-forwarding while
  attached, and no replay of output produced while detached.
- Guest bindings and the `USER` story are measured **inside a guest**, with a control that goes red when
  falsified: `/sdcard` and `/storage/*` are the guest's own empty rootfs directories, `/tmp` and `/dev/shm`
  are per-container, and the host home is not mounted (`4d51ea5`; #66).
- Every fenced example in `docs/src/cli-reference.md` is executed, or excluded with a printed reason, by a
  harness that runs in CI (`4d2fa28`, `aa1bb70`; #64) — it found two genuinely wrong examples.
- Terminal fidelity and repeated-session soak are measured, not assumed (`eab10ce`; #56, #58), and the argv
  quoting table is asserted against a real guest, NUL included (`5c34115`).
- The 123-screen `--help` dump is a **tracked fixture** compared in CI (`1c9a778`), and the gate prints what
  it verified when it passes instead of staying silent (`48cd242`).
- The warning count is a ratchet with ceilings keyed by toolchain, so growth is refused while a fall is
  allowed (`71e028d`, `8ef07a5`, `bcae820`).
- Re-runnable release tooling: `scripts/local_gate.sh` (`68ab837`),
  `scripts/smoke_release.sh` (`54d3f7f`), `scripts/extract_changelog.sh` (`e64bb47`).
- A pre-commit hook blocks unformatted Rust at commit time (`a9bf482`), and CI runs the suite with a
  relocated `HOME` and prefix (`a6c4fa6`; #46).
- The issue ledger is a coverage gate: every audit finding carries a predicate or a recorded disposition,
  and an untriaged issue fails the build (`aa3d2b0`).

### Changed

- **Shape.** `main.rs` 4,679 → 479 and the whole Clap surface moved into `cli.rs`, then into eight `cli/`
  domain modules (`a455d10`, `17476f1`, `181d2fe`, `472d9b3`, `dc40a06`, `892549a`, `17ed375`, `82122eb`,
  `9be459a`); `commands/mod.rs` 3,719 → 813 (`d8f84d5`); `cli.rs` 1,450 → 687 and `main.rs` 1,170 → 479
  (`fc6d288`; #65). The last file above the ceiling, `native.rs` at 1,605 - grown from 1,151 by the
  detach/attach work, so this project's own doing - was split along the OCI-install versus container-session
  seam already present in it into `native/image.rs` 579, `native/session.rs` 623 and `native/meta.rs` 184,
  behind a 261-line module root (`eaf3e2d`; #72). The largest file in `crates/podnext/src` is now
  `commands/mod.rs` at **813** - the module root, where re-exports live by necessity, 13 lines over a `~`800
  guideline; it is stated rather than smoothed. Both splits are held to a byte-identical `--help` dump, so a
  moved screen is either a defect or a fix that has to be named line by line.
- Six metadata queries (`list` / `info` / `ps`, table and JSON) are routed through `podnext-daemon` with an
  empty diff against the in-process path and constant round trips (`83827a1`). **Latency independence is a
  recorded exception**: at ten containers the daemon path is slower (in-process p50 5.20 ms against
  1.48 ms) and still grows with the container count, because freshness revalidates each `metadata.json`
  over IPC. An O(1) freshness key would serve in-place rewrites stale, which is a regression against the
  index cache, so the criterion is left unmet on purpose with the number beside it.
- The container index is cached with mtime invalidation proved in both directions: in-process p50
  **1.3–2.2 ms** with ten containers against a criterion of under 10 ms (`cf3f487`).
- Every command carries a `stable` / `experimental` label and a domain in `--help`, and every stub refuses
  with a named reason and a non-zero exit instead of printing a not-implemented note on stdout (`fcabc1c`;
  #42).
- `login --user` and `run --user` refuse with the measurement behind the refusal, and `run`/`login` state
  the mapped user on stderr while stdout stays empty; `config validate` exits the documented `2` instead of
  the phantom `3` (`5e4f24d`).
- Daemon path defaults follow `TERMUX__HOME`, `PREFIX` and `XDG_*` instead of hard-coded Termux paths, and
  the relocation is proved in CI (`285720a`, `a6c4fa6`; #46).
- Ledger predicates are directory-scoped rather than pinned to a file, so a module split cannot silently
  invalidate the check that guards it (`0bde427`, `0a78b26`).

### Fixed

- `--bind` and `--volume` reach the guest: the argument is read, and the `:rw` suffix that PRoot misread as
  part of the guest path is no longer rendered (`a29ee6d`; #43).
- `image ls` prints a measured size, or `>=` when subtrees could not be read, or `unknown`, instead of a
  directory `stat` presented as a size; the name column is `CONTAINER` (`8477181`; #44).
- `run` refuses the seven options it cannot honour, and `--help` no longer claims `--detach` runs in the
  background (`7960de8`; #47).
- OCI whiteouts are obeyed when layers are extracted (`aa3d2b0`).
- A conflict marker that had been committed inside a constant (`11338ed`; #70), and the same wrong
  temp-base expression that had been copied into three scripts (`dcefff7`; #71).
- An interactive guest stays in the terminal it was given: a session no longer lands in a background process
  group where reading stdin raises `SIGTTIN`, and `TERM` is forwarded (`9886a29`; #68, #56).
- The non-UTF8 argv panic (`e08ff48`; #41), and one licence declared the same way in `LICENSE`,
  `Cargo.toml` and `README.md`, pinned by four predicates (`3f8fb61`; #40).
- Volume and network name validation: path traversal (`356e5f8`), control characters and non-printable
  output (`703f151`), mountpoint cleanup on `rm` (`2ca711b`), non-zero exit propagation (`a59f0f5`), and
  empty or invalid names (`5afaa8d`).
- `--seccomp` and `--no-kill-on-exit` refuse with a named reason instead of being accepted and ignored
  (`5d85ec1`, `255d25a`; #60, #63).
- Interactive prompts and progress messages go to stderr (`41768a6`), `--json` emits pure JSON on stdout
  (`f6329c5`), the HTTP client gets a 60-second default timeout (`ebffba0`), and an unset `HOME` says so
  instead of failing with an OS error 13 (`ae23633`).

### Removed

- Nine one-shot commit scripts that staged a fixed path list, committed it under an old subject and pushed
  straight to `master` with a token in the URL (`28dba29`).

### Deferred

- The mobility polygon (roadmap section 10, now `v0.3.0`): alias-deriving installs, install from a local
  tarball or OCI layout, install from a URL with a checksum, multi-arch through the emulator, the wider
  `login` polygon (`--shared-tmp`, `--shared-home`, `--termux-home`, `--work-dir`, `--fix-low-ports`,
  `--redirect-ports`, `--no-link2symlink`, `--get-proot-cmd`), portable export/import, and registry
  `push`/`pull` — the last of which is blocked until the image store retains layer blobs (#37, #39).

## [0.1.8] - 2026-11-05

Tag `v0.1.8` (`a62254f`, `release: 0.1.8`). 130 commits since `v0.1.7`
(`git log --oneline v0.1.7..v0.1.8 | wc -l`).

### Added

- A TOML configuration loader for `pn-gateway`, and `podnext config validate` as its load site
  (`ab66048`, `6ab17fb`; #27).
- `build local`: wildcard expansion for `COPY` / `ADD` within the build context (`2366193`), and
  `EXPOSE` / `USER` recorded in image metadata (`caecf56`; #30).
- `run` reinstates the image's `ENV` / `WORKDIR` (`d562eb6`).

### Changed

- The issue ledger became a full-coverage gate with verdicts for the gated checks, wired into
  CI outside `ci.yml`'s path filter (`7a05c6f`, `234b370`, `9bb5fe7`).
- The documentation drift guard covers every documentation file (`54aed46`; #27); the fork-PR
  benchmark is reported from a `workflow_run` workflow (`adcf76b`; #36).

### Fixed

- Container detection is metadata-based, not an `etc`/`bin`/`usr` guess (`2c6bea4`; #38).
- `podnext image list` works, and `image inspect` finds what `build local` built (`4a125f3`,
  `d3e6ae6`; #34, #35).
- A tag is not a path component (`546bf24`; #35); durations written as documented load
  (`e951948`; #34).
- `--json` emits pure JSON on stdout for `| jq` (`710eda4`).

[Unreleased]: https://github.com/michaelamoetoevsky/PDN/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/michaelamoetoevsky/PDN/releases/tag/v0.2.0
[0.1.8]: https://github.com/michaelamoetoevsky/PDN/releases/tag/v0.1.8
