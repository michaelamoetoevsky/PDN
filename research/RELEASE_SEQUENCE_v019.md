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
| 9.1 exit-code contract | met | `crates/podnext/tests/exit_codes.rs`, now 7 classes - the phantom `3` from `config validate` was removed by moving the CODE to the documented `2`; `665b3ba` |
| 9.1 contract test incl. examples | met | commands and flags gated by `28c4594`; the 50 example blocks are now executed or excluded with a printed reason by `scripts/check_doc_examples.py` (`10 executed / 41 excluded, 0 failures`), and the harness runs in CI; it found two genuinely wrong examples - #64, `c9108ec` |
| 9.1 triage of the audit | met | the ledger fails on an untriaged issue; 0 untriaged of 69 issues, and the disposition document is generated rather than hand-kept (`build_issue_ledger.py --disposition`) |
| 9.2 split, no file over ~800 lines | **half** | `main.rs` 4,679 to 1,040, `commands/mod.rs` 3,719 to **813**, Clap surface in `cli.rs` 1,445; 123 help screens byte-identical - two files are still above the ceiling, #65 |
| 9.2 index cache p50 under 10 ms | met in-process | `cf3f487`: 1.3-2.2 ms in-process with ten containers; process start alone is 31-36 ms, so end-to-end belongs to #59 |
| 9.2 honest image size | met | `#44`: a measured size, a `>=` bound, or `unknown` |
| 9.2 metadata through the daemon | **exception, recorded** | routed: six queries diff empty against the fallback, round trips constant (1 vs 0), never stale; latency independence NOT met (5.20 vs 1.48 ms at n=10) and left unmet on purpose, with the reason |
| 9.2 environment paths | met | `#46`: `TERMUX__HOME`, `PREFIX`, `XDG_*`; `TERMUX__PREFIX` excluded with the measurement |
| 9.3 detach and attach | met | #57 `ff2e1e3`: the same shell pid answers two attaches, a marker written while detached survives, `0 proot/podnext rows` before and after, and a SIGKILL of the supervisor leaves no orphan; supervisor `setsid`, `PPid=1`. Two simplifications are DECLARED in `known-limitations.md`, not hidden: no `SIGWINCH` re-forwarding while attached, no replay of output produced while detached |
| 9.3 terminal fidelity | met | `scripts/shell_fidelity.py` PASS - winsize, live resize, `TERM`, `COLORTERM`, `PS1`, prompt, UTF-8 both ways, with a control that goes red; it also found and proved the #68 regression, now fixed |
| 9.3 argv quoting | met | `5c34115`: 15 cases against a real guest, NUL-separated |
| 9.3 bindings and `USER` | met | #66 `523ae58` measured it inside a guest with a control: `/sdcard`/`/storage/*` are the guest's own empty rootfs dirs (dev id matches `/`), `/tmp` and `/dev/shm` are per-container, the host home is NOT mounted. `USER`: build states the mapping, `run`/`login` now print it on stderr with stdout untouched, and both `run --user` and `login --user` refuse with the measurement - the silent discard is gone (`5e4f24d`) |
| 9.3 session hygiene and soak | met | `scripts/soak_sessions.py` - 20/20 iterations with zero container processes left, and an injected leak caught at iteration 7 |
| 9.3 doctor | met | `#45`: host and guest checks including the proot argv |

