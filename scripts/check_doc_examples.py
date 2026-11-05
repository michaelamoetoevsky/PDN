#!/usr/bin/env python3
"""Execute every fenced example in docs/src/cli-reference.md (roadmap section 9.1).

`crates/podnext/tests/cli_reference_contract.rs` gates the commands and the option/flag
tables of the reference and never runs a code block. This harness runs them: every fenced
block lands in exactly one bucket, and both counts are printed.

  executed   the block's command lines run for real. Mutating examples run against a
             throwaway `PN_TEST_ROOT`/`TMPDIR` built the way scripts/bench_container_index.py
             builds one, so a run cannot touch the user's container store.
  excluded   the block cannot run here, and the entry names why. Each `--flag` such a block
             names is still checked against the command's own `--help`, because a flag that a
             command does not have is the same fiction as an example that does not run
             (`podnext dbus list --socket` was exactly that).

A silently skipped example is not acceptable: there is no third bucket, the exclusion list is
printed, and any broken example - a failing run or a flag the tool does not have - exits 1.

In CI without the binary this prints SKIP and exits 0: a skip is not a pass, and it says so.

Usage::

    env -u LD_LIBRARY_PATH python3 scripts/check_doc_examples.py --binary $PREFIX/tmp/pdn/podnext
    PDNEXT_BIN=target/debug/podnext python3 scripts/check_doc_examples.py
    python3 scripts/check_doc_examples.py --doc /tmp/broken.md --binary ...   # negative control
"""

import argparse
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
DOC = REPO / "docs/src/cli-reference.md"

# Command paths whose documented examples cannot run on this host. Every entry names the
# reason; a block is never dropped without one. ponytail: no container factory and no registry
# stub - the examples that need an installed container or the network are declared unrunnable
# rather than faked. A new entry needs a reason of the same kind.
CANNOT_RUN = {
    ("install",): "pulls an OCI image from a registry: needs the network",
    ("login",): "needs an installed container and an interactive terminal",
    ("run",): "needs an installed container",
    ("rename",): "needs a container named in the block; only an install creates one",
    ("copy",): "needs a container named in the block; only an install creates one",
    ("sync",): "needs a container named in the block; only an install creates one",
    ("reset",): "needs a container named in the block; only an install creates one",
    ("search",): "queries a registry: needs the network",
    ("image", "pull"): "pulls from a registry: needs the network",
    ("image", "push"): "pushes to a registry: needs the network",
    ("build", "local"): "needs a Dockerfile build context and a base image on disk; the block supplies neither",
    ("config", "validate"): "needs a gateway.toml on disk; the block names ./gateway.toml, which is not provided",
    ("dbus",): "dbus serve is a foreground server and dbus list/call need a running broker",
}

ENV_PREFIX = re.compile(r"^(?:\w+=\S+\s+)*")
WORD = re.compile(r"^[a-z][a-z0-9-]*$")
REDIRECT = re.compile(r"(?:^|\s)>>?\s*(\S+)")


def fenced_blocks(text):
    """(lang, first_line, last_line, body) for every ``` fence in the document."""
    lines = text.splitlines()
    out, i = [], 0
    while i < len(lines):
        m = re.match(r"^\s*```(\S*)\s*$", lines[i])
        if not m:
            i += 1
            continue
        lang, start, j = m.group(1), i + 1, i + 1
        while j < len(lines) and not re.match(r"^\s*```\s*$", lines[j]):
            j += 1
        out.append((lang, start + 1, j, lines[start:j]))
        i = j + 1
    return out


class Cli:
    """The binary as an oracle: its `--help` says which subcommands and flags exist."""

    def __init__(self, binary, timeout):
        self.binary = str(binary)
        self.timeout = timeout
        self._help = {}

    def help_text(self, path):
        key = tuple(path)
        if key not in self._help:
            argv = [self.binary, *path, "--help"]
            try:
                p = subprocess.run(argv, capture_output=True, text=True, timeout=self.timeout)
                self._help[key] = (p.stdout + p.stderr) if p.returncode == 0 else None
            except subprocess.TimeoutExpired:
                self._help[key] = None
        return self._help[key]

    @staticmethod
    def subcommands(text):
        names, inside = set(), False
        for line in text.splitlines():
            if line.startswith("Commands:"):
                inside = True
                continue
            if inside:
                if not line.startswith(" "):
                    break
                token = line.strip().split(maxsplit=1)[0] if line.strip() else ""
                if WORD.match(token):
                    names.add(token)
        return names

    def command_path(self, tokens):
        """Leading subcommand words after `podnext`, resolved against the real command tree."""
        path, text = [], self.help_text([])
        for token in tokens[1:]:
            if text is None or not WORD.match(token) or token not in self.subcommands(text):
                break
            path.append(token)
            text = self.help_text(path)
        return path


