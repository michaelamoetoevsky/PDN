#!/usr/bin/env python3
"""Dump every `--help` this CLI can produce, so a refactor can be proven byte-identical.

Roadmap section 9.2 requires the CLI to be split by domain with `--help` byte-identical
afterwards. "Looks the same" is not a check, so this walks the whole command tree and prints
each help screen under a header:

    podnext <binary> > /tmp/help-after.txt
    diff /tmp/help-before.txt /tmp/help-after.txt

Exit status is 0 when every help screen was produced. Run it with the same environment for a
before/after pair: some screens print resolved paths (the daemon's defaults follow HOME/XDG).
"""

import subprocess
import sys

TOP_LEVEL = ["podnext"]


def help_text(binary: str, args: list) -> str:
    out = subprocess.run(
        [binary, *args, "--help"], capture_output=True, text=True, timeout=60
    )
    if out.returncode != 0:
        return f"<<exit {out.returncode}>>\n{out.stdout}{out.stderr}"
    return out.stdout


def subcommands(text: str) -> list:
    """The command names in this help screen's Commands: block."""
    names = []
    in_block = False
    for line in text.splitlines():
        if line.startswith("Commands:"):
            in_block = True
            continue
        if in_block:
            if not line.strip():
                break
            stripped = line.strip()
            if stripped.startswith("-") or not stripped:
                continue
            name = stripped.split()[0]
            # clap wraps long descriptions onto the next line; only the first token counts.
            if name and name[0].isalpha() and name not in ("help",):
                names.append(name)
    return names


def main() -> int:
    binary = sys.argv[1] if len(sys.argv) > 1 else "target/debug/podnext"
    seen = {}
    # Breadth-first over the whole tree: top level, then each subcommand, then theirs.
    frontier = [[]]
    for _depth in range(4):
        next_frontier = []
        for args in frontier:
            key = " ".join(args)
            if key in seen:
                continue
            text = help_text(binary, args)
            seen[key] = text
            for name in subcommands(text):
                candidate = [*args, name]
                if " ".join(candidate) not in seen:
                    next_frontier.append(candidate)
        if not next_frontier:
            break
        frontier = next_frontier

    for key in sorted(seen):
        print(f"===== podnext {key} --help =====")
        print(seen[key], end="" if seen[key].endswith("\n") else "\n")
    print(f"===== {len(seen)} help screens =====", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
