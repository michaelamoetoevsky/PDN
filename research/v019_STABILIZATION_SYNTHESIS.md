# v0.1.9 Stabilization & Hardening Synthesis Report

**Status**: Eight remediation batches are integrated. The `v0.1.9` tag was withdrawn pending roadmap section 9
**Date**: November 5, 2026 WIB
**Integration head**: `ed7dc3f`. The v0.1.9 tag is withdrawn. The workspace version is back to `0.1.8`
**Baseline**: 57 empirically observed audit findings. These are 2 critical, 16 high, 22 medium, 12 low and 5 nit

---

## 1. Stream Status Overview

| Batch | Target Scope | Findings Covered | Worktree (branch) | State on `master` | Evidence |
| :---: | :--- | :--- | :--- | :--- | :--- |
| **Manual** | Critical C1 (argv non-UTF8 `#41`) & C2 (image pull honesty `01-1`) | 2 critical | host | merged & pushed (`e08ff48`) | CI runs 41951506268, 41951506196 — success |
| **A** | Storage layer validation & path traversal | 5 (`05-2`..`05-6`) | `wt-fix-a` (`fix/storage-validation`) | merged (`356e5f8`, `703f151`, `2ca711b`, `a59f0f5`, `5afaa8d`) | `names::store_key_tests`, `storage::tests` pass |
| **B** | Automation contract: JSON purity, prompts, exit codes | 6 (`04-1`..`04-6`) | `wt-fix-b` (`fix/automation`) | merged (`f6329c5`) | `crates/podnext/tests/automation_contract.rs` |
| **C** | Guest environment: isolation, truthful mounts, live DNS | 5 (`11-1`..`11-5`) | `wt-fix-c` (`fix/guest-env`) | merged (`04692dd`) | `network::tests`, `sysdata` bindings |
| **D** | Documentation honesty & phantom CLI deletion | 6 (`13-1`..`13-3`, `13-7`..`13-9`) | `wt-fix-d` (`fix/docs`) | merged (`4fe2455`) | CLI reference vs Clap tree |
| **E** | Install & image mechanics | 6 (`01-2`..`01-7`) | `wt-fix-e` (`fix/install`) | merged (`839b232`) | staging-directory install path |
| **F** | CLI surface & flag reconciliation | 7 (`13-4`..`13-6`, `13-10`..`13-13`) | `wt-fix-f` (`fix/cli`) | merged (`66e733b`) | Clap definitions + docs |
| **G** | Sysdata virtualization (`/proc`, `/sys`, `/dev`) | 6 (`11-6`..`11-11`) | `wt-fix-g` (`fix/proc-stubs`) | merged (`9dbce8e`) | `sysdata::tests` (5 checks) |
| **H** | Polish & defect mop-up | 11 (`04-7`..`04-12`, `01-8`..`01-10`) | `wt-fix-h` (`fix/polish`) | merged (`ebffba0`..`8bc2ef4`, 8 commits) | per-finding unit tests |

Twenty commits landed between `7cf3fd4` and the integration head: 18 originate in the
batches and 2 were written during integration. See section 3.

---

## 2. Verification Evidence Measured on the Android/Termux Target

| Command | Result |
| :--- | :--- |
| `cargo fmt --all -- --check` | exit 0, no diff |
| `cargo check -p proot-distro-core -j 1` | exit 0 (peak 455 MB, 8 s) |
| `cargo check -p podnext -j 1` | exit 0 (peak 1190 MB, 48 s) |
| `cargo test -p proot-distro-core` | **56 passed / 0 failed** (lib, 1.04 s) + 3 + 1 integration tests passed |
| `cargo test --workspace -j 1` | all binaries green: 4, 1 (258 s), 9, 9, 2, 58, 19, 32 tests passed, 0 failed — one stale assertion was fixed in `c3139bb` (see section 3.4) |
| `build_issue_ledger.py --offline --json` (source predicates) | 55 PASS / 0 FAIL / 4 SKIPPED of 59 checks |
| `build_issue_ledger.py --cargo` (full verdicts) | **59 PASS / 0 FAIL / 0 SKIPPED** of 59 checks |