**14 met, 1 half, 1 recorded exception, 0 not met.** The half row and the exception are the ones a
status paragraph tends to round up, which is why they are rows here. The half is the line ceiling (#65);
the exception is the daemon's latency independence, measured and left unmet on purpose.

## 3. The sequence

Each batch is a unit of work with its own gate. A batch is not done because the code is written; it
is done when the gate has run and its output is in the ticket.

### S1 - done (wave 2, base `a0b4025`)

| Ticket | Department | Landed | Outcome |
|---|---|---|---|
| #54 | `dept/build` | `d8f84d5` | `commands/mod.rs` 1,945 -> 813; `build.rs` 698, `gitops.rs` 455 |
| #56 + #58 | `dept/shell` | `eab10ce` | two harnesses; fidelity found the #68 regression, soak passed 20/20 with a leak control |
| #59 | `dept/daemon` | `83827a1` | metadata routed; identical output, constant round trips, never stale; latency independence recorded as an exception |
| #60 + #63 | `dept/honesty` | `255d25a` | seccomp profile refused with the decision measured; `--no-kill-on-exit` refused |

`dept/build`'s extraction turned six file-pinned predicates red; it reported the repath instead of
editing the parent-owned ledger, and the parent applied it in the same push. The parent also fixed
the #68 regression the shell department found, using that department's harness as the oracle.

### S2 and S3 - landed as wave 3 (base `58636ee`, integrated on `master`)

| Ticket | Department | Landed | Outcome |
|---|---|---|---|
| #57 | `dept/attach` | `ff2e1e3` | detach/attach round trip, same shell pid, marker survives, no orphan; refusal lifted |
| #64 | `dept/examples` | `c9108ec` | 50 blocks: 10 executed, 40 excluded with reasons; two wrong examples fixed |
| #66 | `dept/bindings` | `523ae58` | storage paths measured (rootfs dirs, not binds), shared home/tmp and the USER story measured |
| #67 | `dept/scanner` | `634bf3d` | 19 flags honoured or refused with the argv as evidence |

Cherry-picked with no conflict: the four branches touched eleven distinct files. The parent added eleven
predicates (three for #57, two each for #64 and #66, three for #67), deleted #67's now-stale decline reason,
wired both new harnesses into CI, and filed #70 for a committed conflict marker found while integrating.

### Wave 4 - landed (`ad123ef`, base `1fcba7d`)

| Department | Landed | Outcome |
|---|---|---|
| `dept/doctext` | `650e564` | detach/attach prose corrected in `cli.rs` + four documents; two fabricated sample outputs replaced with real captured output; the two simplifications declared; baseline regenerated with exactly three screens moved |
| `dept/userstory` | `5e4f24d` | `login --user` refuses instead of being silently discarded; `run`/`login` state the mapped user on stderr with stdout untouched; `config validate` exits the documented 2 - the code was the wrong side |
| parent | `ad123ef` | `scripts/local_gate.sh` gained a help-dump stage that cannot report a false identity (verified by sabotage) |

### Wave 5 - the leftovers the departments reported but did not own

| Work | Why it needs its own wave |
|---|---|
| `cli.rs` and five doc locations still say detach refuses / attach creates a session | `cli.rs` text appears in the 123-screen help dump, so the text change and the regenerated baseline must land in one commit |
| `login --user` is accepted and ignored while `run --user` refuses | `container.rs`/`native.rs` ownership, and it is a behaviour change, not a text change |
| `run`/`login` print no user, which 9.3 asks the build/run output to state | same files, and it needs a decision about the output format |
| Two fabricated sample outputs in `cli-reference.md`; `config validate` exits 3 where the doc says 2 | doc plus contract question |
| #65: `cli.rs` 1,450 and `main.rs` 1,170 above the ceiling | in flight (wave 6, `dept/split`); the tracked help fixture is its guard rail |

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
   `proot-distro-core --lib`; the 123-screen help dump against the TRACKED fixture
   `research/cli-help-baseline.txt`; `build_issue_ledger.py --offline` with 0 FAIL; `check_non_goals.py`.
   (That path is not the untracked scratch file: comparing one of those across worktrees cost three
   defects in a single round - see `86a148b`)
5. Provenance, because the target directory is shared: build and copy inside the lock, prove the
   binary with a string only this tree has, prove the library with the test names in the output.
6. Push, then read the CI run on the pushed head: the ledger job must report all predicates with
   verdicts and 0 FAIL.

### S5 - release preparation (documentation only, no behaviour change)

- `Cargo.toml` `0.1.8` to **`0.2.0`**; `Cargo.lock` follows. It is `0.2.0` and not `0.1.9` or `0.1.10`
  because `v0.1.9` was tagged and withdrawn (`ed7dc3f`, before any release existed) and `roadmap.md`
  section 10 names `v0.2.0` as the mobility target; a withdrawn tag is not a released version.
- `CHANGELOG.md`: move `[Unreleased]` to `## [0.2.0] - <date>`, with every entry naming its commit.
- Re-record the warning ceiling to the value the settled tree actually produces. It is 290 in the fixture
  today and the runner measured **266** - the detach/attach work consumes symbols that the extraction had
  left as unused imports. A ceiling only refuses growth, so the gap is safe but it is slack, and slack is
  what the next unrelated change spends. Re-measure **cold** on the device (`--cold`) and record both
  toolchains, because the ceiling is keyed by toolchain and a warm build measures zero.
- Housekeeping entry for the nine one-shot commit scripts removed in `28dba29`, so the release notes do not
  imply the repository always looked like this.
- `docs/src/roadmap.md`: stamp section 9 statuses with the measured numbers, including any criterion
  that ends the release as a **recorded exception** rather than met.
- `README.md`: "What's New" reflects the same list.
- `docs/src/known-limitations.md`: unchanged except where a ticket added an entry.

### S6 - tag, release, smoke test

The release workflow (`.github/workflows/release.yml`) triggers on a `v*` tag and does a release
build, test and clippy per target, then uploads `podnext-<target>`, `install.sh` and
`install-short.sh`, runs the security audit, and creates the release. So the sequence is:

1. Tag **`v0.2.0`** on the S5 commit, push the tag, watch `Release Automation` to completion.
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
