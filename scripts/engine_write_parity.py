#!/usr/bin/env python3
"""Compare the two engines on a WRITE workload, on a real rootfs, field by field.

Why this exists: until 2026-11-09 the native engine covered the read side only, so a guest could read
the rootfs but not change it. `mkdir -p /root/x && cd /root/x && echo hi > f && ln -s f l && chmod 600 f
&& cd / && rm -rf /root/x` failed on the native arm with named refusals for `SYS_mkdirat`, `SYS_chdir`
and `SYS_unlinkat`, while the proot arm ran it. Three things had to land for the two arms to agree:

  * `chdir` in the path-syscall table, with the engine's own bootstrap chdir exempted by readability
    (its argument is not a guest string at that stop),
  * a PER-TRACEe current directory, inherited by fork children, because `plan.cwd` is only the starting
    directory and `head`/`rm` are children of the shell that moved,
  * the write-side calls themselves (`mkdirat`, `unlinkat`, `fchmodat`, `utimensat`, `symlinkat`) and
    dirfd-relative names, which `rm -rf` uses through an open descriptor.

A pass means the two engines produced the same output for the same workload on the same rootfs. It does
NOT mean every write call is translated: `renameat` and `linkat` take two paths and are still refused by
name, which this harness would surface as a difference if a workload used them.
"""

from __future__ import annotations

import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pdn_fixture_guard import resolve_binary, seed_fixture, store_root  # noqa: E402

CONTAINER = os.environ.get("PDN_PARITY_CONTAINER", "realdeb")
WORKLOAD = (
    "mkdir -p /root/x && cd /root/x && echo hi > f && ln -s f l && chmod 600 f "
    "&& ls -l | head -1 && cd / && rm -rf /root/x && echo WRITE-OK"
)


def run(binary: str, store_env: dict, engine: str, script: str) -> tuple[int, str]:
    argv = [binary]
    if engine == "native":
        argv += ["--engine", "native"]
    argv += ["run", CONTAINER, "--", "/bin/sh", "-c", script]
    proc = subprocess.run(argv, env=store_env, capture_output=True, text=True, timeout=300)
    out = "\n".join(
        line
        for line in ((proc.stdout or "") + (proc.stderr or "")).splitlines()
        if not line.startswith(("NOTE:", "WARNING:"))
    )
    return proc.returncode, out


def main() -> int:
    try:
        binary = resolve_binary()
        store = store_root()
    except SystemExit as exc:
        print(f"REFUSING: {exc}")
        return 2

    # This harness needs a REAL rootfs (the workload runs a shell, mkdir, ln, chmod and rm). `seed_fixture`
    # is safe to call on a store that already holds one - it skips an existing rootfs - and copies the
    # container out of the operator's store READ-ONLY when it does not, which is the same path the other
    # harnesses take. A container with no /bin/sh afterwards is a refusal, not a silent skip.
    try:
        seed_fixture(store, [CONTAINER])
    except SystemExit as exc:
        print(f"REFUSING: {exc}")
        return 2
    guest = store / "containers" / CONTAINER / "rootfs" / "bin" / "sh"
    if not guest.exists():
        print(
            f"REFUSING: {CONTAINER} has no guest shell at {guest}; point PN_TEST_ROOT at a store with a "
            "real container (for example a read-only copy of one) and set PDN_PARITY_CONTAINER if its "
            "name differs"
        )
        return 2
    fixture = store.parent
    env = dict(os.environ)
    env.update({"PN_TEST_ROOT": str(fixture), "HOME": str(fixture)})
    env.pop("LD_LIBRARY_PATH", None)

    results = {}
    for engine in ("proot", "native"):
        code, out = run(binary, env, engine, WORKLOAD)
        results[engine] = (code, out)
        print(f"  [{engine}] exit={code}")
        for line in out.splitlines():
            print(f"    {line}")

    proot_code, proot_out = results["proot"]
    native_code, native_out = results["native"]
    if proot_code != native_code or proot_out != native_out:
        print("DIFFERENTIAL: NOT EQUAL on the write workload")
        print(f"  proot  exit={proot_code} out={proot_out!r}")
        print(f"  native exit={native_code} out={native_out!r}")
        return 1
    if "WRITE-OK" not in native_out:
        print("DIFFERENTIAL: BOTH ARMS FAILED the same way - that is not parity, it is a broken fixture")
        return 2
    print("DIFFERENTIAL: EQUAL on exit and stdout for the write workload")
    return 0


if __name__ == "__main__":
    sys.exit(main())