The four cargo-gated behaviour checks `#16`, `#18`, `#25` and `#31` carry verdicts in the
`--cargo` run recorded above. The source-only run leaves them `SKIPPED`, and a `SKIPPED`
check is never a pass. The `Issue ledger` CI workflow, the required status check on this
branch, runs the same gate with `--cargo`.

`cargo test --workspace` runs on the device with `-j 1` under the memory guard. Release
builds and the full matrix remain the job of CI. A release build on the device exceeds the
2400 MB budget and the guard kills it with `BUILD_RC=137`.

---

## 3. Integration-Only Defects Invisible to Per-Batch CI

Per-batch verification cannot see these defects. They appear only when the branches meet,
or only on the target device. All four came from a run of the merged tree. All four are
fixed in `a52029f`, `2925626` and `c3139bb`.

1. **Duplicate symbols from a union-of-hunks conflict resolution, error `E0428`.**
   Batches C and G both edited `crates/proot-distro-core/src/sysdata.rs`. The conflict
   resolution kept both sides of every region. It left two definitions of `FAKE_MOUNTS` and
   two of `fake_vmstat`. The crate did not compile at all.
   Fix: keep the richer mount table with `/dev`, `/dev/shm` and `/tmp`. Keep the helper
   that reads `host_mem_free_kb()`. Drop the older duplicates.

2. **Borrow conflict in the proot spawn path, error `E0499`.**
   `crates/proot-core/src/lib.rs` acquired the command's mutable reference before
   `.spawn()`. Only visible once the umask/pgid work met the `pre_exec` hook.
   Fix: call `pre_exec` on `cmd` first, then chain `stdout`/`stderr`/`stdin`/`process_group(0)`
   into a single `.spawn()`.

3. **A read probe on `/dev/ptmx` blocks forever, so the test hung and did not fail.**
   `sysdata::bindings()` probes host paths with `readable()`, which opens the path and reads
   one byte. On a device where `/sys` and `/dev` cannot be listed, the `/dev` mask branch
   runs and probes the device nodes. A 1-byte read from a PTY master waits for data the
   slave never sends, so `bindings()` never returned:
   `sysdata::tests::ensure_writes_every_entry_then_bindings_cover_the_blocked_ones` stalled
   past 180 s, with a memguard peak of 86 MB and `--test-threads=1`. The CI Ubuntu runner
   lists `/dev` normally and skips the branch entirely.
   Fix: `readable()` returns true on metadata alone for a character device, so the guest
   still gets the real `/dev/null`, `/dev/zero`, `/dev/urandom` and `/dev/ptmx` bound and
   never reads from one.
   The same test then failed fast on a second, independent defect: it asserted that
   `/sys/fs/selinux` is always bound, but the two mask branches are mutually exclusive. On
   Android the whole `/sys` tree is masked, so the assertion now accepts either mask.

4. **A cross-crate assertion left stale by a new bind.**
   Batch G added a second bind to `proot_distro_core::guestdirs::bindings()` as
   `<container>/tmp:/tmp`. The guest then gets a writable `/tmp` from its own directory and
   not from the host directory. Batch G updated the tests inside `guestdirs`. The
   colon-free store-key test in `crates/podnext/src/commands/mod.rs`, a different crate,
   asserted that every bind ends with `:/dev/shm`. The merged tree failed with
   `unexpected bind: <tmpdir>/tmp:/tmp`. The code is right. The assertion now accepts
   either target and keeps its real intent: exactly one colon, and only directories PDN
   owns. Re-run: `cargo test -p podnext --bin podnext` → 32 passed / 0 failed.

Lesson: "does it compile" and "do the unit tests pass" are per-branch questions. The
integration tree needs its own compile, its own test run, and its own ledger pass before
release.

---

## 4. Integrated Behaviour Highlights

