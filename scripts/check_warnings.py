#!/usr/bin/env python3
"""Count compiler warnings and refuse to let the total grow.

Nothing gated warnings, so the extraction that took `commands/mod.rs` from 3,719 lines to 813 left
blocks of unused imports behind and no check noticed: the split reads as clean while 290 warnings
scroll past every build. A warning is not an error, which is exactly why it needs a number.

This is a ratchet, not a cleanup. `scripts/warning_baseline.json` records what each target emitted on
a stated toolchain; the gate fails when a target goes above its recorded number. Numbers only move
down, and a deliberate re-record is a visible commit, not a quiet edit.

  compare : measure and compare against the baseline       (0 within, 1 grew, 2 cannot measure)
  record  : measure and rewrite the baseline entry          (only with a reason to move it)
  self-check : exercise the decision on fixed data, no cargo

  --cold          measure with a fresh target directory, so nothing is served from cache
  --from-log FILE take the counts from a saved `cargo check` stderr instead of running cargo
  --toolchain KEY select the baseline entry (`--toolchain show` lists what is recorded)

Measured on a WARM target directory, `cargo check` re-emits nothing for targets it does not rebuild,
so the counts fall towards zero. That direction cannot produce a false failure, but zero is not
evidence of a clean tree - which is why CI measures cold and why `self-check` uses fixed data rather
than a cargo run to prove the gate fires.

**The toolchain is part of the measurement, and the runner's is not ours.** rustc adds and removes
lints between versions, and `dtolnay/rust-toolchain@stable` moves on its own, so a baseline is stored
per toolchain. On a toolchain with no recorded entry this exits **2: cannot measure** - it prints the
counts it measured and claims nothing. Exit 2 is never a pass.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASELINE = os.path.join(ROOT, "scripts", "warning_baseline.json")

# `warning: `pn-next` (lib) generated 208 warnings` - cargo's own per-target summary, which is
# authoritative and survives the JSON/human output switch.
# Cargo writes either "generated N warnings" at end of line or "generated N warnings (run
# `cargo fix ...`)". Anything else that mentions a count is prose, not a count.
SUMMARY = re.compile(r"^warning: `([^`]+)` \(([^)]+)\) generated (\d+) warnings?(?: \(|$)")

# Cargo colours its diagnostics when CARGO_TERM_COLOR is set, and the workflow sets it to `always` -
# so the runner emitted `\x1b[1m\x1b[33mwarning\x1b[0m\x1b[1m: ...`, the anchored regex matched
# nothing, and the gate measured an EMPTY set on every CI run while working here, where the variable
# is unset. Both halves are fixed: escapes are stripped before matching, and the child build is told
# to emit plain text. A gate that reads its input as text must not depend on the terminal's decoration.
ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")


def plain(text: str) -> str:
    return ANSI.sub("", text)


def env() -> dict:
    # The TUI here redirects LD_LIBRARY_PATH, which breaks any python that inherits it; cargo is
    # unaffected but the child processes of a test run are not. Dropping it is harmless elsewhere.
    e = {k: v for k, v in os.environ.items() if k != "LD_LIBRARY_PATH"}
    # Deterministic parsing regardless of what the caller's CI configuration asks for.
    e["CARGO_TERM_COLOR"] = "never"
    return e


VERSION = re.compile(r"(cargo|rustc)\s+(\d+\.\d+\.\d+)")


def toolchain() -> str:
    """A key for the ceilings: host triple plus version NUMBERS.

    The first version used the whole version strings verbatim, so this device (`rustc 1.99.1
    (58cf8f9c3 2026-10-13) (built from a source tarball)`) and a CI runner
    (`rustc 1.99.1 (58cf8f9c3 2026-10-13)`) looked like different toolchains and the ratchet sat
    idle in CI - the exit-2 path firing for a cosmetic reason instead of a real one. The build hash
    and date do not change the lint set; the version does. The host triple does matter, because
    which cfg branches compile (and therefore which warnings exist) depends on the target.
    """
    c = subprocess.run(["cargo", "--version"], capture_output=True, text=True, env=env()).stdout
    r = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, env=env()).stdout
    nums = {m.group(1): m.group(2) for m in VERSION.finditer(c + "\n" + r)}
    host = ""
    for line in r.splitlines():
        if line.startswith("host:"):
            host = line.split(":", 1)[1].strip()
    if not nums.get("cargo") or not nums.get("rustc"):
        # Unrecognisable version output is a measurement problem, not a reason to invent a key.
        return ""
    return f"{host} cargo {nums['cargo']} rustc {nums['rustc']}"


def counts_from(text: str) -> dict[str, int]:
    counts: dict[str, int] = {}
    for line in plain(text).splitlines():
        m = SUMMARY.match(line)
        if m:
            key = f"{m.group(1)} ({m.group(2)})"
            counts[key] = counts.get(key, 0) + int(m.group(3))
    return counts


def temp_base() -> str:
    """Delegates to scripts/temp_base.py, the single owner of this rule.

    The rule has been got wrong three times in this repository (see that module); keeping a second copy
    here would be a fourth chance to get it wrong.
    """
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    from temp_base import temp_base as _base
    return str(_base())


def measure(cold: bool) -> dict[str, int]:
    target = os.path.join(ROOT, "target")
    tmp = None
    if cold:
        base = temp_base()
        os.makedirs(base, exist_ok=True)
        try:
            tmp = tempfile.mkdtemp(prefix="pdn-warn-", dir=base)
        except OSError as e:
            print(f"cannot measure: no writable temp directory to build in ({e})")
            sys.exit(2)
        target = tmp
    try:
        cmd = ["cargo", "check", "--workspace", "--locked", "--target-dir", target]
        r = subprocess.run(cmd, capture_output=True, text=True, env=env(), cwd=ROOT)
        if r.returncode != 0:
            sys.stderr.write(f"cargo check failed (rc={r.returncode}); the count would be meaningless\n")
            sys.stderr.write(r.stderr[-2000:])
            sys.exit(2)
        return counts_from(r.stderr)
    finally:
        if tmp:
            shutil.rmtree(tmp, ignore_errors=True)


def compare(measured: dict[str, int], base: dict) -> tuple[int, list[str]]:
    """Pure: measured counts against one baseline entry. Returns (exit code, lines to print).

    Separate from measurement so the decision can be exercised on fixed data, and because a warm
    cargo run measures zero: a gate whose only evidence is a warm run cannot fail, and one that
    cannot fail is worse than none.
    """
    lines: list[str] = []
    grew = [(c, was, measured.get(c, 0))
            for c, was in base["crates"].items() if measured.get(c, 0) > was]
    if grew:
        lines.append("WARNINGS GREW:")
        lines += [f"  {crate}: {was} -> {now}" for crate, was, now in sorted(grew)]
        lines.append("A warning is not an error, which is why it needs a number. Fix it, or record the")
        lines.append("new number deliberately with the reason - do not raise the ceiling quietly.")
        return 1, lines
    lowered = [(c, base["crates"][c], measured.get(c, 0))
               for c in base["crates"] if measured.get(c, 0) < base["crates"][c]]
    if lowered:
        lines.append("lower than the baseline (good - re-record to keep the ratchet tight):")
        lines += [f"  {crate}: {was} -> {now}" for crate, was, now in sorted(lowered)]
    lines.append("warnings: within the recorded ceiling")
    return 0, lines


def entry_for(doc: dict, tc: str) -> dict | None:
    """The baseline entry for a toolchain, or None when this toolchain has none.

    A flat {toolchain, crates} file from before the keying is still read, so an in-flight run that
    started against the old format is not invalidated by this change.
    """
    if "baselines" in doc:
        return doc["baselines"].get(tc)
    if doc.get("toolchain") == tc:
        return {"total": doc.get("total"), "crates": doc.get("crates", {})}
    return None


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["compare", "record", "self-check"])
    ap.add_argument("--cold", action="store_true", help="fresh target dir, nothing cached")
    ap.add_argument("--from-log", metavar="FILE", help="read counts from a saved cargo check stderr")
    ap.add_argument("--toolchain", metavar="KEY", help="baseline entry to use, or 'show'")
    ap.add_argument("--baseline", metavar="FILE", default=BASELINE,
                    help="ceilings file to read (default: scripts/warning_baseline.json)")
    args = ap.parse_args()

    if args.mode == "self-check":
        return self_check()

    try:
        doc = json.load(open(args.baseline))
    except OSError as e:
        print(f"cannot compare: {e}")
        return 2
    if args.toolchain == "show":
        for k in sorted(doc.get("baselines", {})):
            print(f"{doc['baselines'][k]['total']:5d}  {k}")
        return 0
    tc = args.toolchain or toolchain()
    if not tc:
        print("cannot measure: could not read a cargo/rustc version to key the ceilings by")
        return 2

    if args.from_log:
        try:
            with open(args.from_log, errors="replace") as f:
                measured = counts_from(f.read())
        except OSError as e:
            print(f"cannot measure: {e}")
            return 2
        if not measured:
            print(f"cannot measure: {args.from_log} holds no cargo summary lines")
            return 2
    else:
        measured = measure(args.cold)
    total = sum(measured.values())

    # An EMPTY measurement is not a clean tree - it is a measurement that did not happen. Without
    # this guard it flows into the comparison as zero for every target and PASSES against any
    # ceiling, which is the same trap as a warm build wearing a different coat. Seen for real: a CI
    # runner produced no summary lines at all and the only symptom was an empty list under a
    # heading that promised "what the tree emits here".
    if not measured:
        print("cannot measure: cargo reported no per-target warning summaries")
        print(f"  source: {args.from_log or ('cargo check --cold' if args.cold else 'cargo check')}")
        if args.from_log:
            print(f"  the file held {os.path.getsize(args.from_log)} bytes")
        print("  An empty measurement is not evidence of zero warnings. Check that the build actually")
        print("  ran and emitted diagnostics (target dir writable, toolchain resolvable).")
        return 2

    if args.mode == "record":
        doc.setdefault("baselines", {})[tc] = {"cold": args.cold, "total": total, "crates": measured}
        doc["note"] = ("Warning ceilings. One entry per toolchain, because rustc's lint set changes "
                       "between versions and the CI runner uses `stable`, which moves on its own.")
        with open(BASELINE, "w") as f:
            json.dump(doc, f, indent=2, sort_keys=True)
            f.write("\n")
        print(f"recorded: {total} warnings on {tc} (cold={args.cold})")
        for k, v in sorted(measured.items(), key=lambda kv: -kv[1]):
            print(f"  {v:4d}  {k}")
        return 0

    base = entry_for(doc, tc)
    source = f" (from {args.from_log})" if args.from_log else ""
    if base is None:
        print(f"cannot compare: no baseline recorded for this toolchain{source}")
        print(f"  this toolchain: {tc}")
        print("  recorded entries:", ", ".join(sorted(doc.get("baselines", {}))) or "(none)")
        print("  measured now (not a verdict, just what the tree emits here):")
        for k, v in sorted(measured.items(), key=lambda kv: -kv[1]):
            print(f"    {v:4d}  {k}")
        print("  exit 2: cannot measure. Record this toolchain deliberately, or the comparison is fiction.")
        return 2

    rc, lines = compare(measured, base)
    print(f"measured: {total} warnings{source} (ceiling {base['total']} on this toolchain, cold={args.cold})")
    for line in lines:
        print(line)
    return rc


def self_check() -> int:
    """The control the gate needs: prove it fires on growth, and stays quiet on equality.

    Written after a first attempt at the control used a warm cargo run, measured zero everywhere and
    would have "passed" while proving nothing.
    """
    base = {"total": 30, "crates": {"a (lib)": 20, "b (lib)": 10}}
    assert compare({"a (lib)": 20, "b (lib)": 10}, base)[0] == 0, "equality must pass"
    rc, lines = compare({"a (lib)": 21, "b (lib)": 10}, base)
    assert rc == 1 and any("a (lib): 20 -> 21" in l for l in lines), "growth must fail and name it"
    assert compare({"a (lib)": 19, "b (lib)": 10}, base)[0] == 0, "fewer must pass"
    rc, lines = compare({}, base)
    assert rc == 0 and any("lower than the baseline" in l for l in lines), "a warm run must not fail"
    assert entry_for({"baselines": {"X": base}}, "X") is base, "keyed lookup"
    assert entry_for({"baselines": {"X": base}}, "Y") is None, "an unrecorded toolchain is not a pass"
    assert entry_for({"toolchain": "X", "crates": base["crates"]}, "X") is not None, "reads the old flat file"
    colored = "\x1b[1m\x1b[33mwarning\x1b[0m\x1b[1m: `pn-next` (lib) generated 208 warnings\x1b[0m\n"
    assert counts_from(colored) == {"pn-next (lib)": 208}, "a coloured diagnostic must still parse"
    real = ("warning: `pn-next` (lib) generated 208 warnings\n"
            'warning: `podnext` (bin "podnext") generated 50 warnings (run `cargo fix`)\n'
            "warning: `podnext` (bin \"podnext\") generated 1 warning\n")
    c = counts_from(real)
    assert c == {"pn-next (lib)": 208, 'podnext (bin "podnext")': 51}, c
    assert not SUMMARY.match("warning: `pn-next` (lib) was generated 208 warnings"), "word order"
    assert not SUMMARY.match("6 warnings were generated by `pn-next` (lib)"), "prose, not a count"
    assert VERSION.findall("cargo 1.99.1 (5f94df478 2026-08-27)") == [("cargo", "1.99.1")], "version parse"
    assert VERSION.findall("rustc 1.99.1 (58cf8f9c3 2026-10-13) (built from a source tarball)") == [
        ("rustc", "1.99.1")], "the Termux parenthetical must not change the key"
    base_dir = temp_base()
    assert not base_dir.endswith("/tmp/tmp"), f"temp base must not double the tmp segment: {base_dir}"
    if os.environ.get("PREFIX"):
        assert base_dir.startswith(os.environ["PREFIX"]), base_dir
    print("self-check: ok - fires on growth, quiet on equality, sums per target, refuses an unknown toolchain")
    print(f"  temp base here: {base_dir}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
