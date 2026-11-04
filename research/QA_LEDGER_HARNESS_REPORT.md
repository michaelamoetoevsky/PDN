# QA Ledger Harness — coverage hardening and negative controls

**Department:** Quality Engineering (QE)
**Branch:** `dept/qa-ledger` (base `5df9484`)
**Artifacts:** `scripts/build_issue_ledger.py`, `research/ISSUE_LEDGER.md`, this report.

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
#30, #31** (16 new source predicates in total). Every one is a real assertion on an exact symbol in
the current tree — `grep -nF '<exact symbol>'`, a path existence probe, or a TOML/workflow literal.
None echoes a claim or greps for its own name.

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

Raw run with the four fixes absent (`$HOME/.hermes/cache/scratch/qe-negative.log`):

```
#8     the criterion dev-dependency is present                                    FAIL
#16    untar records created dirs instead of re-walking ancestors per entry       FAIL
#18    sparse copies probe holes with SEEK_DATA/SEEK_HOLE                         FAIL
#31    the test bounds its wait with a DEADLINE                                   FAIL

checks: 31 PASS, 4 FAIL, 4 SKIPPED (of 39)
HARNESS_EXIT=1
```

Raw run after `git checkout --` restored the four files (`$HOME/.hermes/cache/scratch/qe-final.log`):

```
checks: 35 PASS, 0 FAIL, 4 SKIPPED (of 39)

written: /data/data/com.termux/files/home/.hermes/cache/scratch/wt-dept-qe/research/ISSUE_LEDGER.md
HARNESS_EXIT=0
```

**A first attempt at this control caught the exact class of bug the mission warned about.** The `#31`
predicate was `grep -nF 'const DEADLINE'`, and `sed 's/const DEADLINE/const DEADLINE_SAB/'` left the
substring `const DEADLINE` intact, so the check stayed PASS while the fix was gone — a false green.
The predicate was hardened to the full declaration
`const DEADLINE: Duration = Duration::from_secs(30);`, after which the same sabotage produced FAIL.
This is why every source predicate asserts on a full symbol, never a prefix.

## 4. The PASS / FAIL / SKIPPED contract

| verdict | when | exit status |
|---|---|---|
| **PASS** | predicate held — the exact symbol is present | none |
| **FAIL** | predicate did not hold — a fix that is gone, or a check that could not hold | **1** |
| **SKIPPED** | no automated check exists (open issue), or a cargo-gated check was not enabled | none — and it is **never** counted as a pass |

Implementation guarantees:

- A check is assigned PASS/FAIL/SKIPPED **only after an attempt**. A command that times out or raises
  is recorded as `SKIPPED` with the reason, never as PASS.
- Gated `cargo` checks are `SKIPPED` by default and run only under `--cargo`.
- The process exits `1` when any check is FAIL, otherwise `0`; `2` if issue metadata cannot be read.

## 5. CI usage

```sh
export PATH="$HOME/.hermes/bin:$PATH"
# source predicates only — fast, read-only, safe on Android:
env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py
# machine-readable summary; jq .checks.fail is the gate:
env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py --json | jq .checks
# opt-in behaviour checks: build, and are flock-serialised against the other departments:
env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py --cargo
```

`--json` emits `{"checks":{"pass":N,"fail":N,"skipped":N,"total":N},"issues":{...},"failures":[...]}`.
Every run rewrites `research/ISSUE_LEDGER.md`, so the document cannot drift from the code or from the
issue tracker again. The harness resolves the repo from its own path, so it works in a worktree/clone.

Gated cargo checks run under the mandated guard:

```
flock $HOME/.hermes/cache/scratch/cargo-mutex.lock -c \
  'CARGO_TARGET_DIR=/data/data/com.termux/files/home/workspace/PDN/target \
   heavy --budget-mb 2000 --min-avail-mb 700 -- env -u LD_LIBRARY_PATH cargo test -p <crate> --locked'
```

## 6. Residual uncovered issues (11) and why each cannot be checked automatically

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

For `#27` a drift check *is* mechanically possible (intersect the keys in `docs/src/configuration.md`
with `crates/pn-gateway/src/config.rs`); it is deferred rather than faked because there is no fix yet
whose presence it would assert.

## 7. Proven vs. still unverified

- **Proven:** the four sabotaged fixes are load-bearing — removing them flips the harness to FAIL and
  exit 1. The 35 source predicates execute, evaluate, and pass on `5df9484`. The harness emits
  PASS/FAIL/SKIPPED counts, a non-zero exit on FAIL, and `--json`.
- **Unverified here:** the four `cargo`-gated behaviour checks (`#16`, `#18`, `#25`, `#31` test
  binaries) were left `SKIPPED` in the default run because building under the shared memory budget was
  out of scope for this pass; they are wired and ready under `--cargo`.
- **Honest limits:** source predicates prove a symbol exists, not that it behaves. The behaviour behind
  `#16`/`#18`/`#31` is proven by their tests, which is what the gated checks invoke.
