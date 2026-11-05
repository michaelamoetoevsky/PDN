#!/usr/bin/env python3
"""Integrate department branches, then run the gates on the merged tree.

Implements the integration half of the delegation procedure in
`hermes-harness/references/parallel-department-delegation.md`, because doing it by hand has two
sharp edges that have already bitten this project:

1. **Count the pick range from the BASE, not from master.**
   `git rev-list --count master..dept/x` over-counts when master already holds some of the
   branch's commits; the range to pick is `merge-base(master, dept/x)..dept/x`. After cherry-picking,
   the number of new commits on the integration branch must equal that count - a mismatch means
   commits were re-picked or dropped, and `git cherry-pick` with an empty argument list is a no-op
   that still exits 0.

2. **Report what each branch touched *relative to its base*.**
   `git diff --name-only master..dept/x` lists the integrator's own newer commits as if the child
   had made them. Compare against the merge base instead, and grep the result against the files the
   parent owns: a child that touched one of those is a conflict to find by reading, not by resolving.

Usage:
    scripts/integrate_branches.py --branch dept/obs --branch dept/sup --dry-run
    scripts/integrate_branches.py --branch dept/obs --branch dept/sup --gate
"""

import argparse
import subprocess
import sys

PARENT_OWNED = (".github/", "scripts/build_issue_ledger.py", "scripts/dump_cli_help.py",
                "scripts/extract_cli_module.py", "scripts/integrate_branches.py",
                "research/ISSUE_LEDGER.md")


def git(*args, check=True):
    r = subprocess.run(["git", *args], capture_output=True, text=True)
    if check and r.returncode != 0:
        raise SystemExit(f"git {' '.join(args)} failed: {r.stderr.strip()}")
    return r.stdout.strip()


def describe(branch, base_ref):
    base = git("merge-base", base_ref, branch)
    commits = git("rev-list", "--count", f"{base}..{branch}")
    files = git("diff", "--name-only", f"{base}..{branch}").splitlines()
    clashes = [f for f in files if f.startswith(PARENT_OWNED)]
    return base, commits, files, clashes


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--branch", action="append", required=True)
    ap.add_argument("--base", default="master", help="the ref to integrate onto (default: master)")
    ap.add_argument("--integration", default=None, help="integration branch name")
    ap.add_argument("--dry-run", action="store_true", help="report only, change nothing")
    ap.add_argument("--gate", action="store_true", help="run the gates on the merged tree")
    ap.add_argument("--crate", action="append", default=["podnext"],
                    help="crate to test at the gate (repeatable)")
    args = ap.parse_args()

    head = git("rev-parse", "--short", args.base)
    print(f"base {args.base} at {head}")
    plan = []
    for branch in args.branch:
        base, commits, files, clashes = describe(branch, args.base)
        plan.append((branch, base, int(commits), files, clashes))
        print(f"\n{branch}")
        print(f"  base           {base[:12]}")
        print(f"  commits to pick {commits}")
        print(f"  files touched  {len(files)}")
        for f in sorted(files):
            print(f"    {f}")
        if clashes:
            print(f"  !! touches parent-owned paths: {sorted(set(clashes))}")

    total = sum(p[2] for p in plan)
    print(f"\ntotal commits to pick: {total}")
    if any(p[4] for p in plan):
        print("refusing to integrate: a branch touched a parent-owned path", file=sys.stderr)
        return 2
    if args.dry_run:
        print("dry run: nothing changed")
        return 0

    integration = args.integration or f"integration/round-{head}"
    current = git("rev-parse", "--abbrev-ref", "HEAD")
    if current != integration:
        if git("branch", "--list", integration):
            print(f"integration branch {integration} already exists; switching to it")
            git("switch", integration)
        else:
            git("switch", "-c", integration, args.base)
    for branch, base, commits, _files, _clashes in plan:
        print(f"\ncherry-picking {commits} commit(s) from {branch}")
        shas = git("rev-list", "--reverse", f"{base}..{branch}").split()
        assert len(shas) == commits, f"{branch}: counted {commits}, listed {len(shas)}"
        before = int(git("rev-list", "--count", f"{args.base}..{integration}"))
        for sha in shas:
            r = subprocess.run(["git", "cherry-pick", sha], capture_output=True, text=True)
            if r.returncode != 0:
                print(f"CONFLICT on {sha}:\n{r.stdout}\n{r.stderr}", file=sys.stderr)
                print("resolve, `git cherry-pick --continue`, then re-run this script",
                      file=sys.stderr)
                return 3
        after = int(git("rev-list", "--count", f"{args.base}..{integration}"))
        assert after - before == commits, f"{branch}: picked {after - before}, expected {commits}"
        print(f"  ok: {after - before} commit(s) landed, branch now at {after} over {args.base}")

    if not args.gate:
        print("\nintegration tree ready; re-run with --gate to run the gates, "
              "or inspect by hand")
        return 0

    gates = [(["env", "-u", "LD_LIBRARY_PATH", "cargo", "fmt", "--all", "--", "--check"],
              "cargo fmt --all -- --check")]
    for crate in args.crate:
        gates.append((["env", "-u", "LD_LIBRARY_PATH", "cargo", "test", "-p", crate, "--lib"],
                      f"cargo test -p {crate} --lib"))
    gates.append((["env", "-u", "LD_LIBRARY_PATH", "cargo", "test", "-p", "podnext",
                   "--bin", "podnext", "--test", "cli_reference_contract"],
                  "podnext unit tests + the CLI contract"))
    gates.append((["env", "-u", "LD_LIBRARY_PATH", "python3",
                   "scripts/build_issue_ledger.py", "--offline"],
                  "the verification ledger"))
    failed = False
    for cmd, label in gates:
        r = subprocess.run(cmd, capture_output=True, text=True)
        tail = (r.stdout or r.stderr).strip().splitlines()[-3:]
        print(f"\n{label}: exit {r.returncode}")
        for line in tail:
            print(f"  {line}")
        if r.returncode != 0:
            failed = True
    print("\ngates " + ("FAILED" if failed else "passed") + f" on {integration}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
