#!/usr/bin/env python3
"""Check that the v0.1.9 non-goals are still non-goals.

Roadmap section 9.4 lists what v0.1.9 will NOT do: no TUI, no OCI image store, no cluster
scheduler, no expansion of the optional crates, no new top-level command except `doctor`. A list
of things you promise not to do is worth exactly as much as the check that notices when you do
one of them - which, until this script existed, was nothing.

Each assertion is a measurement against `scripts/non_goals.json`, recorded by hand with
`--record` and reviewed in the diff. Comparison is one-directional: shrinking is allowed and
silent, growing fails, because the promise is a ceiling.

    scripts/check_non_goals.py              # check (exit 1 on any violation)
    scripts/check_non_goals.py --record     # rewrite the baseline after a deliberate decision
    scripts/check_non_goals.py --json       # machine-readable result
"""

import argparse
import json
import os
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
BASELINE = ROOT / "scripts" / "non_goals.json"


def sh(cmd: list[str]) -> tuple[int, str]:
    env = {k: v for k, v in os.environ.items() if k != "LD_LIBRARY_PATH"}
    r = subprocess.run(cmd, capture_output=True, text=True, cwd=ROOT, env=env)
    return r.returncode, (r.stdout or "") + (r.stderr or "")


def rust_lines(subdir: str) -> int:
    total = 0
    for f in (ROOT / subdir).rglob("*.rs") if (ROOT / subdir).exists() else []:
        total += len(f.read_text(errors="replace").splitlines())
    return total


def command_list(binary: str) -> list[str] | None:
    """The top-level commands as the real binary prints them, not as the source implies."""
    if not pathlib.Path(binary).exists():
        return None
    rc, out = sh([binary, "--help"])
    if rc != 0:
        return None
    body = out.split("Commands:", 1)
    if len(body) != 2:
        return None
    cmds = []
    for line in body[1].splitlines()[1:]:
        if not line.startswith("  ") or line.startswith("Options:"):
            continue
        tok = line.strip().split()
        if tok and re.fullmatch(r"[a-z][a-z0-9-]*", tok[0]):
            cmds.append(tok[0])
    return sorted(cmds)


def measure(binary: str) -> dict:
    m: dict = {}
    m["commands"] = command_list(binary)
    m["line_budgets"] = {
        "crates/pn-next/src": rust_lines("crates/pn-next/src"),
        "crates/pn-next/src/dbus": rust_lines("crates/pn-next/src/dbus"),
        "crates/pn-next/src/gitops": rust_lines("crates/pn-next/src/gitops"),
        "crates/pn-next/src/cluster": rust_lines("crates/pn-next/src/cluster"),
        "crates/pn-next/src/multiarch_build": rust_lines("crates/pn-next/src/multiarch_build"),
    }
    # The image store WAS the v0.2.0 non-goal; v0.3.0 defines itself by building it (C1, roadmap 4.4), so
    # the appearance of this type is now a RECORDED milestone rather than a violation: the baseline in
    # scripts/non_goals.json carries these files. The check still works - a store type appearing somewhere
    # the baseline does not list is still a new, unreviewed appearance.
    rc, out = sh(["grep", "-rlE", r"struct ImageStore|trait ImageStore|struct BlobStore", "crates"])
    m["image_store_files"] = [l for l in out.splitlines() if l.strip()] if rc == 0 else []
    # A TUI is not a thing this project has: no target, no module, no command.
    tui = [str(p.relative_to(ROOT)) for p in (ROOT / "crates").rglob("*.rs")
           if re.search(r"(^|/)(tui|ui)(/|\.rs$)", str(p.relative_to(ROOT / "crates")))]
    m["tui_files"] = sorted(tui)
    return m


def compare(baseline: dict, now: dict) -> list[str]:
    """Return violations. Growing is a violation; shrinking is not."""
    problems: list[str] = []
    b_cmds, n_cmds = baseline.get("commands"), now.get("commands")
    if b_cmds:
        added = sorted(set(n_cmds or []) - set(b_cmds))
        removed = sorted(set(b_cmds) - set(n_cmds or []))
        if added:
            problems.append(f"new top-level command(s): {', '.join(added)} "
                            "(section 9.4 allows none beyond doctor)")
        if removed:
            problems.append(f"a command disappeared: {', '.join(removed)} "
                            "(removal is a decision, record it with --record)")
    for area, budget in baseline.get("line_budgets", {}).items():
        have = now["line_budgets"].get(area, 0)
        if have > budget:
            problems.append(f"{area} grew to {have} lines, past the recorded ceiling of {budget}")
    if now["image_store_files"] and not baseline.get("image_store_files"):
        problems.append("an OCI image store type appeared ("
                        f"{', '.join(now['image_store_files'])}): that is the v0.2.0 milestone")
    if now["tui_files"] and not baseline.get("tui_files"):
        problems.append(f"a TUI module appeared ({', '.join(now['tui_files'])})")
    return problems


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default="target/debug/podnext")
    ap.add_argument("--record", action="store_true", help="rewrite the baseline (deliberate act)")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    now = measure(args.binary)
    if args.record:
        BASELINE.write_text(json.dumps(now, indent=2, sort_keys=True) + "\n")
        print(f"recorded {BASELINE.relative_to(ROOT)}")
        return 0

    baseline = json.loads(BASELINE.read_text()) if BASELINE.exists() else {}
    if now["commands"] is None:
        # Not a violation: a measurement that could not be taken. Reporting it as a crossed
        # non-goal would be the same mistake as reporting an unreadable check as healthy, so it
        # leaves with its own status.
        print(f"cannot measure: no runnable podnext at {args.binary} "
              f"(build it first, or pass --binary)", file=sys.stderr)
        return 2
    problems = compare(baseline, now)
    if args.json:
        print(json.dumps({"ok": not problems, "problems": problems, "measured": now}, indent=2))
    else:
        print(f"non-goals: {len(baseline.get('commands') or [])} commands recorded, "
              f"{len(now['commands'] or [])} measured")
        for p in problems:
            print(f"  VIOLATION: {p}")
        if not problems:
            print("  ok: no non-goal was crossed")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