def plan(line, cli):
    """(kind, payload, reason) for one documented command line.

    kind is "run" (payload is argv) or "excluded" (payload is the resolved command path,
    reason says why it cannot run here).
    """
    prefix = ENV_PREFIX.match(line).group(0)
    rest = line[len(prefix):]
    if not rest.startswith("podnext"):
        return "excluded", [], "not a podnext invocation"

    redirect = REDIRECT.search(rest)
    if redirect and (redirect.group(1).startswith("/") or redirect.group(1).startswith("~")):
        return (
            "excluded",
            [],
            f"redirects into the host filesystem ({redirect.group(1)}), outside the throwaway root",
        )
    if redirect:
        rest = rest[: redirect.start()]

    if "/path/to/" in rest:
        return (
            "excluded",
            [],
            "uses a placeholder path written as a literal (/path/to/rootfs); no such guest rootfs exists here",
        )

    no_optionals = re.sub(r"\[[^\]]*\]", "", rest)
    metavar = re.search(r"<([^>]+)>", no_optionals)
    if metavar:
        return (
            "excluded",
            [],
            f"usage synopsis: <{metavar.group(1)}> is a metavariable the block does not fill",
        )

    tokens = shlex.split(no_optionals)
    path = cli.command_path(tokens)
    for depth in range(len(path), 0, -1):
        reason = CANNOT_RUN.get(tuple(path[:depth]))
        if reason:
            return "excluded", path, reason
    return "run", shlex.split(prefix) + tokens, None


