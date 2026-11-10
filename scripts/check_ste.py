#!/usr/bin/env python3
"""Check a Markdown file against the mechanical part of ASD-STE100.

STE (Simplified Technical English) controls vocabulary, sentence length and verb
form. Vocabulary is not checkable without a dictionary, so this gate enforces the
parts that can be counted, and says so instead of implying more:

  * sentence length - 25 words maximum, counted outside code blocks and tables
  * no contractions (`don't`, `it's`, `we'll`)
  * no speculative modals (`should`, `would`, `may`, `might`, `could`)
  * no progressive or gerund verbs (`is running`, `by using`, `Adding the file`)
  * no semicolons and no em/en dashes in prose
  * no parentheses in prose (STE writes the sentence instead)

A file passes when every check reports zero violations. `--report` prints every
offending line so a violation can be fixed, not just counted.

Exit codes: 0 clean, 1 violations found, 2 cannot measure (no input, unreadable).
Zero observations are NOT a pass, so an empty file set exits 2.
"""

import glob
import re
import sys
import pathlib

MAX_WORDS = 25

CONTRACTION = re.compile(r"\b(?:can't|don't|doesn't|didn't|isn't|aren't|won't|wouldn't|"
                         r"shouldn't|couldn't|it's|that's|we'll|we've|you'll|you've|I'm)\b", re.I)
MODAL = re.compile(r"\b(?:should|would|may|might|could)\b", re.I)
PROGRESSIVE = re.compile(r"\b(?:is|are|was|were|be|been|being)\s+(\w+ing)\b", re.I)
GERUND_BY = re.compile(r"\b(?:by|before|after|without|instead\s+of)\s+(\w+ing)\b", re.I)
GERUND_START = re.compile(r"^\s*[-*]?\s*(\w+ing)\b")
SEMICOLON = re.compile(r";")
DASH = re.compile(r"[\u2014\u2013]")
PAREN = re.compile(r"\([^)]{3,}\)")

INLINE_CODE = re.compile(r"`[^`]*`")
LINK_TARGET = re.compile(r"\]\([^)]*\)")


def prose_lines(text):
    """Yield (line_number, prose) for lines that are prose, not code or structure."""
    in_fence = False
    for number, raw in enumerate(text.split("\n"), start=1):
        stripped = raw.strip()
        if stripped.startswith("```"):
            in_fence = not in_fence
            continue
        if in_fence or not stripped:
            continue
        if stripped.startswith("|") or stripped.startswith("#") and stripped.count(" ") > 8:
            continue
        if stripped.startswith(("    ", "\t")):
            continue
        line = INLINE_CODE.sub("CODE", raw)
        line = LINK_TARGET.sub("]", line)
        yield number, line


def sentences(prose):
    parts = re.split(r"(?<=[.!?:])\s+", prose.strip())
    return [p for p in parts if p.strip()]


def check(path):
    text = pathlib.Path(path).read_text()
    if not text.strip():
        return None
    out = []
    for number, line in prose_lines(text):
        for needle, label in ((CONTRACTION, "contraction"), (MODAL, "modal"),
                              (SEMICOLON, "semicolon"), (DASH, "dash"),
                              (PAREN, "parentheses")):
            if needle.search(line):
                out.append((number, label, line.strip()[:90]))
        for pattern, label in ((PROGRESSIVE, "progressive"), (GERUND_BY, "gerund")):
            match = pattern.search(line)
            if match:
                out.append((number, label + ":" + match.group(1), line.strip()[:90]))
        match = GERUND_START.match(line)
        if match:
            out.append((number, "gerund-start:" + match.group(1), line.strip()[:90]))
        for sentence in sentences(line):
            words = len(re.findall(r"[\w'-]+", sentence))
            if words > MAX_WORDS:
                out.append((number, f"long:{words}w", sentence.strip()[:90]))
    return out


def main(argv):
    report = "--report" in argv
    paths = [a for a in argv[1:] if not a.startswith("-")]
    if not paths:
        print("cannot measure: no files given", file=sys.stderr)
        return 2
    failed = 0
    measured = 0
    for pattern in paths:
        matches = sorted(glob.glob(pattern)) or sorted(glob.glob(pattern, recursive=True))
        if not matches:
            print(f"cannot measure: {pattern} matched nothing", file=sys.stderr)
            return 2
        for name in matches:
            path = pathlib.Path(name)
            found = check(path)
            if found is None:
                print(f"cannot measure: {path} is empty", file=sys.stderr)
                return 2
            measured += 1
            total = len(found)
            verdict = "PASS" if total == 0 else "FAIL"
            if total:
                failed += 1
            print(f"{verdict} {path} ({total} violations)")
            if report:
                for number, label, snippet in found:
                    print(f"    {path}:{number}  {label}  {snippet}")
    if measured == 0:
        print("cannot measure: no file was read", file=sys.stderr)
        return 2
    print(f"measured {measured} file(s), {failed} with violations")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))