# Changelog

All notable changes to PDN (`podnext`) are recorded here. The format is
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The release of record is the version in `Cargo.toml` (currently `0.2.0`). Entries under
**Unreleased** have landed on `master` and are not yet tagged. Every entry names the commit
that carries it, so it can be re-derived with `git show <sha>`.

## [0.2.0] - 2026-11-06

Tag `v0.2.0`. 113 commits since `v0.1.8` (`git log --oneline v0.1.8..v0.2.0 | wc -l`) across 129 files, and
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
  (`fc6d288`; #65). The split is held to a byte-identical `--help` dump, so a moved screen is either a
  defect or a fix that has to be named line by line.
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
