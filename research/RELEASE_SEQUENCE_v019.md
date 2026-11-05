# Release sequence for v0.1.9

What has to happen, in what order, with what evidence, before `v0.1.9` can be tagged. Written to be
read next to `docs/src/roadmap.md` section 9 (the criteria) and `research/ISSUE_INVENTORY.md` (the
tickets). Every number here came from a command that is quoted beside it.

A previous attempt at this release was tagged and withdrawn (`be4919d`, reverted in `ed7dc3f`)
because the tag claimed a milestone whose criteria were largely unmet. This document exists so that
does not happen twice: the sequence ends in a gate, not in an intention.

## 1. What "v0.1.9" means

The release contract is **16 acceptance criteria** in three groups, plus a non-goal list that is
itself checked:

| Group | Criteria | What it is about |
|---|---|---|
| 9.1 | 5 | honesty: labels, refusals, exit codes, a contract test, triage of the audit |
| 9.2 | 5 | simplicity and performance: the module split, the index cache, honest sizes, daemon routing, environment paths |
| 9.3 | 6 | the interactive shell: detach/attach, terminal fidelity, argv quoting, bindings and `USER`, session hygiene, doctor |
| 9.4 | - | explicit non-goals, enforced by `scripts/check_non_goals.py` |

Nothing outside section 9 is in scope. The next tag after this one is `v0.2.0` (roadmap section 10);
there is no `v0.1.10`.

## 2. Where the release stands, measured

| Criterion | State | Evidence |
|---|---|---|
| 9.1 labels | met | 37 commands + `doctor` labelled; `fcabc1c` |
| 9.1 stub refusals | met | zero "not fully implemented" in user-facing output; `fcabc1c` |
| 9.1 exit-code contract | met | `crates/podnext/tests/exit_codes.rs`, 5 classes; `665b3ba` |
| 9.1 contract test incl. examples | **half** | commands and flags gated by `28c4594`; ~100 example blocks in `cli-reference.md` are never executed - #64 |
| 9.1 triage of the audit | met | the ledger now fails on an untriaged issue; 0 of 61 untriaged (`research/AUDIT_FINDINGS_DISPOSITION.md`) |
| 9.2 split, no file over ~800 lines | **half** | `main.rs` 4,679 to 1,040, `commands/mod.rs` 3,719 to 1,945, Clap surface in `cli.rs` 1,445; 123 help screens byte-identical - but three files are above the ceiling, #65 |
| 9.2 index cache p50 under 10 ms | met in-process | `cf3f487`: 1.3-2.2 ms in-process with ten containers; process start alone is 31-36 ms, so end-to-end belongs to #59 |
| 9.2 honest image size | met | `#44`: a measured size, a `>=` bound, or `unknown` |
| 9.2 metadata through the daemon | **not met** | #59, dispatched |
| 9.2 environment paths | met | `#46`: `TERMUX__HOME`, `PREFIX`, `XDG_*`; `TERMUX__PREFIX` excluded with the measurement |
| 9.3 detach and attach | **not met** | #57, held behind #59 (same file) |
| 9.3 terminal fidelity | **not met** | #56, dispatched |
| 9.3 argv quoting | met | `5c34115`: 15 cases against a real guest, NUL-separated |
| 9.3 bindings and `USER` | **half** | documented; never measured inside a guest - #66 |
| 9.3 session hygiene and soak | **not met** | #58, dispatched |
| 9.3 doctor | met | `#45`: host and guest checks including the proot argv |

**9 met, 3 half, 4 not met.** The half rows are the ones a status paragraph tends to round up, which
is why they are rows here.

## 3. The sequence

Each batch is a unit of work with its own gate. A batch is not done because the code is written; it
is done when the gate has run and its output is in the ticket.

### S1 - the four dispatched departments (in flight)

| Ticket | Department | Files (exclusive) | Gate |
|---|---|---|---|
| #54 | `dept/build` | `commands/mod.rs` regions + `build.rs`, `gitops.rs`, `compose.rs` | fmt, 48 unit, 6 contract, 123 help screens byte-identical, ledger 0 FAIL |
| #56 + #58 | `dept/shell` | `scripts/shell_fidelity.py`, `scripts/soak_sessions.py` | the measurements themselves, each with a demonstrated negative control |
| #59 | `dept/daemon` | `daemon.rs`, `commands/container.rs` | both paths diffed, latency and round-trip counts, a staleness control |
| #60 + #63 | `dept/honesty` | `main.rs`, `cli.rs`, `security.rs`, two docs | measured argv comparison for #63; a decision stated in terms of the measurement for #60 |

### S2 - #57, held on purpose

`attach` and `sessions` live in `commands/container.rs`, which `dept/daemon` owns until S1 lands.
Running both is how the first wave produced an eleven-block conflict. S2 starts from the result of
#59.

### S3 - the three criterion gaps