def extract(doc, cli):
    """(blocks, failures, flags_checked) - every block classified, flags spot-checked."""
    blocks, failures, flags_checked = [], [], 0
    for lang, first, last, body in fenced_blocks(doc):
        first_text = next((ln.strip() for ln in body if ln.strip()), "<empty>")
        if lang not in ("bash", "sh"):
            blocks.append(
                {
                    "first": first,
                    "label": first_text,
                    "status": "excluded",
                    "entries": [
                        {
                            "line": first,
                            "text": first_text,
                            "reason": f"not a shell block (```{lang or 'plain'}): sample output or configuration, no command to run",
                        }
                    ],
                }
            )
            continue
        if any(ln.lstrip().startswith("$ ") for ln in body):
            blocks.append(
                {
                    "first": first,
                    "label": first_text,
                    "status": "excluded",
                    "entries": [
                        {
                            "line": first,
                            "text": first_text,
                            "reason": "transcript: the block pairs the command with its expected output",
                        }
                    ],
                }
            )
            continue

        entries, any_run, any_excluded = [], False, False
        for offset, raw in enumerate(body):
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            lineno = first + offset
            kind, payload, reason = plan(line, cli)
            if kind == "run":
                any_run = True
                entries.append({"line": lineno, "text": line, "kind": "run", "argv": payload})
                continue
            any_excluded = True
            entries.append({"line": lineno, "text": line, "kind": "excluded", "reason": reason})
            flags = re.findall(r"--[A-Za-z][A-Za-z0-9-]*", line)
            if not flags:
                continue
            text = cli.help_text(payload)
            if text is None:
                failures.append(
                    f"L{lineno}: {line}\n    'podnext {' '.join(payload)} --help' does not run".rstrip()
                )
                continue
            flags_checked += len(flags)
            missing = [f for f in flags if f not in text]
            if missing:
                failures.append(
                    f"L{lineno}: {line}\n    'podnext {' '.join(payload)} --help' does not list: {', '.join(missing)}"
                )
        blocks.append(
            {
                "first": first,
                "label": first_text,
                "status": "executed" if any_run else "excluded",
                "partial": any_run and any_excluded,
                "entries": entries,
            }
        )
    return blocks, failures, flags_checked


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--doc", default=str(DOC))
    parser.add_argument("--binary", default=os.environ.get("PDNEXT_BIN") or "target/debug/podnext")
    parser.add_argument("--timeout", type=float, default=60.0)
    parser.add_argument("--keep", action="store_true", help="keep the throwaway store")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    binary = Path(args.binary).expanduser().resolve()
    if not (binary.is_file() and os.access(binary, os.X_OK)):
        print(f"SKIP: no podnext binary at {binary} (set --binary or PDNEXT_BIN); a skip is not a pass")
        return 0
    probe = subprocess.run([str(binary), "--version"], capture_output=True, text=True)
    version = probe.stdout.strip()
    if probe.returncode != 0:
        print(f"SKIP: {binary} --version exited {probe.returncode}, so no example can run here; a skip is not a pass")
        return 0

    cli = Cli(binary, args.timeout)
    doc_path = Path(args.doc)
    blocks, failures, flags_checked = extract(doc_path.read_text(), cli)

    base = Path(os.environ.get("TMPDIR") or Path(os.environ.get("PREFIX", "/tmp")) / "tmp")
    root = Path(tempfile.mkdtemp(prefix="pdn-doc-examples-", dir=base))
    env = dict(os.environ)
    env.pop("LD_LIBRARY_PATH", None)
    env.update(
        {
            "PN_TEST_ROOT": str(root),
            "TMPDIR": str(root / "tmp"),
            "XDG_CONFIG_HOME": str(root / "xdg"),
            "XDG_STATE_HOME": str(root / "state"),
            "XDG_RUNTIME_DIR": str(root / "run"),
        }
    )

    ran = 0
    try:
        for block in blocks:
            for entry in block["entries"]:
                if entry.get("kind") != "run":
                    continue
                try:
                    p = subprocess.run(
                        [str(binary), *entry["argv"][1:]],
                        capture_output=True,
                        text=True,
                        env=env,
                        timeout=args.timeout,
                    )
                except subprocess.TimeoutExpired:
                    failures.append(
                        f"L{entry['line']}: {entry['text']}\n    timed out after {args.timeout:.0f}s"
                    )
                    continue
                ran += 1
                if p.returncode != 0:
                    detail = (p.stderr or p.stdout).strip().splitlines()
                    failures.append(
                        f"L{entry['line']}: {entry['text']}\n    exited {p.returncode}: {detail[0] if detail else ''}"
                    )
    finally:
        if not args.keep:
            shutil.rmtree(root, ignore_errors=True)

    executed = [b for b in blocks if b["status"] == "executed"]
    excluded = [b for b in blocks if b["status"] == "excluded"]
    excluded_entries = [e for b in blocks for e in b["entries"] if e.get("kind") == "excluded"]

    print(f"doc            {doc_path}")
    print(f"binary         {binary}  ({version})")
    print(f"store          {root if args.keep else '(throwaway store, removed)'}")
    print(f"fenced blocks  {len(blocks)}")
    print(f"executed       {len(executed)} blocks, {ran} command lines run")
    print(f"excluded       {len(excluded)} blocks, {len(excluded_entries)} entries (reason per entry below)")
    print(f"flags          {flags_checked} --flag occurrences in excluded blocks matched against `--help`")
    print()
    print("executed blocks:")
    for block in executed:
        print(f"  L{block['first']:<5} {block['label'][:78]}")
    print()
    print("exclusions:")
    for block in excluded:
        for entry in block["entries"]:
            print(f"  L{entry['line']:<5} {entry['text'][:78]}")
            print(f"  {'':<5} -> {entry['reason']}")
    partial = [b for b in blocks if b.get("partial")]
    if partial:
        print()
        print("partly executed blocks (some lines ran, the rest are in the list above):")
        for block in partial:
            print(f"  L{block['first']}  {block['label'][:72]}")

    if failures:
        print()
        print(f"FAIL: {len(failures)} documented example(s) do not hold")
        for failure in failures:
            print(f"  {failure}")
        print(f"\nFAIL {len(failures)}")
    else:
        print(f"\nPASS: {len(executed)} blocks executed / {len(excluded)} excluded, 0 failures")

    if args.json:
        print(
            "JSON "
            + json.dumps(
                {
                    "blocks": len(blocks),
                    "executed_blocks": len(executed),
                    "excluded_blocks": len(excluded),
                    "commands_run": ran,
                    "excluded_entries": len(excluded_entries),
                    "flags_checked": flags_checked,
                    "failures": failures,
                }
            )
        )
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