### 4.1 Critical Hardening and the Zero-Lie Engine
- **argv non-UTF8 sanitization for issue `#41`**: `std::env::args_os()` + `to_string_lossy()`
  replaces `std::env::args()`. The regression test is `crates/podnext/tests/argv_nonutf8.rs`.
- **Deceptive `image pull` neutralized for finding `01-1`**: it bails with exit 1 and points
  the user at `podnext install`. It does not fabricate a local container.

### 4.2 Storage and Name Hygiene for Batch A
- `validate_container_name` is the single validator behind container, volume and network
  names: `../`, control bytes, empty names and invalid delimiters are rejected.
- `volume rm` deletes the mountpoint directory and returns non-zero when a removal fails.

### 4.3 Guest Environment for Batches B, C and G
- The guest gets truthful `/proc/mounts` and `/etc/mtab`. The host PIDs are masked out of
  the guest's `/proc`.
- `/etc/resolv.conf` is written when a docker image ships none, so `apk update` resolves.
- `/sys` masked, `/proc/net`, `/proc/filesystems`, `/proc/cmdline` stubbed, `umask 022`.
- Interactive prompts and progress messages go to stderr, so `--format json` output stays
  parseable. A non-tty stdin without `--force`/`--yes` is an error, not a silent no-op.

### 4.4 Documentation Honesty for Batches D, F and H
- The fictitious `podnext storage` group is purged. The fictitious exit codes 3 to 12 are
  replaced by the real codes 0, 1, 2 and the child pass-through code. The phantom Clap
  options are removed from the reference.
- Top-level `--help` examples are standardized to English. Unimplemented build stubs are
  annotated and warned about on stderr.

---

## 5. Release Status: v0.1.9 WITHDRAWN and Roadmap Section 9 Is Not Met

The `v0.1.9` tag and GitHub release were created on 2026-11-05 and **withdrawn the same day**:
`gh release delete v0.1.9 --cleanup-tag`, the remote tag deleted, `releases/tags/v0.1.9` now
answers 404 and `v0.1.8` is Latest again. The version bump was reverted in `ed7dc3f`, so the
workspace version is back to `0.1.8`. The artifact claimed a milestone that is not
implemented.

Reason: `docs/src/v0.1.9-release-strategy.md` scopes v0.1.9 to the audit findings, while
`docs/src/roadmap.md` section 9 scopes it to stability **plus simplicity/performance plus
the interactive shell**. Section 9's own acceptance criteria are largely unmet:

| section 9 work item | Acceptance criterion | State at `ed7dc3f` |
|---|---|---|
| 9.1 labelling | every command labelled `stable` / `experimental`, in `--help` and in the docs | 0 occurrences of `experimental` in `main.rs` and `cli-reference.md` |
| 9.1 contract test | every command, flag and example in `cli-reference.md` exists and runs | no such test in the repo |
| 9.2 CLI split | no CLI file over ~800 lines | `main.rs` 4472, `commands/mod.rs` 3699 |
| 9.2 env-var paths | honour `TERMUX__HOME`, `TERMUX__PREFIX`, `TERMUX_APP__PACKAGE_NAME`, `XDG_*` | 0 occurrences in `crates/*/src` |
| 9.2 cached index | metadata query p50 < 10 ms with 10 containers | not implemented |
| 9.2 honest size | a real image size, or an explicit `unknown` | not implemented |
| 9.3 `doctor` | `podnext doctor` on the host and inside the guest | command absent |
| 9.3 shell | detach → attach round trip, `SIGWINCH`/`PS1`/`TERM`, quoting table, soak leaving no processes | not verified |
| release checklist | roadmap section 9 marked shipped, `known-limitations.md`, `CHANGELOG.md`, README "What's New" | `known-limitations.md` and `CHANGELOG.md` absent; section 9 unmarked |
| release checklist | manual smoke test: install alpine → run → login → remove | not run |

The withdrawal does **not** invalidate these facts. Section 9.1's triage arm is done and
verified:

