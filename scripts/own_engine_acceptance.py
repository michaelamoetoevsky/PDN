#!/usr/bin/env python3
"""The operator's requirement, as a runnable check: a container must run WITHOUT the proot binary.

This is the acceptance gate for the goal recorded in workbench/WORKLIST.md section Z (and in the audit
as BUG-PROOT-01, workbench/asec/REPORT.md section 10):

    a container runs with the engine that ships inside podnext, not with the host `proot` binary

Today it FAILS, and that is the point: the measurement exists before the fix, so the fix's proof is that
this script flips. It is written to fail LOUDLY and by name, never to skip:

  * it refuses to run against the real store (`pdn_fixture_guard.store_root()` fails closed),
  * it proves the binary acts on the fixture before it is allowed to act at all,
  * it removes `proot` from PATH **and** points PROOT_BIN at a dead path, so a pass cannot be an accident
    of the host having proot installed,
  * it checks the GUEST's stdout and exit status, because "the command returned 0" is not evidence that
    the guest ran (an outer wrapper can exit 0 having done nothing).

Exit codes: 0 = the requirement holds, 1 = it does not (with the reason printed), 2 = REFUSING (the rig
cannot run, e.g. no binary) - a refusal is not a pass.

Run:
    cd ~/workspace/PDN && python3 scripts/own_engine_acceptance.py
"""

from __future__ import annotations

import os
import pathlib
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from pdn_fixture_guard import assert_binary_honours_store, resolve_binary, seed_fixture, store_root  # noqa: E402

CONTAINER = "own-engine-acceptance"
GUEST_MARKER = "guest-ran-inside-the-rootfs"
# Libraries the guest needs are placed HERE inside the rootfs and the guest is linked with this as its
# DT_RUNPATH, so the loader resolves them through the engine's path translation. Mirrors the recipe the
# engine's own test (crates/proot-core/tests/native_dynamic_guest.rs) proves.
RUNPATH_IN_ROOTFS = "/p7loader/lib"


def build_guest(rootfs: pathlib.Path) -> pathlib.Path:
    """Compile a DYNAMIC guest as <rootfs>/bin/sh and give the rootfs its own loader and libraries.

    Termux has no static toolchain (measured: `cc -static` fails with `cannot open crtbegin_static.o`
    and `unable to find library -lc`), so the guest is dynamically linked - which is also the honest
    case for a container, since every real guest is dynamic. The interpreter and every NEEDED library
    are copied into the rootfs AT THE SAME ABSOLUTE PATHS the binary was linked against, so a translated
    open has a real target to find. Without that copy, a loader that is not translated quietly opens the
    HOST's libraries, the guest appears to work, and the harness reports a pass it did not earn - which
    is the exact false positive this project exists to remove.

    The build fails LOUDLY when cc is missing; a harness that skips its own fixture reports a pass it
    never earned.
    """
    cc = "cc"
    dst = rootfs / "bin" / "sh"
    dst.parent.mkdir(parents=True, exist_ok=True)
    src = rootfs.parent / "guest.c"
    src.write_text(
        "#include <stdio.h>\n"
        "int main(void) { printf(\"%s\\n\", \"" + GUEST_MARKER + "\"); return 0; }\n"
    )
    proc = subprocess.run(
        [cc, "-o", str(dst), str(src), "-Wl,-rpath," + RUNPATH_IN_ROOTFS],
        capture_output=True, text=True, timeout=180,
    )
    if proc.returncode != 0:
        print(f"REFUSING: could not build the guest fixture with {cc}: {proc.stderr.strip()[:300]}")
        raise SystemExit(2)
    dst.chmod(0o755)

    # The interpreter stays at its absolute host path (the kernel opens it before the tracee runs, so it
    # cannot be translated), but every NEEDED library goes into the RUNPATH directory INSIDE the rootfs.
    # That is the recipe the engine's own test proves: libraries resolved through DT_RUNPATH are opened
    # by the guest's loader and translated, while copying Android's /system/lib64 stubs next to a
    # Termux-linked binary fails to link (measured).
    needed: list[str] = []
    interp = subprocess.run(
        ["readelf", "-l", str(dst)], capture_output=True, text=True, timeout=60
    ).stdout
    for line in interp.splitlines():
        if "Requesting program interpreter" in line:
            needed.append(line.split(":")[-1].strip().rstrip("]"))
    dyn = subprocess.run(["readelf", "-d", str(dst)], capture_output=True, text=True, timeout=60).stdout
    for line in dyn.splitlines():
        if "(NEEDED)" in line and "[" in line:
            needed.append(line.split("[", 1)[1].rstrip("]").strip())

    copied, implied = [], []
    search = [pathlib.Path(os.environ.get("PREFIX", "/data/data/com.termux/files/usr")) / "lib", pathlib.Path("/system/lib64"), pathlib.Path("/system/lib"), pathlib.Path("/lib"), pathlib.Path("/usr/lib")]
    for path in needed:
        source = pathlib.Path(path)
        if not source.exists():
            for directory in search:
                candidate = directory / pathlib.Path(path).name
                if candidate.exists():
                    source = candidate
                    break
            else:
                implied.append(path)
                continue
        if path.startswith("/") and "linker" in path:
            target = rootfs / path.lstrip("/")          # the interpreter keeps its absolute path
        else:
            target = rootfs / RUNPATH_IN_ROOTFS.lstrip("/") / pathlib.Path(path).name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(source.read_bytes())
        copied.append(f"{pathlib.Path(path).name} <- {source}")
    print(f"guest    : {dst} ({dst.stat().st_size} bytes, dynamic, RUNPATH {RUNPATH_IN_ROOTFS})")
    print(f"in rootfs: {', '.join(copied) if copied else '<none>'}")
    if implied:
        print(f"implied  : {', '.join(implied)} (no file on this host - bionic satisfies these from libc.so)")
    return dst