- **#64** executes the documented examples, with a written exclusion list.
- **#65** brings `cli.rs`, `main.rs` and `commands/mod.rs` under the ceiling, or records the
  exception with its size in the roadmap. Lowering the promise is allowed; hiding the number is not.
- **#66** measures the bindings and the `USER` story inside a guest, then makes code and
  documentation agree.

These three can run in parallel with S2 provided they do not take a file S2 owns.

### S4 - integration

1. `scripts/integrate_branches.py` for each branch: it counts the commits to pick from the branch's
   **base**, refuses a branch that touched a parent-owned file, and asserts the pick count before
   and after.
2. Where two branches extracted the same file, reconstruct deterministically: take one branch's
   final file and re-run `scripts/extract_cli_module.py` with the other branch's symbol list. Never
   hand-resolve a thousand conflicting lines.
3. Re-check every ledger predicate that greps a file a branch replaced wholesale - a master-side fix
   to that file is silently undone by taking the branch's copy, and that is exactly how the #41
   `args_os` fix was lost and caught.
4. Gates on the merged tree: `cargo fmt --all -- --check`; `--bin podnext` plus every test target;
   `proot-distro-core --lib`; the 123-screen help dump against `$PREFIX/tmp/help-baseline.txt`;
   `build_issue_ledger.py --offline` with 0 FAIL; `check_non_goals.py`.
5. Provenance, because the target directory is shared: build and copy inside the lock, prove the
   binary with a string only this tree has, prove the library with the test names in the output.
6. Push, then read the CI run on the pushed head: the ledger job must report all predicates with
   verdicts and 0 FAIL.

### S5 - release preparation (documentation only, no behaviour change)

- `Cargo.toml` `0.1.8` to `0.1.9`; `Cargo.lock` follows.
- `CHANGELOG.md`: move `[Unreleased]` to `## [0.1.9] - <date>`, with every entry naming its commit.
- `docs/src/roadmap.md`: stamp section 9 statuses with the measured numbers, including any criterion
  that ends the release as a **recorded exception** rather than met.
- `README.md`: "What's New" reflects the same list.
- `docs/src/known-limitations.md`: unchanged except where a ticket added an entry.

### S6 - tag, release, smoke test

The release workflow (`.github/workflows/release.yml`) triggers on a `v*` tag and does a release
build, test and clippy per target, then uploads `podnext-<target>`, `install.sh` and
`install-short.sh`, runs the security audit, and creates the release. So the sequence is:

1. Tag `v0.1.9` on the S5 commit, push the tag, watch `Release Automation` to completion.
2. Download one artifact and run the **smoke test on this device**, which the withdrawn attempt
   never did: `podnext install alpine`, `run`, `login`, `remove`, and `doctor` - each command's exit
   status recorded.
3. Only then mark the release as latest, and post the acceptance summary as a comment on map #48,
   quoting the ledger and smoke-test output.

## 4. The standing protocol for every batch

- One worktree and one branch per department, cut from the base named in its ticket; the child
  commits and **never pushes**; the parent cherry-picks.
- Builds serialised: `flock $HOME/.hermes/cache/scratch/cargo-mutex.lock` around
  `heavy --budget-mb N -- -- env -u LD_LIBRARY_PATH cargo ...`. The device's memory guard kills
  a job that exceeds its budget, and a killed job is not a failed test.
- The gate runs first and is read; the commit is a separate command. A commit that rode along with
  its own test is how a broken test target reached master once.
- Every ticket states a **negative control**: the demonstration that its check can go red. A check
  that has never been seen failing is not yet a check.
- Every claim in a report carries the command that produced it. A number that cannot be re-derived
  is reported as un-derivable rather than repeated - that is how a wrong figure in the roadmap is
  found.
- Temp files under `$PREFIX/tmp`. `/tmp` is not writable on Android and `temp_dir()` falls back to
  it when `TMPDIR` is unset.

## 5. Do not ship if

- Any of the 16 criteria is "not met" and is not recorded as an exception in the roadmap with its
  measurement.
- The ledger reports any FAIL on the pushed head, or a predicate has no verdict where a verdict is
  possible.
- The 123-screen help dump is not byte-identical to the recorded baseline.
- The non-goal gate is red.
- A smoke test step has not been run on a released artifact.
- A ticket in S1-S3 is closed without its evidence comment.

If the tag happens anyway and the criteria are not met, the rule is the same as last time:
**withdraw the release and say so**, rather than relabel what was tagged.

## 6. What this document does not promise

It does not promise a date. It does not claim the four "not met" criteria will be met by S1 as
dispatched - a department that cannot meet its criterion must leave its branch at a compiling state
and report where it stopped. And it does not claim the list of criteria is complete: the three
tickets in S3 exist because a criterion was read closely enough to notice that half of it had never
been exercised, and reading the remaining criteria that closely is part of S3, not after it.
