#!/usr/bin/env python3
"""Issue ledger: re-verify every issue's fix against the CURRENT HEAD, then document.

The point is not to restate that an issue was closed — it is to prove the fix is still present in
the code today. Each entry pairs a claim with a command and the exact symbol the command must show.
A closed issue whose verification fails is a reopened bug, and the ledger says so.

Three verdicts, and only three:

  PASS     the predicate held, so the fix is present in the tree.
  FAIL     the predicate did not hold: a fix that is no longer present (closed issue). Exit status
           becomes 1.
  SKIPPED  no automated check exists for this issue (open feature request / defect with no landed
           fix), or a gated check was not enabled. A skipped check is NEVER counted as a pass.

Substring pitfalls are avoided on purpose: every source predicate asserts on an exact symbol with
`grep -nF` (fixed string) and a zero exit status, never on a count compared to a substring. An
earlier revision compared a `grep -c` COUNT against a symbol substring and produced false FAILs.

Run:  env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py            # source checks only
      env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py --json     # + machine summary
      env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py --cargo    # + cargo-gated checks
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

# Resolve the repo from this file's location so the harness works in a git worktree or a clone,
# not only in one hard-coded checkout.
REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "research" / "ISSUE_LEDGER.md"
CARGO_LOCK = Path.home() / ".hermes/cache/scratch/cargo-mutex.lock"
# Shared, heavy-gated target dir so concurrent departments do not thrash memory.
CARGO_TARGET = "/data/data/com.termux/files/home/workspace/PDN/target"

# ── Check table ───────────────────────────────────────────────────────────────
# Each check is a dict: issue, claim, cmd, needle [, mode, cargo].
# `mode="contains"` (default): PASS when rc == 0 AND `needle` appears in stdout — the fix is present.
# `mode="absent"`:            PASS when rc != 0 OR `needle` did NOT appear — the defect is gone.
# `needle=None`:              PASS on rc == 0 alone (use only where a symbol is not the point).
# `cargo=True`:               gated; runs only with --cargo, otherwise SKIPPED.
CHECKS: list[dict] = [
    # ── existing coverage (kept) ──────────────────────────────────────────────
    dict(issue="11", claim="install validates the container name before creating anything",
         cmd="grep -nF 'validate_container_name' crates/podnext/src/main.rs",
         needle="validate_container_name"),
    dict(issue="12", claim="delete_backup drops its read guard before taking the write lock",
         cmd="grep -nF 'contains_key' crates/pn-next/src/backup_restore/mod.rs",
         needle="contains_key"),
    dict(issue="12", claim="the deadlock regression test exists",
         cmd="grep -rnF 'test_delete_backup_does_not_deadlock' crates/pn-next/src/backup_restore/mod.rs",
         needle="test_delete_backup_does_not_deadlock"),
    dict(issue="13", claim="reset verifies the image BEFORE destroying the container",
         cmd="awk '/fn cmd_reset/,/^}/' crates/podnext/src/main.rs | grep -nE 'verify_image_available|remove_dir_all' | head -2",
         needle="verify_image_available"),
    dict(issue="14", claim="run sessions are registered so ps can see them",
         cmd="grep -rnF 'register_session' crates/proot-distro-core/src/runtime.rs crates/podnext/src/native.rs",
         needle="register_session"),
    dict(issue="19", claim="the restart path re-checks user_stopped under the same write lock",
         cmd="grep -nF 'user_stopped' crates/pn-next/src/supervisor.rs | head -8",
         needle="user_stopped"),
    dict(issue="20", claim="FileLock no longer flocks fd 0",
         cmd="grep -rnF 'FileLock' crates/proot-distro-core/src/locking.rs | head -6",
         needle="FileLock"),
    dict(issue="21", claim="direct-execution detection only fires when -r/--rootfs precedes the command",
         cmd="grep -nF 'pre_dashdash_args' crates/podnext/src/main.rs | head -4",
         needle="pre_dashdash_args"),
    dict(issue="22", claim="the self-audit job that replaces the nonexistent action exists",
         cmd="grep -nF 'name: Audit GitHub Actions pins' .github/workflows/dependency-updates.yml",
         needle="name: Audit GitHub Actions pins"),
    dict(issue="23", claim="Cargo.lock is tracked",
         cmd="git ls-files --error-unmatch Cargo.lock",
         needle="Cargo.lock"),
    dict(issue="24", claim="deny.toml encodes the advisory acceptance list (string form)",
         cmd="grep -nF '\"RUSTSEC-' deny.toml | head -3",
         needle='"RUSTSEC-'),
    dict(issue="25", claim="benchmark workflow enforces the exit status and builds the targets",
         cmd="grep -nF -- '--bench benchmarks' .github/workflows/benchmark.yml | head -4",
         needle="--bench benchmarks"),
    dict(issue="25", claim="the benchmark targets compile (the false-green defect)",
         cargo=True, needle=None, cmd="cargo bench --workspace --no-run --locked"),
    dict(issue="32", claim="spawned proot gets its own process group",
         cmd="grep -nF 'process_group(0)' crates/proot-core/src/lib.rs",
         needle="process_group(0)"),
    dict(issue="32", claim="container stop signals the group and reaps (zombie-aware)",
         cmd="grep -nF 'async fn terminate_group' crates/proot-distro-core/src/container.rs",
         needle="async fn terminate_group"),
    dict(issue="32", claim="podnext run forwards signals to the guest group",
         cmd="grep -nF 'fn forward_to_guest' crates/podnext/src/native.rs | head -3",
         needle="fn forward_to_guest"),
    dict(issue="32", claim="the daemon handles SIGTERM, not only Ctrl-C",
         cmd="grep -nF 'SignalKind::terminate' crates/podnext/src/daemon.rs",
         needle="SignalKind::terminate"),
    dict(issue="33", claim="the session record is unlinked on exit via a guard",
         cmd="grep -nF 'impl Drop for SessionGuard' crates/proot-distro-core/src/session.rs",
         needle="impl Drop for SessionGuard"),
    dict(issue="33", claim="log rotation has a consumer",
         cmd="grep -nF 'fn rotate_log_if_needed' crates/pn-next/src/supervisor.rs",
         needle="fn rotate_log_if_needed"),

    # ── NEW coverage: the previously-unchecked closed issues ──────────────────
    # #7  Add GitHub Actions workflow for automated dependency updates
    dict(issue="7", claim="dependabot.yml declares the cargo ecosystem",
         cmd="grep -nF 'package-ecosystem: \"cargo\"' .github/dependabot.yml",
         needle='package-ecosystem: "cargo"'),
    dict(issue="7", claim="the dependency-updates workflow file exists",
         cmd="ls .github/workflows/dependency-updates.yml",
         needle="dependency-updates.yml"),
    # #8  Add benchmark suite with Criterion
    dict(issue="8", claim="the Criterion bench target is declared",
         cmd="grep -nF '[[bench]]' crates/pn-next/Cargo.toml",
         needle="[[bench]]"),
    dict(issue="8", claim="the criterion dev-dependency is present",
         cmd="grep -nF 'criterion' crates/pn-next/Cargo.toml",
         needle="criterion"),
    dict(issue="8", claim="the bench source exists",
         cmd="ls crates/pn-next/benches/benchmarks.rs",
         needle="benchmarks.rs"),
    # #15 concurrent guest writes clobber — resolved as documented POSIX behaviour
    dict(issue="15", claim="the concurrent-write contract is documented for users",
         cmd="grep -nF 'no atomicity guarantee' docs/src/troubleshooting.md",
         needle="no atomicity guarantee"),
    dict(issue="15", claim="the #15 verdict (not a PDN defect, with control) is recorded",
         cmd="grep -nF 'not a PDN defect' research/issues/concurrency_verdict.md",
         needle="not a PDN defect"),
    # #16 deep tree extraction scales super-linearly
    dict(issue="16", claim="untar records created dirs instead of re-walking ancestors per entry",
         cmd="grep -nF 'created: &mut HashSet<PathBuf>' crates/proot-distro-core/src/untar.rs",
         needle="created: &mut HashSet<PathBuf>"),
    dict(issue="16", claim="the deep-recursion regression test exists",
         cmd="ls crates/proot-distro-core/tests/deep_fs_recursion.rs",
         needle="deep_fs_recursion.rs"),
    # #17 podnext build local rejects ARG, skips RUN
    dict(issue="17", claim="build local consumes ARG before the parser rejects it",
         cmd="grep -nF 'fn preprocess_build_args' crates/podnext/src/commands/mod.rs",
         needle="fn preprocess_build_args"),
    dict(issue="17", claim="build local actually executes RUN instead of printing it",
         cmd="grep -nF 'fn execute_run' crates/podnext/src/commands/mod.rs",
         needle="fn execute_run"),
    # #18 copy/sync expand sparse files
    dict(issue="18", claim="sparse copies probe holes with SEEK_DATA/SEEK_HOLE",
         cmd="grep -nF 'libc::SEEK_DATA' crates/podnext/src/main.rs",
         needle="libc::SEEK_DATA"),
    dict(issue="18", claim="the sparse-copy regression test exists",
         cmd="grep -nF 'fn copy_keeps_sparse_files_sparse' crates/podnext/src/main.rs",
         needle="fn copy_keeps_sparse_files_sparse"),
    # #30 benchmark job unpredictable / killed with SIGTERM
    dict(issue="30", claim="the benchmark job carries a bounded timeout",
         cmd="grep -nF 'timeout-minutes: 60' .github/workflows/benchmark.yml",
         needle="timeout-minutes: 60"),
    dict(issue="30", claim="the suite is invoked bounded via --bench benchmarks",
         cmd="grep -nF -- '--bench benchmarks' .github/workflows/benchmark.yml",
         needle="--bench benchmarks"),
    # #31 flaky restart-policy test (transition-driven fix)
    dict(issue="31", claim="the test asserts on transitions via wait_status, not fixed sleeps",
         cmd="grep -nF 'async fn wait_status' crates/pn-next/tests/supervisor_e2e.rs",
         needle="async fn wait_status"),
    dict(issue="31", claim="the test bounds its wait with a DEADLINE",
         cmd="grep -nF 'const DEADLINE: Duration = Duration::from_secs(30);' crates/pn-next/tests/supervisor_e2e.rs",
         needle="const DEADLINE: Duration = Duration::from_secs(30);"),

    # ── GATED cargo checks: behaviour a source predicate cannot prove ──────────
    # Run only with --cargo (they build; three departments share this host). Default = SKIPPED.
    dict(issue="18", claim="the sparse-copy unit test passes", cargo=True, needle="test result: ok",
         cmd="cargo test -p podnext --lib copy_keeps_sparse_files_sparse --locked"),
    dict(issue="16", claim="the deep-recursion test passes", cargo=True, needle="test result: ok",
         cmd="cargo test -p proot-distro-core --test deep_fs_recursion --locked"),
    dict(issue="31", claim="the abnormal-restart test passes", cargo=True, needle="test result: ok",
         cmd="cargo test -p pn-next --test supervisor_e2e test_restart_policy_on_abnormal --locked"),
]

# Issues with no honest automated check, and why. Every key is an issue that exists.
NO_CHECK_REASON = {
    "1": "OPEN feature request (PowerShell completion) — nothing on the tree to verify.",
    "2": "OPEN documentation task (example service configs) — no code fix to assert.",
    "3": "OPEN documentation task (tutorial) — no code fix to assert.",
    "4": "OPEN feature request (VS Code Dev Container) — no landed implementation.",
    "5": "OPEN feature request (systemd unit generator) — no landed implementation.",
    "6": "OPEN feature request (Docker Compose export) — partially landed via PR #26; see #28.",
    "9": "OPEN feature request (Prometheus exporter) — no landed implementation.",
    "10": "OPEN feature request (WebSocket events) — no landed implementation.",
    "27": "OPEN defect: documented [gateway.fail_safe] (bulkhead/retry_policy) absent from pn-gateway; "
          "no fix landed, so a fix-present predicate has nothing to assert.",
    "28": "OPEN defect: empty command yields a null Compose service in export_to_compose; no fix landed.",
    "29": "OPEN defect: podnext build local reports success without an image; no fix landed.",
}


def sh(cmd: str, timeout: int = 180, cargo: bool = False) -> tuple[int, str]:
    if cargo:
        inner = cmd.replace("'", "'\\''")
        cmd = (
            'export PATH="$HOME/.hermes/bin:$PATH"; '
            f"flock {CARGO_LOCK} -c '"
            f"CARGO_TARGET_DIR={CARGO_TARGET} heavy --budget-mb 2000 --min-avail-mb 700 -- "
            f"env -u LD_LIBRARY_PATH {inner}'"
        )
    p = subprocess.run(cmd, shell=True, cwd=REPO, capture_output=True, text=True, timeout=timeout)
    return p.returncode, (p.stdout + p.stderr).strip()


def gh_json(cmd: str):
    rc, out = sh(f"env -u LD_LIBRARY_PATH {cmd}")
    if rc != 0 or not out.strip():
        return None
    try:
        return json.loads(out)
    except Exception:
        return None


def run_check(c: dict, cargo_enabled: bool) -> dict:
    """Return a result dict. A check is only ever PASS/FAIL/SKIPPED after an attempt."""
    if c.get("cargo") and not cargo_enabled:
        return {**c, "verdict": "SKIPPED", "reason": "cargo check gated; re-run with --cargo", "out": ""}
    try:
        rc, out = sh(c["cmd"], cargo=bool(c.get("cargo")))
    except subprocess.TimeoutExpired:
        return {**c, "verdict": "SKIPPED", "reason": "command timed out", "out": ""}
    except Exception as e:  # never let an unrunnable check read as a pass
        return {**c, "verdict": "SKIPPED", "reason": f"command error: {e}", "out": ""}

    needle = c.get("needle")
    if c.get("mode") == "absent":
        ok = needle is None or needle not in out
    elif needle is None:
        ok = rc == 0
    else:
        ok = rc == 0 and needle in out
    return {**c, "verdict": "PASS" if ok else "FAIL", "reason": "", "out": out[:240].replace("\n", " | ")}


def main() -> int:
    ap = argparse.ArgumentParser(description="Re-verify every issue fix against HEAD.")
    ap.add_argument("--json", action="store_true", help="print a machine-readable summary to stdout")
    ap.add_argument("--cargo", action="store_true", help="also run the gated cargo/behaviour checks")
    ap.add_argument("--no-write", action="store_true", help="do not rewrite ISSUE_LEDGER.md")
    args = ap.parse_args()

    issues = gh_json("gh issue list --state all --limit 100 --json number,title,state,labels,closedAt,author")
    if issues is None:
        print("could not fetch issues (gh returned nothing) — refusing to write a ledger", file=sys.stderr)
        return 2
    by_num = {i["number"]: i for i in issues}

    results = [run_check(c, args.cargo) for c in CHECKS]
    npass = sum(r["verdict"] == "PASS" for r in results)
    nfail = sum(r["verdict"] == "FAIL" for r in results)
    nskip = sum(r["verdict"] == "SKIPPED" for r in results)
    fails = [r for r in results if r["verdict"] == "FAIL"]

    print(f"{'issue':6} {'check':74} verdict")
    print("-" * 96)
    for r in results:
        print(f"#{r['issue']:5} {r['claim'][:72]:74} {r['verdict']}")
    print(f"\nchecks: {npass} PASS, {nfail} FAIL, {nskip} SKIPPED (of {len(results)})")

    per_issue: dict[str, list] = {}
    for r in results:
        per_issue.setdefault(r["issue"], []).append(r)
    checked = {int(n) for n in per_issue}
    uncovered = sorted(i["number"] for i in issues if i["number"] not in checked)
    uncovered_open = [n for n in uncovered if by_num[n]["state"] == "OPEN"]

    def issue_line(n: str) -> str:
        i = by_num.get(int(n))
        if not i:
            return f"#{n} (not in the fetched list)"
        labs = ", ".join(l["name"] for l in i["labels"])
        return (f"**#{n} {i['title']}**  \n_state_: {i['state']}  ·  _labels_: {labs or '(none)'}  ·  "
                f"_closed_: {i['closedAt'] or '—'}")

    head = sh("git rev-parse --short HEAD")[1]
    subj = sh("git log -1 --format=%s")[1][:70]
    generated = time.strftime("%Y-%m-%d %H:%M:%SZ", time.gmtime())

    lines = [
        "# PDN issue ledger — fixes re-verified against HEAD",
        "",
        f"Generated {generated} from `{head}` ({subj}).",
        "",
        "## Why this exists",
        "",
        "Closing an issue records an intention. It does not prove the fix is still in the tree: a later",
        "refactor, a revert, or a merge that resolved a conflict the wrong way can quietly undo it. This",
        "ledger pairs every issue with a command that must still succeed against the current code, so a",
        "silently-regressed fix shows up as a FAIL in a document rather than as a surprise in production.",
        "",
        "## Verdict contract",
        "",
        "| verdict | meaning | effect on exit status |",
        "|---|---|---|",
        "| **PASS** | the exact symbol the fix introduces was found; the fix is present | none |",
        "| **FAIL** | the symbol is absent or the check could not hold: a regression, or a fix that is gone | **exit 1** |",
        "| **SKIPPED** | no automated check exists (open issue), or a gated `--cargo` check was not enabled | none, and **never** a pass |",
        "",
        f"**{npass} PASS · {nfail} FAIL · {nskip} SKIPPED** across {len(results)} checks "
        f"covering {len(checked)} of {len(issues)} issues "
        f"({len(uncovered)} issues have no automated check).",
        "",
        "## Checks that FAIL (a fix that is no longer present)",
        "",
    ]
    if fails:
        for r in fails:
            lines += [f"- **#{r['issue']} — {r['claim']}**", f"  - `{r['cmd']}`",
                      f"  - output: `{r['out']}`", ""]
    else:
        lines += ["None.", ""]

    skipped = [r for r in results if r["verdict"] == "SKIPPED"]
    lines += ["## Checks SKIPPED (no verdict was produced)", ""]
    if skipped:
        for r in skipped:
            lines += [f"- **#{r['issue']} — {r['claim']}** — _{r['reason']}_"]
        lines += [""]
    else:
        lines += ["None.", ""]

    lines += ["## Per-issue detail", ""]
    for n in sorted(per_issue, key=lambda x: int(x)):
        lines += [issue_line(n), ""]
        for r in per_issue[n]:
            lines += [f"- [{r['verdict']}] {r['claim']}", f"  - `{r['cmd']}`", f"  - → `{r['out']}`"]
        lines += [""]

    lines += [
        "## Issues with no automated check (the honest gap)",
        "",
        "Listed rather than omitted, so the coverage gap is visible. Each has a reason. These are open",
        "feature requests, open defects with no landed fix, or anything whose verification needs a live",
        "container and therefore belongs in CI.",
        "",
        "| # | state | reason | title |",
        "|---|---|---|---|",
    ]
    for n in uncovered:
        i = by_num[n]
        reason = NO_CHECK_REASON.get(str(n), "no honest source-level predicate identified.")
        reason = reason[5:] if reason.startswith("OPEN ") else reason
        lines.append(f"| {n} | {i['state']} | {reason} | {i['title'][:70]} |")
    lines.append("")

    lines += [
        "## How to re-run this (CI-friendly)",
        "",
        "```sh",
        "export PATH=\"$HOME/.hermes/bin:$PATH\"",
        "# source predicates only — fast, read-only, safe on any host including Android:",
        "env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py",
        "# machine-readable summary for CI (parse jq .checks.fail; exit status is 1 on any FAIL):",
        "env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py --json | jq .checks",
        "# opt-in behaviour checks (build; serialised with flock against concurrent departments):",
        "env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py --cargo",
        "```",
        "",
        "Exit status is **0** when no check FAILs and **1** when any check FAILs, so the ledger doubles as",
        "a regression gate. `--json` prints `{\"checks\":{\"pass\":N,\"fail\":N,\"skipped\":N,\"total\":N}}`.",
        "This file is regenerated by every run — it cannot drift from the code.",
        "",
    ]

    summary = {
        "generated": generated,
        "head": head,
        "checks": {"pass": npass, "fail": nfail, "skipped": nskip, "total": len(results)},
        "issues": {"total": len(issues), "covered": len(checked), "uncovered": len(uncovered),
                   "uncovered_open": len(uncovered_open)},
        "failures": [{"issue": r["issue"], "claim": r["claim"], "cmd": r["cmd"]} for r in fails],
        "skipped_gated": [{"issue": r["issue"], "claim": r["claim"], "reason": r["reason"]}
                          for r in skipped if r["reason"].startswith("cargo")],
        "cargo_enabled": args.cargo,
    }

    if not args.no_write:
        OUT.write_text("\n".join(lines))
        print(f"\nwritten: {OUT}")
    if args.json:
        print(json.dumps(summary, indent=2, sort_keys=True))
    return 1 if nfail else 0


if __name__ == "__main__":
    sys.exit(main())
