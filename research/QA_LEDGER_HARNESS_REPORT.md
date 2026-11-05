# QA Ledger Harness — coverage hardening and negative controls

**Department:** Quality Engineering (QE)
**Branch:** `dept/qa-ledger` (base `5df9484`)
**Artifacts:** `scripts/build_issue_ledger.py`, `research/ISSUE_LEDGER.md`, this report.

## 0. Coverage statement (read this first)

From the committed run (source predicates only — `env -u LD_LIBRARY_PATH python3
scripts/build_issue_ledger.py`, exit 0):

- **(a) PASS by direct observation: 35** — every one executed its predicate against this tree.
- **(b) SKIPPED (no verdict produced): 4** — the `--cargo` gated behaviour checks, which are not
  enabled by default and, on this device, cannot run to completion under the memory guard.
- **(c) FAIL: 0.**

**A SKIPPED check is not evidence the fix works.** Four checks covering four issue-claims returned no
verdict whatsoever; do not read "35 PASS" as "39 verified". The four that produced no verdict and the
guard's own reason are in section 6.

## 1. What the mission was

The ledger at `5df9484` claimed *"19 checks PASS, 0 FAIL out of 19 automated checks"* and *"19 issues
without an automated check are recorded explicitly"*. Nineteen checks is a sample, not a regression
harness. The gap: nineteen issues had no predicate at all, the harness could not fail (every check was
`rc == 0 and needle in out`, and it always exited 0), and the document drifted — e.g. `#30`/`#31` were
listed as `OPEN` in the committed ledger while GitHub had since closed them.

## 2. Coverage before vs. after

| | before (`5df9484`) | after (`dept/qa-ledger`) |
|---|---|---|
| checks executing | 19 | 35 source (+ 4 cargo-gated) |
| issues covered | 13 | 21 |
| issues with no check | 19 | 11 |
| states | PASS / FAIL | PASS / FAIL / SKIPPED |
| exit status on FAIL | always 0 | 1 |
| `--json` | no | yes |

New checks were added for the previously-unchecked **closed** issues **#7, #8, #15, #16, #17, #18,
#30, #31** (16 new source predicates). Each is a real assertion on an exact symbol — `grep -nF
'<exact symbol>'`, a path existence probe, or a workflow literal — never an echo of a claim.

## 3. Negative controls (the important part)

Four checks were sabotaged in the worktree — the fix reverted in place — the harness run, and the
resulting FAILs captured; then the tree was restored and the harness re-run showing PASS.

Sabotage applied (raw):

```
sed -i 's/libc::SEEK_DATA/libc::SEEK_CUR/g' crates/podnext/src/main.rs                       # #18
sed -i 's/const DEADLINE/const DEADLINE_SAB/g' crates/pn-next/tests/supervisor_e2e.rs       # #31
sed -i '/^criterion = /d' crates/pn-next/Cargo.toml                                         # #8
sed -i 's/created: &mut HashSet<PathBuf>/created: \&mut Vec<PathBuf>/' crates/proot-distro-core/src/untar.rs  # #16
```

Raw run with the four fixes absent (`~/.hermes/cache/scratch/qe-negative.log`):

```
#8     the criterion dev-dependency is present                                    FAIL
#16    untar records created dirs instead of re-walking ancestors per entry       FAIL
#18    sparse copies probe holes with SEEK_DATA/SEEK_HOLE                         FAIL
#31    the test bounds its wait with a DEADLINE                                   FAIL

checks: 31 PASS, 4 FAIL, 4 SKIPPED (of 39)
HARNESS_EXIT=1
```

Raw run after `git checkout --` restored the four files (`~/.hermes/cache/scratch/qe-final.log`):

```
checks: 35 PASS, 0 FAIL, 4 SKIPPED (of 39)

written: /data/data/com.termux/files/home/.hermes/cache/scratch/wt-dept-qe/research/ISSUE_LEDGER.md
HARNESS_EXIT=0
```

**A first attempt at this control caught the exact bug class the mission warned about.** The `#31`
predicate was `grep -nF 'const DEADLINE'`, and `sed 's/const DEADLINE/const DEADLINE_SAB/'` left the
substring `const DEADLINE` intact, so the check stayed PASS while the fix was gone — a false green.
The predicate was hardened to the full declaration `const DEADLINE: Duration = Duration::from_secs(30);`,
after which the same sabotage produced FAIL. Every source predicate now asserts on a full symbol.

## 4. The PASS / FAIL / SKIPPED contract

| verdict | when | exit status |
|---|---|---|
| **PASS** | predicate held — the exact symbol is present | none |
| **FAIL** | predicate did not hold — a fix that is gone, or a check that could not hold | **1** |
| **SKIPPED** | no automated check exists (open issue), or a cargo-gated check was not enabled | none — **never** a pass |

Guarantees: a check is assigned a verdict **only after an attempt**; a command that times out or raises
is `SKIPPED` with the reason, never PASS; the process exits `1` on any FAIL, else `0` (`2` if issue
metadata cannot be read).