def main() -> int:
    try:
        binary = resolve_binary()
        store = store_root()
    except SystemExit as exc:  # the guard raises SystemExit on a refusal
        print(f"REFUSING: {exc}")
        return 2

    fixture_home = store.parent
    seed_fixture(store, [CONTAINER], copy_rootfs=False)
    # A real guest in the fixture rootfs, so a failure here means "the engine could not run it" rather
    # than "there was nothing to run".
    build_guest(store / "containers" / CONTAINER / "rootfs")
    assert_binary_honours_store(binary, store)
    print(f"binary   : {binary}")
    print(f"fixture  : {fixture_home}")
    print(f"store    : {store}")

    # Two arms. The first is the operator's criterion in its ORIGINAL form (no engine flag, so whatever
    # the default is must not need proot). The second asks the in-tree engine explicitly, which is the
    # increment that can be demonstrated today: it proves the engine path runs a real guest with no proot
    # binary anywhere, while the default still spawns proot until the interpreter limitation below is
    # addressed for real distro rootfs.
    env = dict(os.environ)
    env.update(
        {
            "PN_TEST_ROOT": str(fixture_home),
            "HOME": str(fixture_home),
            "PATH": "/nonexistent-bin",          # no proot here, and nothing else either
            "PROOT_BIN": "/nonexistent/proot",   # explicit: no fallback to a host proot
        }
    )

    results = {}
    for label, extra in (("default", []), ("engine-native", ["--engine", "native"])):
        # `--engine` is a GLOBAL argument, so it precedes the subcommand: `podnext --engine native run ...`
        argv = [binary, *extra, "run", CONTAINER, "--", "/bin/sh"]
        print(f"command  : " + " ".join(argv))
        proc = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=180)
        out = (proc.stdout or "") + (proc.stderr or "")
        ok = GUEST_MARKER in out and proc.returncode == 0
        results[label] = ok
        print(f"  [{label}] exit={proc.returncode} ran={ok}")
        print(f"  [{label}] output: {out.strip()[:300] or '<empty>'}")

    if results.get("default"):
        print("PASS: the DEFAULT path runs a container with no proot binary available to the process")
        return 0

    if results.get("engine-native"):
        print("PARTIAL: --engine native runs the guest with no proot, but the DEFAULT path still spawns")
        print("         proot, so the operator's requirement is not met yet. Blockers, measured:")
        print("           * the default still selects proot (deliberate until the remaining gaps are modelled);")
        print("           The PT_INTERP blocker that used to be listed here is CLOSED (0ab0e45) and the fork")
        print("           DESCENDANT case with it (b7c5d49): a real Debian rootfs whose interpreter is absent")
        print("           from the host now starts, and so do the external programs it executes, because the")
        print("           engine execs the guest's own interpreter with the program as its argument. What is")
        print("           still unproven is the DEFAULT path, not the capability.")
        return 1

    print("FAIL: neither the default path nor --engine native ran the guest without proot.")
    if "proot" in out.lower():
        print("      the failure names proot, so the external engine is still the execution path")
    else:
        print("      the failure does not mention proot - read the output above before concluding why")
    print("      requirement: workbench/WORKLIST.md section Z (BUG-PROOT-01, REPORT.md section 10)")
    return 1


if __name__ == "__main__":
    sys.exit(main())