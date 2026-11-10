# Changelog

All notable changes to PDN, the `podnext` project, are recorded here. The format is [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The release of record is the version in `Cargo.toml`, currently `0.2.0`. The `unstable` tag names the archived state of the project: everything that landed on `master` after the `v0.2.0` release. Entries under **Unreleased** are not tagged at all. Every entry names the commit that carries it, so it can be re-derived with `git show <sha>`.

## [Unreleased]

_None yet._

## [unstable] - 2026-11-10

The archived state of the project. This tag carries everything that landed on `master` after the `v0.2.0` release. The owner stopped development on this date.

### Added

- The in-tree engine is the default for containers. `proot` is optional. `--engine <proot|native>` selects either one. `doctor` and the installer no longer require `proot`. A decoy arm proves that the native path starts no `proot` binary. The change is in `b07cd18`, `1fd91b0` and `6c19ae1`, and in `scripts/own_engine_acceptance.py`.
- `scripts/e2e_parity_battery.sh` runs 30 guest commands on a real Debian rootfs in both engines. It prints one verdict per case: EQUAL, DIFFERENT, BLOCKED or SUSPECT. It never touches the operator store. Each arm has a 30 second ceiling. The change is in `7d72edb` and `f1f0e9c`.
- `scripts/engine_differential.py` and `scripts/engine_write_parity.py` compare one workload in two arms. They report each field separately. The change is in `36ff688` and `b8d3abf`.
- The engine translates the two-path calls, with one scratch slot for each argument. Thus `mv` across two guest paths works. The change is in `292a4c1`.
- The engine tracks the guest working directory for each tracee. It stores the guest form of that directory. Thus `cd` inside the guest works, and `PWD` shows a guest path. The change is in `e4a617b`, `65237ae` and `a4da56c`.
- `--link2symlink` is implemented. A guest hardlink that the kernel refuses becomes a symlink and reports success. This behavior matches `proot`. The change is in `244da09`.
- The engine maps `st_uid` and `st_gid` in stat results. It answers `getuid`, `geteuid`, `getgid` and `getegid` with the mapped identity. `PDN_NATIVE_NO_ID_MAP=1` turns the mapping off for tests. The change is in `ee99ce4` and `dec84bd`.
- The engine forwards the guest's signals. It excludes the four signals that belong to the trace loop. The change is in `364922e`.
- The engine records the exit status of each descendant it reaps. It replays the `wait4` syscall. The change is in `b85cda2`, in part.
- `scripts/check_ste.py` measures a Markdown file against the mechanical part of ASD-STE100. It refuses to report a pass when it measured nothing.
- `research/ARCHIVE_HANDOVER.md` records the archived state, the defects that are closed, and the work that remains.
- `scripts/check_release_notes.sh` checks the file a release body is read from. It checks for real line breaks and no literal backslash-n token. `--self-test` proves the check discriminates. A clean copy passes. A copy that carries the escape that actually shipped fails. When both copies answer the same way, the test exits 2 and does not claim a verdict.
- `research/PROOT_AND_PROOTDISTRO_SOURCE_DISSECTION.md` is the dissection of both comparators' sources, read in full. `proot` has 138 C files and 32,638 lines, with a mechanically generated symbol index. `proot-distro` has 77 files and 29,015 lines, with every module and function docstring. The document states eight doctrines, each with a `file:function` anchor. It states thirteen items for PDN, with the acceptance test that settles each. It states what PDN already does as well or better. It states what was not read at statement level. The change is in `3870792`.

### Fixed

- The first syscall of a fork child was labelled as an exit. Thus the engine misread every `execve` of a child. `dpkg-query -W` counted 0 packages where `proot` counts 78. The fix sets the entry flag at the birth stop. The change is in `3f858c3`.
- Path translation ran at exit stops, where a register holds a result and not an argument. `unlinkat` and `utimensat` were refused for that reason. The change is in `246d027`.
- A NULL path or an empty path was treated as a failed translation. That behavior broke `futimens`. The change is in `9a5787a`.
- A descriptor target was matched against one spelling of the rootfs path only. The engine did not resolve it against the bind sources. `rm -rf` of a directory with content now removes it. The change is in `fd1466d`, `56aa7f7` and `af56dfa`.
- `getcwd` was read at the exit stop, where the return register holds a length. Thus the guest saw the host rootfs path in `PWD`. The engine reads the pointer at the entry stop and answers it in guest form. The change is in `a4da56c`.
- The bootstrap `chdir` exemption was keyed on a flag that is false on the engine's own child. The engine now sets the flag at the exec event. This change did not close `mkdir -p` and did not change the battery. Thus it stands as harmless and insufficient. The change is in `6b11aa1`.
- The workflow no longer flattens the release body on the way out. The workflow submitted the notes through an Actions output, which is line-oriented. Thus every newline became the two characters `\` `n`. The published `v0.2.0` notes arrived as 8,152 bytes on **one line**, and began `5c 6e` instead of `0a`. The notes now go to `release-notes.md`, and the release reads that file. The step no longer overwrites the repository's `CHANGELOG.md`. Two guards refuse a flattened or escaped body before the release is created. The change is in `03b7ddb`. See issue #75. The owner repaired the already-published body in place and re-read it to confirm the repair.
- The release notes for **v0.1.5** and **v0.1.8** were the same defect. Each body was one line, 3,854 and 4,021 bytes, with 49 literal backslash-n tokens. Those notes now describe what those tags are, and do not list commit subjects on a single line. **v0.1.7 has been withdrawn**. Its 285-byte body claimed a completed roadmap, but its tag is `a86aee2`, a CI repair commit, *Fix release workflow: revert to nttld/setup-ndk@v1*. The project keeps the tag, and only the release object is gone. The before-and-after byte counts are in `research/RELEASE_NOTES_REPAIR.md`. That file also holds the two publishers that produced the difference, a `body: |` literal block in `ci.yml` and the escaped Actions output in `release.yml`. It holds the four items that the owner deliberately left alone.

### Changed

- Two `proot` facts that PDN had only inferred are now source-level. The change also corrects this project's own earlier reasoning. The guest-to-host mapping is a string prefix rewrite, from `path/path.c:substitute_path_prefix`, reached from `path/binding.c:substitute_binding`. **No read-only bind exists** anywhere in the option surface. `add_induced_bindings()` only reconfigures. Thus the lookalike directory tree a guest sees is PDN's own making, as #66 measured, and not proot's. The change is in `3870792`.
- Three research documents carried a **false** claim that `ci.yml` has no `workflow_dispatch`. It has carried one since its first commit, `d35ec30`, and it still did at `v0.1.8`. See `git show v0.1.8:.github/workflows/ci.yml`, line 26. The path filter alone explains why a documentation-only push runs nothing. That filter supplies a better remedy than the one those documents recommended: dispatch the workflow by hand on the release commit, so its own artifact exists. Each correction is visible in place, and the owner did not rewrite it silently.
- **Containers now run on PodNext's own engine by default.** `--engine` changed from `proot` to `native`, so a run needs no `proot` binary at all. `--engine proot` keeps the previous path byte for byte. The acceptance harness `scripts/own_engine_acceptance.py` flipped from PARTIAL to PASS. It now also proves the claim the way the requirement words it. `PROOT_BIN` points at a decoy that writes a marker if anything ever executes it, and PASS requires the marker to stay absent. The harness ran on a real Debian 13 rootfs with the same fixture and the same binary. In that run, `ls /etc`, `test -x /bin/ls`, `command -v ls`, `exec /bin/ls`, a pipeline, `apt-get --version` and `ls /usr/bin | wc -l` all match the proot arm, at 257 = 257. The change is in `b07cd18` and `1fd91b0`.
- **`proot` is optional and is no longer installed behind the operator's back.**
  `podnext doctor` warns and does not fail when the binary is absent, because only `--engine proot` needs it. `install.sh` installs neither `proot` nor `proot-distro`, unless `PDN_WITH_PROOT=1` is set. `proot-distro` is the reference implementation, a different program with its own container store. That difference explains why a command that looks like PDN's answered "container is not installed" about a store PDN never reads. The change is in `1fd91b0`.
- **`podnext login` refuses the unimplemented proot flags by name, with a remedy.** It does not answer clap's `error: unexpected argument ... found`. The refused flags are `--shared-home`, `--shared-tmp`, `--shared-x11`, `--termux-home`, `--detach`, `--fix-low-ports` and `--no-link2symlink`. Each message names the flag, the reason and a command that works. A genuine typo still fails as a parse error. `--kill-on-exit`, `--no-kill-on-exit` and `--no-sysvipc` are not duplicated, because they are already global flags refused by name. The change is in `00cb46f`.
- `docs/src/engines.md` documents the engine choice in four parts. It states plainly what each engine does not do, with a "two programs, two stores" table. `docs/src/known-limitations.md` now lists the measured engine gaps. The gaps are these. The identity maps to zero and is switchable with `PDN_NATIVE_NO_ID_MAP`. The engine does not rewrite `st_uid`. An `argv[0]` of spawned programs shows the guest interpreter. Five proot argv features are unimplemented. The guest `/` listing is the rootfs itself. The change is in `052a08a`.

### Removed

- The second, tag-triggered release job in `ci.yml`. The tag commit's `PDN CI/CD` run was **red** because of it. The run failed with `aarch64-linux-android-ranlib: not found` in the vendored OpenSSL build, exit 101. `scripts/preflight_release.sh` refuses a run whose conclusion is not `success`, so a permanently red job inside the tag run blocked the intended release path. The job was a duplicate publisher, because it ends in `softprops/action-gh-release`, as `release.yml` does. Its body was hard-coded boilerplate that replaces the curated notes if the job runs. Its binaries were a second build path that preflight does not verify. Nothing in the repository referenced it or the tarballs it built. The change removed 124 lines and added nothing, and one publisher remains. See `3ae1d37` and issue #77.

### Archived

- The owner stopped the project on 2026-11-10 and tagged this state `unstable`. The documentation of this repository was rewritten to ASD-STE100 Simplified Technical English in the same change.
- The engine is not a complete replacement for `proot` when this tag was made. Two ordinary guest workloads still differ: a nested `mkdir -p` loses a component, and `wait` does not return a background job. Section 5 of `research/ARCHIVE_HANDOVER.md` records the measured mechanism of each one.
- The binary that the owner installs on this device is still version `0.1.7` and still runs the external `proot` binary. The archive note in `research/ARCHIVE_HANDOVER.md` gives the command that installs the release build.

## [0.2.0] - 2026-11-06

Tag `v0.2.0`. The tag has 123 commits since `v0.1.8`, from `git log --oneline v0.1.8..v0.2.0 | wc -l`, across 138 files. The scope is roadmap section 9.

This release is a stability, honesty and shape release, and not a feature release. Exactly **one** command was added, `doctor`. The substance is that the commands already present now do what they say and say what they do. Gates that can fail hold them to it. Where measurement replaced a claim, the file records the number. That number includes the one criterion that ends the release as a **recorded exception** rather than met.

**Renumbering note.** The mobility polygon used to carry the `v0.2.0` number. It is now roadmap section 10 = `v0.3.0`, because `v0.1.9` was tagged and withdrawn before any release existed. This work needed a number of its own. Nothing in the polygon changed except its number.

### Added

- `podnext doctor` reports host checks. The host checks are the proot binary, the temp directory, shared memory, DNS, the kernel and cgroups. With `--container <name>` it also reports the guest side, which includes the proot argv. Its assertions are about the verdict-to-status relationship, and not a fixed exit status. The change is in `285720a`, `a6c4fa6` and `d1df756`. See issues #45 and #46.
- `run --detach` and `attach` are a real round trip. The same shell pid answers two successive attaches. A marker written while detached survives the reconnection. A SIGKILL of the supervisor leaves no orphan. The change is in `9076b93`. See issue #57. Two simplifications are declared rather than hidden. There is no `SIGWINCH` re-forwarding while attached, and no replay of output produced while detached.
- Guest bindings and the `USER` story are measured **inside a guest**, with a control that goes red when falsified. `/sdcard` and `/storage/*` are the guest's own empty rootfs directories. `/tmp` and `/dev/shm` are per-container. The host home is not mounted. The change is in `4d51ea5`. See issue #66.
- A harness that runs in CI executes every fenced example in `docs/src/cli-reference.md`, or excludes it with a printed reason. The harness found two genuinely wrong examples. The change is in `4d2fa28` and `aa1bb70`. See issue #64.
- Terminal fidelity and repeated-session soak are measured and not assumed. The argv quoting table is asserted against a real guest, NUL included. The changes are in `eab10ce` and `5c34115`. See issues #56 and #58.
- The 123-screen `--help` dump is a **tracked fixture** compared in CI. The gate prints what it verified when it passes, and does not stay silent. The changes are in `1c9a778` and `48cd242`.
- The warning count is a ratchet with ceilings keyed by toolchain. The ratchet refuses growth and allows a fall. The change is in `71e028d`, `8ef07a5` and `bcae820`.
- Re-runnable release tooling: `scripts/local_gate.sh` at `68ab837`, `scripts/smoke_release.sh` at `54d3f7f`, and `scripts/extract_changelog.sh` at `e64bb47`.
- A pre-commit hook blocks unformatted Rust at commit time. CI runs the suite with a relocated `HOME` and prefix. The changes are in `a9bf482` and `a6c4fa6`. See issue #46.
- The issue ledger is a coverage gate. Every audit finding carries a predicate or a recorded disposition. An untriaged issue fails the build. The change is in `aa3d2b0`.

### Changed

- **Shape.** `main.rs` 4,679 → 479 and the whole Clap surface moved into `cli.rs`, then into eight `cli/` domain modules. The change is in `a455d10`, `17476f1`, `181d2fe`, `472d9b3`, `dc40a06`, `892549a`, `17ed375`, `82122eb` and `9be459a`. `commands/mod.rs` 3,719 → 813, in `d8f84d5`. `cli.rs` 1,450 → 687 and `main.rs` 1,170 → 479, in `fc6d288`. See issue #65. The last file above the ceiling was `native.rs` at 1,605, grown from 1,151 by the detach/attach work, so this project's own doing. The project split that file along the OCI-install versus container-session seam already present in it. The new files are `native/image.rs` 579, `native/session.rs` 623 and `native/meta.rs` 184. A 261-line module root holds the new files. The change is in `eaf3e2d`. See issue #72. The largest file in `crates/podnext/src` is now `commands/mod.rs` at **813**. It is the module root, where re-exports live by necessity, 13 lines over a `~`800 guideline. The project states this and does not smooth it. Both splits are held to a byte-identical `--help` dump. Thus a moved screen is either a defect or a fix that has to be named line by line.
- Six metadata queries are routed through `podnext-daemon` with an empty diff against the in-process path and constant round trips. The queries are `list`, `info` and `ps`, in table and JSON form. The change is in `83827a1`. **Latency independence is a recorded exception**. At ten containers the daemon path is slower. Its in-process p50 is 5.20 ms against 1.48 ms. It still grows with the container count. Freshness revalidates each `metadata.json` over IPC, which causes that growth. An O(1) freshness key will serve in-place rewrites stale, which is a regression against the index cache. Thus the project leaves the criterion unmet on purpose, with the number beside it.
- The container index is cached, with mtime invalidation proved in both directions. With ten containers the in-process p50 is **1.3 to 2.2 ms**, against a criterion of under 10 ms. The change is in `cf3f487`.
- Every command carries a `stable` / `experimental` label and a domain in `--help`. Every stub refuses with a named reason and a non-zero exit, and does not print a not-implemented note on stdout. The change is in `fcabc1c`. See issue #42.
- `login --user` and `run --user` refuse, with the measurement behind the refusal. `run` and `login` state the mapped user on stderr, while stdout stays empty. `config validate` exits the documented `2`, and not the phantom `3`. The change is in `5e4f24d`.
- Daemon path defaults follow `TERMUX__HOME`, `PREFIX` and `XDG_*`, and not hard-coded Termux paths. CI proves the relocation. The change is in `285720a` and `a6c4fa6`. See issue #46.
- Ledger predicates are directory-scoped, and not pinned to a file. Thus a module split cannot silently invalidate the check that guards it. The change is in `0bde427` and `0a78b26`.

### Fixed

- `--bind` and `--volume` reach the guest. The engine reads the argument. The engine no longer renders the `:rw` suffix that PRoot misread as part of the guest path. The change is in `a29ee6d`. See issue #43.
- `image ls` prints a measured size. It prints `>=` when it cannot read subtrees, or `unknown`. It does not present a directory `stat` as a size. The name column is `CONTAINER`. The change is in `8477181`. See issue #44.
- `run` refuses the seven options it cannot honour. `--help` no longer claims that `--detach` runs in the background. The change is in `7960de8`. See issue #47.
- OCI whiteouts are obeyed when layers are extracted. The change is in `aa3d2b0`.
- A conflict marker that had been committed inside a constant, in `11338ed`. See issue #70. The same wrong temp-base expression had been copied into three scripts, in `dcefff7`. See issue #71.
- An interactive guest stays in the terminal it was given. A session no longer lands in a background process group where reading stdin raises `SIGTTIN`. The engine forwards `TERM`. The change is in `9886a29`. See issues #68 and #56.
- The non-UTF8 argv panic, in `e08ff48`. See issue #41. One licence is declared the same way in `LICENSE`, `Cargo.toml` and `README.md`, pinned by four predicates. The change is in `3f8fb61`. See issue #40.
- Volume and network name validation covers these items. It covers path traversal, in `356e5f8`. It covers control characters and non-printable output, in `703f151`. It covers mountpoint cleanup on `rm`, in `2ca711b`. It covers non-zero exit propagation, in `a59f0f5`. It covers empty or invalid names, in `5afaa8d`.
- `--seccomp` and `--no-kill-on-exit` refuse with a named reason. The engine does not accept and ignore them. The change is in `5d85ec1` and `255d25a`. See issues #60 and #63.
- Interactive prompts and progress messages go to stderr, in `41768a6`. `--json` emits pure JSON on stdout, in `f6329c5`. The HTTP client gets a 60-second default timeout, in `ebffba0`. An unset `HOME` says so, and does not fail with an OS error 13, in `ae23633`.

### Removed

- Nine one-shot commit scripts. They staged a fixed path list, committed it under an old subject and pushed straight to `master` with a token in the URL. The change is in `28dba29`.

### Deferred

- The mobility polygon is roadmap section 10, now `v0.3.0`. It holds alias-deriving installs. It holds install from a local tarball or an OCI layout. It holds install from a URL with a checksum. It holds multi-arch through the emulator. It holds the wider `login` polygon and portable export/import. It holds registry `push`/`pull`. The wider `login` polygon holds `--shared-tmp`, `--shared-home`, `--termux-home`, `--work-dir`, `--fix-low-ports`, `--redirect-ports`, `--no-link2symlink` and `--get-proot-cmd`. The registry `push`/`pull` item is blocked until the image store retains layer blobs. See issues #37 and #39.

## [0.1.8] - 2026-11-05

Tag `v0.1.8` is commit `a62254f`, subject `release: 0.1.8`. The tag has 130 commits since `v0.1.7`, from `git log --oneline v0.1.7..v0.1.8 | wc -l`.

### Added

- A TOML configuration loader for `pn-gateway`, and `podnext config validate` as its load site. The change is in `ab66048` and `6ab17fb`. See issue #27.
- `build local` does wildcard expansion for `COPY` / `ADD` within the build context, in `2366193`. It records `EXPOSE` / `USER` in image metadata, in `caecf56`. See issue #30.
- `run` reinstates the image's `ENV` / `WORKDIR`. The change is in `d562eb6`.

### Changed

- The issue ledger became a full-coverage gate with verdicts for the gated checks. It is wired into CI outside `ci.yml`'s path filter. The change is in `7a05c6f`, `234b370` and `9bb5fe7`.
- The documentation drift guard covers every documentation file, in `54aed46`. See issue #27. The fork-PR benchmark is reported from a `workflow_run` workflow, in `adcf76b`. See issue #36.

### Fixed

- Container detection is metadata-based, and not an `etc`/`bin`/`usr` guess. The change is in `2c6bea4`. See issue #38.
- `podnext image list` works, and `image inspect` finds what `build local` built. The change is in `4a125f3` and `d3e6ae6`. See issues #34 and #35.
- A tag is not a path component, in `546bf24`. See issue #35. Durations written as documented load, in `e951948`. See issue #34.
- `--json` emits pure JSON on stdout for `| jq`. The change is in `710eda4`.

[Unreleased]: https://github.com/michaelamoetoevsky/PDN/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/michaelamoetoevsky/PDN/releases/tag/v0.2.0
[0.1.8]: https://github.com/michaelamoetoevsky/PDN/releases/tag/v0.1.8