## 5. CI usage

```sh
export PATH="$HOME/.hermes/bin:$PATH"
env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py                 # source predicates
env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py --json | jq .checks
env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py --cargo         # + gated behaviour checks
```

`--json` emits `{"checks":{"pass":N,"fail":N,"skipped":N,"total":N},"issues":{...},"failures":[...]}`.
Every run rewrites `research/ISSUE_LEDGER.md`, so the document cannot drift. The harness resolves the
repo from its own path, so it works in a worktree or clone. Gated cargo checks run under the mandated
guard (`flock … heavy --budget-mb 2000 --min-avail-mb 700 -- env -u LD_LIBRARY_PATH cargo test …`).

## 6. Gated (`--cargo`) behaviour checks — observed outcomes and guard reasons

These four run only with `--cargo`; the committed ledger reports them SKIPPED. Their behaviour was
probed in an experimental run; the raw guard status files are quoted rather than summarised:

| check (issue) | command | observed | guard reason (from `~/.hermes/logs/heavy/*.status`) |
|---|---|---|---|
| #16 deep-recursion test | `cargo test -p proot-distro-core --test deep_fs_recursion --locked` | **completed, exit 0** — `test result: ok. 3 passed; 0 failed` | `outcome=completed exit_code=0 reason=exited normally peak_rss_mb=21` |
| #31 abnormal-restart test | `cargo test -p pn-next --test supervisor_e2e test_restart_policy_on_abnormal --locked` | **completed, exit 0** — `1 passed; 18 filtered out` | `outcome=completed exit_code=0 reason=exited normally peak_rss_mb=1` |
| #25 bench compile | `cargo bench -p pn-next --no-run --locked` | **not observed** — guard killed it | `outcome=killed exit_code=137 reason=group RSS 2015MB exceeded budget 2000MB` |
| #18 sparse-copy unit test | `cargo test -p podnext --bins copy_keeps_sparse_files_sparse --locked` | **not observed** — target corrected, not re-run | first form `--lib` errored (`error: no library targets found in package podnext`, exit 101); `--bins` form left un-run |

That experimental exercise also exposed two dishonesty paths in my own gated checks, both fixed before
delivery: the memguard over-budget kill prints `memguard: killed at NMB` (a different summary shape
than `completed`), which was surfacing as FAIL; and the summary appends ` peak=NMB dur=Ns` to the
reason, so an exact-equality test wrongly treated every completed run as guard-stopped. Gated checks
now key on the child's exit status and treat **any** guard stop as SKIPPED with the guard's reason.

## 7. Residual uncovered issues (11) and why each cannot be checked automatically

All eleven are **OPEN**; there is no landed fix, so a fix-present predicate has nothing to assert.

| # | reason |
|---|---|
| 1 | PowerShell shell completion — feature not implemented. |
| 2 | Example service configs — docs-only task, no code fix to assert. |
| 3 | Full-stack tutorial — docs-only task. |
| 4 | VS Code Dev Container support — not implemented. |
| 5 | systemd unit generator — not implemented. |
| 6 | Docker Compose export — partially landed via PR #26; tracked by #28. |
| 9 | Prometheus exporter — not implemented. |
| 10 | WebSocket events — not implemented. |
| 27 | Documented `[gateway.fail_safe]` (bulkhead/retry_policy) absent from `pn-gateway`; no fix landed. |
| 28 | Empty command yields a null Compose service in `export_to_compose`; no fix landed. |
| 29 | `podnext build local` reports success without an image; no fix landed. |

## 8. Proven vs. still unverified

- **Proven:** the four sabotaged fixes are load-bearing — removing them flips the harness to FAIL and
  exit 1; restoring them returns PASS and exit 0. The 35 source predicates execute, evaluate and pass
  on this tree. The harness emits PASS/FAIL/SKIPPED counts, a non-zero exit on FAIL, and `--json`.
- **Still unverified (must not be read as verified):** the 4 gated behaviour checks — see section 0 and section 6.
  The `#25` bench compile has **no** verdict (guard-killed); `#18`'s sparse test has **no** verdict
  (target corrected, deliberately not re-run). `#16` and `#31` were observed to pass in the
  experimental run, but the committed ledger reports them SKIPPED because they did not run in the
  delivered default invocation.
- **Honest limits:** a source predicate proves a symbol exists, not that it behaves. The behaviour
  behind `#16`/`#18`/`#31`/`#25` is what the gated checks exist to prove; on this Android host the
  memory guard is the binding constraint.

## 9. Isolation note

The mission's very first probe ran the old harness in the **main integration checkout**
(`~/workspace/PDN`) before this worktree existed, and rewrote `research/ISSUE_LEDGER.md` there
(mtime 22:42). The Integrator restored that file and kept the stray as
`~/.hermes/cache/scratch/stray-ledger-from-qe-kickoff.md`. All subsequent work — every write, every
sabotage, every harness run — happened under `$HOME/.hermes/cache/scratch/wt-dept-qe`. Nothing under
`~/workspace/PDN` was touched after that first probe.
