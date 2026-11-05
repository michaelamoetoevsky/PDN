# Changelog

All notable changes to PDN (`podnext`) are recorded here. The format is
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The release of record is the version in `Cargo.toml` (currently `0.1.8`). Entries under
**Unreleased** have landed on `master` and are not yet tagged. Every entry names the commit
that carries it, so it can be re-derived with `git show <sha>`.

## [Unreleased]

Work on `master` after the `v0.1.8` tag (roadmap section 9, the v0.1.9 targets). Section 9 is
**not complete**: section 9.2 (the module split, plus cache and daemon routing) and section 9.3
(terminal fidelity, detach/attach, the quoting table, a soak run) remain open.

### Added

- `podnext doctor` reports host checks (proot binary, temp dir, shared memory, DNS, kernel,
  cgroups) and, with `--container <name>`, six guest checks including the proot argv
  (`285720a`, `a6c4fa6`; #45, #46).
- CI runs the podnext suite with a relocated `HOME` and prefix, so the environment-variable
  work is proved in CI and not only on the dev device (`a6c4fa6`; #46).
- A pre-commit hook blocks unformatted Rust at commit time (`a9bf482`).

### Changed

- The CLI is split by domain (roadmap section 9.2). Extracted modules, each in its own commit:
  `commands/image.rs` (`17476f1`), `commands/volume.rs` (`181d2fe`), `commands/network.rs`
  (`472d9b3`), `commands/sync.rs` (`dc40a06`), `commands/container.rs` (`892549a`).
- Every command carries a `stable` / `experimental` label in `--help`, and every stub refuses
  with a named reason instead of printing a not-implemented note on stdout (`fcabc1c`; #42).
- The issue ledger is a real CI gate, outside `ci.yml`'s path filter, so a predicate change is
  always exercised against the tree it verifies (`234b370`).

### Fixed

- `--bind` and `--volume` reach the guest: the argument is read, and the `:rw` suffix that
  PRoot misread as part of the guest path is no longer rendered (`a29ee6d`; #43).
- `image ls` prints a measured size, or `>=` when subtrees could not be read, or `unknown`,
  instead of a directory `stat` presented as a size; the name column is `CONTAINER` (`8477181`;
  #44).
- `run` refuses the seven options it cannot honour, and `--help` no longer claims `--detach`
  runs in the background (`7960de8`; #47).
- Daemon path defaults follow `TERMUX__HOME` and `XDG_*` instead of hard-coded Termux paths
  (`285720a`; #46).
- One licence everywhere: `GPL-2.0-or-later`, declared identically in `LICENSE`, `Cargo.toml`
  and `README.md`, pinned by four ledger predicates (`3f8fb61`; #40).

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

[Unreleased]: https://github.com/michaelamoetoevsky/PDN/compare/v0.1.8...HEAD
[0.1.8]: https://github.com/michaelamoetoevsky/PDN/releases/tag/v0.1.8
