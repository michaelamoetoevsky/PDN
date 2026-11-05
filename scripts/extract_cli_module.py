#!/usr/bin/env python3
"""Move whole items out of a Rust file into a module, by brace matching rather than by line
numbers, so the same extraction works after every earlier move shifts the file.

Used for the roadmap section 9.2 CLI split, where main.rs and commands/mod.rs are being divided
by domain. The tool only moves text; making imports and visibility line up is the compiler's job
and stays visible in the diff:

    scripts/extract_cli_module.py --from crates/podnext/src/main.rs \\
        --module crates/podnext/src/commands/volume.rs --item cmd_volume_create --item cmd_volume_ls

It prints the range each item occupied and the resulting line counts, and refuses to run when an
item is not found or a destination already holds it.
"""

import argparse
import re
import sys
from pathlib import Path

DOC_OR_ATTR = re.compile(r"^\s*(///|//!|#\[|#!\[)")
NAME_AT_COL0 = re.compile(r"^(?:pub(?:\(crate\))? )?(?:async )?(?:fn|struct|enum|trait|mod) (\w+)")


def item_range(lines, name):
    """(start, end) 1-based inclusive for the item named `name`."""
    for i, line in enumerate(lines):
        m = re.match(r"^(?:pub(?:\(crate\))? )?(?:async )?(?:fn|struct|enum|trait|mod) " + re.escape(name) + r"\b", line)
        if not m:
            continue
        start = i
        # Pull in the doc comments and attributes directly above.
        while start > 0 and (DOC_OR_ATTR.match(lines[start - 1]) or lines[start - 1].strip() == ""):
            if lines[start - 1].strip() == "":
                # A blank line ends the block unless an attribute chain continues above it.
                j = start - 2
                if j >= 0 and DOC_OR_ATTR.match(lines[j]):
                    start -= 1
                    continue
                break
            start -= 1
        depth = 0
        seen_brace = False
        for j in range(i, len(lines)):
            depth += lines[j].count("{") - lines[j].count("}")
            if "{" in lines[j]:
                seen_brace = True
            if seen_brace and depth == 0:
                return start + 1, j + 1
            # A one-line item without braces (e.g. a `mod x;` declaration) ends at its semicolon.
            if not seen_brace and lines[j].rstrip().endswith(";"):
                return start + 1, j + 1
        raise SystemExit(f"unterminated item {name}")
    return None


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--from", dest="src", required=True)
    ap.add_argument("--module", dest="dst", required=True)
    ap.add_argument("--item", action="append", required=True)
    args = ap.parse_args()

    src = Path(args.src)
    dst = Path(args.dst)
    lines = src.read_text().split("\n")
    before = len(lines)

    ranges = []
    for name in args.item:
        found = item_range(lines, name)
        if not found:
            print(f"item not found: {name}", file=sys.stderr)
            return 2
        ranges.append((name, *found))

    # Overlapping ranges mean two names picked the same block; refuse rather than corrupt.
    ranges.sort(key=lambda r: r[1])
    for (n1, s1, e1), (n2, s2, e2) in zip(ranges, ranges[1:]):
        if s2 <= e1:
            print(f"ranges overlap: {n1} ({s1}-{e1}) and {n2} ({s2}-{e2})", file=sys.stderr)
            return 2

    moved = "\n\n".join("\n".join(lines[s - 1 : e]) for _, s, e in ranges)
    existing = dst.read_text() if dst.exists() else ""
    for name, _, _ in ranges:
        if re.search(r"\b" + re.escape(name) + r"\b", existing):
            print(f"{dst} already mentions {name}; moving it again would duplicate", file=sys.stderr)
            return 2
    dst.write_text(existing + moved + "\n" if existing else moved + "\n")

    for _, s, e in sorted(ranges, key=lambda r: r[1], reverse=True):
        del lines[s - 1 : e]
    src.write_text("\n".join(lines))

    for name, s, e in ranges:
        print(f"{name}: lines {s}-{e} ({e - s + 1} lines) -> {dst}")
    print(f"{src.name}: {before} -> {len(lines)} lines; {dst.name}: {len(dst.read_text().split(chr(10)))} lines")
    return 0


if __name__ == "__main__":
    sys.exit(main())