1. C1/C2 plus batches A to H: 20 commits, ledger `--cargo` **59 PASS / 0 FAIL / 0 SKIPPED**
   of 59 checks. The four gated behaviour checks `#16`, `#18`, `#25` and `#31` now carry
   verdicts.
2. CI `14/14 success` on `be4919d` and on `0e00daf`. The jobs are `Rust Host Build`,
   `Fuzzing`, `Security Audit`, `Build Documentation`, `PRoot Distro Integration Test`,
   `Rust Android Cross-Compile (aarch64)`, four NDK cross-compile matrix jobs,
   `Run Benchmarks`, `Issue ledger`, `Aggregate Artifacts & Checksums` and
   `Report benchmark results`.
3. The script regenerated `research/ISSUE_LEDGER.md` at `64cfc8f`. The summary paragraph
   stays conditional on the skipped count.
4. The tag-gated `Release` job in `ci.yml` is repaired at `0e00daf`. The job never installed
   the Android Rust target, so its `Build release binaries (Android aarch64)` step died with
   `E0463`. The job stays **unobserved on the event it guards**: a `master` push does not run
   it. The next tag is the evidence.

Size note: the remainder is not a patch release. Section 9.3 is the interactive shell.
Section 9.2 is a CLI split and a cached index. The version after them is **v0.2.0**, from
roadmap section 10, with the OCI image store from section 4.4 as its spine. There is no
v0.1.10.

Lesson recorded in the skill: a tag is a claim about scope. Check the acceptance criteria of
the roadmap section and the release checklist before you create a public artifact. Do not
check them after the fact.

Next: the section 9 work items above, in this order. First 9.1 for the labels and the
contract test. Then 9.2 for the paths, the split and the index. Then 9.3 for `doctor` and
the shell. After that, the release checklist and a single v0.1.9 tag.

## 6. Where Section 9 Stands Now, Measured 2026-11-05 at Head `7960de8`

Section 5 records the withdrawal. This section is the live state, re-measured rather than
carried forward: every row below was produced by the command named in its evidence column,
on this device.

