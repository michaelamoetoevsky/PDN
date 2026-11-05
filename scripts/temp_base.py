#!/usr/bin/env python3
"""One writable temp base, correct on this device and on a CI runner.

The rule has now been got wrong **three times** in this repository, in three files, always as the same
expression:

    Path(os.environ.get("TMPDIR") or Path(os.environ.get("PREFIX", "/tmp")) / "tmp")

`/ "tmp"` binds to the *fallback* too, so a host with neither variable asks the filesystem for
`/tmp/tmp`:

    FileNotFoundError: [Errno 2] No such file or directory: '/tmp/tmp/pdn-doc-examples-44ph24wj'

On this device `$PREFIX` is always set, so the line reads correctly and the bug is invisible; it only
appears where `$PREFIX` is unset, which is every CI runner. Three files carrying one wrong rule is a
reason to have one file carrying the right one, so this module owns it.

Resolution order, and why:
  1. `$TMPDIR` - already set to `$PREFIX/tmp` under Termux, and the caller knows best.
  2. `$PREFIX/tmp` - the only writable temp directory on Android when TMPDIR is unset.
  3. the platform temp directory - a runner has `/tmp` and no `$PREFIX`. Not `/tmp/tmp`.
"""
from __future__ import annotations

import os
import tempfile
from pathlib import Path


def temp_base(create: bool = True) -> Path:
    """A writable directory for throwaway work, on both hosts.

    `create=True` makes the directory first, because every caller immediately calls mkdtemp inside it
    and a missing base is the failure this module exists to prevent.
    """
    if os.environ.get("TMPDIR"):
        base = Path(os.environ["TMPDIR"])
    elif os.environ.get("PREFIX"):
        base = Path(os.environ["PREFIX"]) / "tmp"
    else:
        base = Path(tempfile.gettempdir())
    if create:
        base.mkdir(parents=True, exist_ok=True)
    return base


def self_check() -> int:
    """The control: the doubled segment must be unreachable, on this host and on a runner's shape."""
    import subprocess
    import sys

    # The first version of this control asserted resolution into "/x/tmp" and "/p/tmp" - directories
    # that cannot be created - so the child died on mkdir and the assertion blamed the module. A control
    # has to be runnable, or it reports the control's own impossibility as a defect.
    sandbox = Path(tempfile.mkdtemp(prefix="pdn-tempbase-selfcheck-"))
    for label, env, expect in (
        ("device shape", {"PREFIX": str(sandbox / "prefix"), "TMPDIR": str(sandbox / "tmp")},
         str(sandbox / "tmp")),
        ("prefix only", {"PREFIX": str(sandbox / "prefix2")}, str(sandbox / "prefix2" / "tmp")),
        ("runner shape", {}, tempfile.gettempdir()),
    ):
        script = (
            "import sys; sys.path.insert(0, 'scripts');"
            "from temp_base import temp_base; print(temp_base())"
        )
        r = subprocess.run([sys.executable, "-c", script], capture_output=True, text=True, env={**env, "PATH": os.environ["PATH"]})
        got = r.stdout.strip()
        assert got == expect, f"{label}: expected {expect}, got {got!r}"
        assert not got.endswith("/tmp/tmp"), f"{label}: the doubled segment came back: {got}"
        if env:
            assert Path(got).is_dir(), f"{label}: {got} was not created"
    import shutil as _sh
    _sh.rmtree(sandbox, ignore_errors=True)
    print("self-check: ok - the doubled /tmp segment is unreachable in all three host shapes")
    return 0


if __name__ == "__main__":
    raise SystemExit(self_check())
