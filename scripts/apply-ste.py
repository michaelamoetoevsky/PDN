#!/usr/bin/env python3
"""Apply verified STE rewrites from the scratch tree into the repository.

The invariant is that the STRUCTURE must equal the ORIGINAL file, not the current
repository file. Comparing against the current file was wrong: after a first pass
already replaced it, a later and better rewrite of the same document looks like a
structural difference from the file in the tree, so a correct rewrite is refused.

So the source of truth is the pre-STE blob in git, read with `git show <base>:<path>`.
A file is copied only when BOTH checks pass:
  * the repository's own gate reports 0 violations for the rewrite
  * the structure equals the ORIGINAL: same count of fenced blocks, headings and
    table rows, and fenced text byte-identical
Anything else is reported and left alone, so a partial or wrong rewrite can never
land. Reads only; writes only the approved files.

Usage: apply-ste.py [base-commit]   default base: b3f69d6^ (before the STE work)
"""
import pathlib
import subprocess
import sys

repo = pathlib.Path("/data/data/com.termux/files/home/workspace/PDN")
scratch = pathlib.Path("/data/data/com.termux/files/home/.hermes/cache/scratch/ste")
gate = repo / "scripts" / "check_ste.py"
temp = pathlib.Path("/data/data/com.termux/files/usr/tmp/ste-original.md")
base = sys.argv[1] if len(sys.argv) > 1 else "b3f69d6^"


def shape(path):
    lines = pathlib.Path(path).read_text().split("\n")
    fences = [i for i, l in enumerate(lines) if l.strip().startswith("```")]
    blocks = ["\n".join(lines[a:b + 1]) for a, b in zip(fences[0::2], fences[1::2])]
    return (len(fences), sum(1 for l in lines if l.startswith("#")),
            sum(1 for l in lines if l.strip().startswith("|")), blocks)


applied = unchanged = skipped = 0
for out in sorted(scratch.rglob("*.md")):
    rel = out.relative_to(scratch)
    if rel.parts[0] == "parts":
        continue
    original = subprocess.run(["git", "show", f"{base}:{rel}"], cwd=repo,
                              capture_output=True, text=True)
    if original.returncode != 0:
        print(f"SKIP-NOSOURCE {rel}")
        skipped += 1
        continue
    target = repo / rel
    if target.exists() and target.read_bytes() == out.read_bytes():
        unchanged += 1
        continue
    ok = subprocess.run([sys.executable, str(gate), str(out)], capture_output=True, text=True)
    if ok.returncode != 0:
        print(f"SKIP-GATE     {rel}  {ok.stdout.strip().splitlines()[-1]}")
        skipped += 1
        continue
    temp.write_text(original.stdout)
    s_orig, s_new = shape(temp), shape(out)
    if s_orig[:3] != s_new[:3] or s_orig[3] != s_new[3]:
        print(f"SKIP-STRUCT   {rel}  fences {s_orig[0]}/{s_new[0]} headings {s_orig[1]}/{s_new[1]} "
              f"rows {s_orig[2]}/{s_new[2]} code-same={s_orig[3] == s_new[3]}")
        skipped += 1
        continue
    target.write_bytes(out.read_bytes())
    print(f"APPLIED       {rel}")
    applied += 1

print(f"base {base}: applied {applied}, unchanged {unchanged}, skipped {skipped}")