| section 9 work item | Acceptance criterion | State | Evidence |
|---|---|---|---|
| 9.1 labelling | no command unlabelled | **met** | `podnext --help` labels 38/38; clap's own `help` carries none, which the reference states; gated by `the_documented_stability_table_matches_the_binary` |
| 9.1 stubs | zero "not fully implemented" in user-facing output | **met** | `grep` of `crates/podnext/src` excluding comments: 0; `stub_refusals` 5/5 covers build subcommands, `watchdog events`, `login/runs --detach`, and the seven `run` options |
| 9.1 exit codes | a test asserts the code for each failure class | **partial** | 0/1/2 documented and the contract test rejects documented codes outside that set; `stub_refusals` pins 1; no single test walks a failure of each class |
| 9.1 contract test | every command, flag and example in the reference exists and runs | **partial** | `cli_reference_contract` 6/6 checks commands, options, stability labels, exit codes and env vars; the documented **examples** are not executed |
| 9.1 triage | every audit finding fixed, filed or declined with a reason | **partial** | 44 issues (29 closed, 15 open); 11 carry an explicit "no honest automated check, and why" in the ledger script; no single disposition table for the 13-agent audit |
| 9.2 CLI split | no CLI file over ~800 lines; `--help` byte-identical | **not met** | `main.rs` 4638, `commands/mod.rs` 3714 |
| 9.2 cached index | metadata query p50 < 10 ms with 10 containers | **not met** | no index cache exists (`grep` for one: 0) |
| 9.2 honest size | a real size, or an explicit `unknown` | **met** | `du -sb` is the measurement of record; a failed measurement prints `>= X` or `unknown`; 3 tests; issue #44 |
| 9.2 daemon routing | CLI latency independent of container count | **not met** | the socket is used for daemon control only; metadata queries still load containers in-process |
| 9.2 env paths | honour `TERMUX__*`/`XDG_*` instead of 12 hard-coded paths; suite passes with `HOME`/`PREFIX` relocated | **partial** | 11 hard-coded paths → 8 occurrences, and those are the documented defaults (`paths.rs`, `constants.rs`) and two uml4termux paths; resolvers + 5 unit tests + `relocated_paths` 2/2; the relocated-suite job is now a CI job (`Relocated HOME and prefix`); `TERMUX__PREFIX` is excluded because termux-exec owns it |
| 9.3 `detach`/`attach` | login → detach → attach returns to the same shell | **not met** | `--detach` refuses with a reason (honest, but not the feature) |
| 9.3 terminal fidelity | `vim`/`htop` render and resize correctly | **not met** | no `SIGWINCH`/`PS1`/`TERM` work or test |
| 9.3 argv quoting | a table of quoting cases passes | **not met** | 0 tests mention it |
| 9.3 bindings and USER | `/sdcard`/`/storage` and shared home/tmp documented and tested; build output states USER is not enforced | **partial** | `--volume`/`--bind` now reach the guest (issue #43) and `:ro` is refused with a reason; `podnext info` prints "User (recorded, not enforced)"; the storage presets have no test |
| 9.3 session hygiene | a soak run leaves no processes behind | **not met** | no soak test |
| 9.3 `doctor` | reports DNS, `/dev/shm`, `os-release`, cgroup, TMPDIR, **proot argv** with pass/fail | **met** | 12 checks on a real container, including `guest proot argv` (12,371 characters, 125 `--bind=` entries, elided in the table and whole in `--format json`) |
| 9.4 non-goals | no new top-level commands | **amended** | `doctor` is the 38th and section 9.3 requires it; the roadmap text now says so |
| release checklist | section 9 marked shipped, `CHANGELOG.md`, README "What's New", manual smoke test | **not met** | `CHANGELOG.md` absent; README unchanged; section 9 unmarked; smoke test not run |

Commits that moved these rows, each with the evidence that closed it:

| Commit | Change | Proof |
|---|---|---|
| `a29ee6d` | `--bind`/`--volume` reach the guest (issue #43); the rendered bind lost the `:rw` suffix PRoot turns into part of the target name | guest sees `MARKER-OK` via `--volume`, global `-b` and `login -b`; control run shows nothing; `:ro` exits 1; `research/PROOT_BIND_SUFFIX_MEASUREMENT.md` |
| `8477181` | `image ls` reports a measured size or says so; column renamed `CONTAINER` (issue #44) | `3.4 KB` for a 291 MB rootfs became `>=291.2 MB`; 3 tests |
| `285720a` | `podnext doctor` (issue #45) and environmental path defaults (issue #46) | 11 checks on a real container; `relocated_paths` 2/2, `host_path_tests` 5/5 |
| `d1df756` | the doctor test asserts the verdict-to-status relationship, not a fixed status | CI run `42020474716` failed 85/1 because a runner has no proot; test rewritten, ledger 86/86 |
| `5d75652` | "section" instead of the `§` sign (it reads as "ss") | 118 occurrences in 24 files; nothing depended on it |
| `7960de8` | `run` refuses the seven options it cannot honour (issue #47) | `run alpine --rm -- true` exits 1 naming the flag; `-t -i` still runs; `stub_refusals` 5/5 |

What the remainder still requires, in the order that least wastes work:

1. **section 9.2 CLI split**. This is the only item that blocks a release on its own. The
   work is mechanical, but it must keep `--help` byte-identical. The contract test already
   enforces that.
2. **section 9.2 cache and daemon routing**. These are the two performance criteria. Both
   are measurable, with a p50 below 10 ms.
3. **section 9.3 terminal fidelity, `detach`/`attach`, quoting table, soak**. This is the
   interactive shell. It holds the largest remaining value and it is the reason section 9.4
   keeps the TUI out.
4. **Release checklist**. The items are `CHANGELOG.md`, README "What's New", section 9
   marked shipped and the manual smoke test. A version bump comes only after them.
