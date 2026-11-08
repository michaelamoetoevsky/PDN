#!/usr/bin/env python3
"""p7 step 7: is `--engine native` behaviourally equal to the proot path?

The acceptance harness (scripts/own_engine_acceptance.py) answers "can the engine run a guest with
no proot at all". This one answers the question that must be settled BEFORE the default is flipped:
given the same rootfs and the same argv, do the two engines produce the same observable result -
stdout, stderr, exit status, and the files the guest leaves behind - and at what cost in wall time?

Deliberately narrow: one fixture, one guest, both engines, three runs each. Anything the guest cannot
observe at all (namespaces, uid mapping inside the guest) is not claimed here; it is listed as NOT
COMPARED so this script cannot be read as a parity proof for more than it measured.

Exit 0 only when all four observables agree and the arms ran. Exit 1 otherwise, naming each mismatch.
Exit 2 when the fixture cannot be established (fail closed, never a pass).
"""

from __future__ import annotations

import os
import pathlib
import shutil
import statistics
import subprocess
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import pdn_fixture_guard as guard  # noqa: E402  (path set above)

CONTAINER = "engine-differential"
RUNPATH_IN_ROOTFS = "/p7loader/lib"
MARKER_IN_ROOTFS = "/differential-marker.txt"
# Written with a RELATIVE path, so its landing place IS the starting directory's effect.
RELATIVE_MARKER = "relative-marker.txt"
RUNS = 3

GUEST_SOURCE = r"""
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

/* The guest reports the four things the two engines are most likely to disagree about, so the
 * differential MEASURES them instead of listing them as uncompared. Each line is key=value, which the
 * harness parses, so a difference is named rather than buried in a blob of stdout. */
int main(int argc, char **argv) {
    char cwd[512];
    printf("differential-ok\n");
    for (int i = 0; i < argc; i++) printf("argv[%d]=%s\n", i, argv[i]);
    printf("cwd=%s\n", getcwd(cwd, sizeof cwd) ? cwd : "<none>");
    printf("envp=%s\n", getenv("PDN_DIFFERENTIAL_PROBE") ? getenv("PDN_DIFFERENTIAL_PROBE") : "<unset>");
    printf("proc=%s\n", access("/proc/self/exe", R_OK) == 0 ? "readable" : "absent");
    printf("devnull=%s\n", access("/dev/null", W_OK) == 0 ? "writable" : "absent");
    printf("sysfs=%s\n", access("/sys", R_OK) == 0 ? "readable" : "absent");
    printf("uid=%d\n", (int)getuid());
    printf("euid=%d\n", (int)geteuid());
    /* A RELATIVE path: where it lands is the starting directory's effect, without needing
     * getcwd() virtualisation - proot shows the guest "/" while the native engine shows the
     * translated host path, and only the effect is comparable. */
    FILE *r = fopen("relative-marker.txt", "w");
    if (r) { fprintf(r, "relative-write-ok\n"); fclose(r); }
    else { printf("relative-write-failed\n"); }
    FILE *f = fopen("/differential-marker.txt", "w");
    if (f) { fprintf(f, "written-by-guest\n"); fclose(f); }
    else { printf("marker-write-failed\n"); }
    fflush(stdout);
    return 7;
}
"""


def needed_entries(binary: pathlib.Path) -> tuple[list[str], list[str]]:
    """(interpreter, DT_NEEDED names) as the binary itself records them."""
    interp: list[str] = []
    for line in subprocess.run(["readelf", "-l", str(binary)], capture_output=True, text=True).stdout.splitlines():
        if "Requesting program interpreter" in line:
            interp.append(line.split(":")[-1].strip().rstrip("]"))
    needed = [
        line.split("[", 1)[1].rstrip("]").strip()
        for line in subprocess.run(["readelf", "-d", str(binary)], capture_output=True, text=True).stdout.splitlines()
        if "(NEEDED)" in line and "[" in line
    ]
    return interp, needed


def build_guest(rootfs: pathlib.Path) -> pathlib.Path:
    """Compile the guest and place everything it needs.

    Two placements on purpose, because the two engines resolve the SAME binary differently:
      * the proot arm enters the rootfs, so the interpreter and libraries must exist INSIDE it;
      * the native arm runs the binary on the host, so the interpreter must exist at its absolute
        HOST path (the kernel opens it before the tracee executes an instruction - PT_INTERP cannot
        be translated), while its libraries come from the RUNPATH, which the engine translates.
    """
    work = rootfs.parent / "guest-build"
    work.mkdir(parents=True, exist_ok=True)
    src = work / "guest.c"
    src.write_text(GUEST_SOURCE)
    dst = rootfs / "opt" / "hello"
    dst.parent.mkdir(parents=True, exist_ok=True)  # /opt: /bin is bind-mounted, see run_arm
    proc = subprocess.run(
        ["cc", "-o", str(dst), str(src), "-Wl,-rpath," + RUNPATH_IN_ROOTFS],
        capture_output=True, text=True, timeout=180,
    )
    if proc.returncode != 0:
        print(f"REFUSING: cannot build the guest with cc: {proc.stderr.strip()[:300]}")
        raise SystemExit(2)
    dst.chmod(0o755)

    interp, needed = needed_entries(dst)
    libdir = rootfs / RUNPATH_IN_ROOTFS.lstrip("/")
    libdir.mkdir(parents=True, exist_ok=True)
    prefix = pathlib.Path(os.environ.get("PREFIX", "/data/data/com.termux/files/usr"))
    search = [prefix / "lib", pathlib.Path("/system/lib64"), pathlib.Path("/system/lib"), pathlib.Path("/usr/lib")]

    placed: list[str] = []
    for path in interp:
        source = pathlib.Path(path)
        if not source.exists():
            print(f"REFUSING: the interpreter {path} does not exist on this host")
            raise SystemExit(2)
        inside = rootfs / path.lstrip("/")          # for the proot arm
        inside.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, inside)
        placed.append(f"interp {path} (host absolute + inside rootfs)")
    for name in needed:
        source = pathlib.Path(name)
        if not source.exists():
            for directory in search:
                candidate = directory / pathlib.Path(name).name
                if candidate.exists():
                    source = candidate
                    break
            else:
                print(f"REFUSING: {name} is not resolvable on this host")
                raise SystemExit(2)
        target = libdir / pathlib.Path(name).name
        target.write_bytes(source.read_bytes())
        placed.append(f"lib {pathlib.Path(name).name} <- {source}")
    print(f"guest    : {dst} ({dst.stat().st_size} bytes, RUNPATH {RUNPATH_IN_ROOTFS})")
    for line in placed:
        print(f"           {line}")
    return dst


def clear_effects(rootfs: pathlib.Path) -> None:
    for path in (rootfs / MARKER_IN_ROOTFS.lstrip("/"), rootfs / RELATIVE_MARKER):
        if path.exists():
            path.unlink()


def run_arm(label: str, argv: list[str], env: dict, rootfs: pathlib.Path) -> dict:
    """Run the same command RUNS times; the observables are taken from the last run."""
    times: list[float] = []
    last: dict = {}
    for attempt in range(RUNS):
        clear_effects(rootfs)
        started = time.monotonic()
        proc = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=180)
        times.append(time.monotonic() - started)
        marker = rootfs / MARKER_IN_ROOTFS.lstrip("/")
        relmarker = rootfs / RELATIVE_MARKER
        last = {
            "exit": proc.returncode,
            "stdout": proc.stdout or "",
            "stderr": proc.stderr or "",
            "marker": marker.read_text() if marker.exists() else None,
            # Where the RELATIVE write landed: the starting directory's effect, measurable in both engines.
            "relmarker": relmarker.read_text() if relmarker.exists() else None,
            "attempt": attempt + 1,
        }
    last["wall_s"] = {
        "min": round(min(times), 3),
        "median": round(statistics.median(times), 3),
        "max": round(max(times), 3),
    }
    return last


def main() -> int:
    try:
        binary = guard.resolve_binary()
        store = guard.store_root()
    except SystemExit as exc:
        print(f"REFUSING: {exc}")
        return 2

    fixture_home = store.parent
    guard.seed_fixture(store, [CONTAINER], copy_rootfs=False)
    rootfs = store / "containers" / CONTAINER / "rootfs"
    build_guest(rootfs)
    guard.assert_binary_honours_store(binary, store)
    print(f"binary   : {binary}")
    print(f"store    : {store}")

    # Same environment for both arms, with the real PATH so the proot binary is reachable: this test
    # compares the two engines, it does not repeat the acceptance test's "no proot anywhere" arm.
    base_env = dict(os.environ)
    base_env.update({"PN_TEST_ROOT": str(fixture_home), "HOME": str(fixture_home)})
    base_env.pop("PROOT_BIN", None)
    # Observed by the guest, so "the native path passes an empty envp" becomes a measurement.
    base_env["PDN_DIFFERENTIAL_PROBE"] = "probe-value"

    # The proot path resolves its command through `/bin/sh` inside the rootfs, and the fixture rootfs has
    # no shell (measured: `proot error: '/bin/sh' not found`). Both arms therefore get the SAME bind,
    # host `$PREFIX/bin` at `/bin`, so the shell exists and the comparison stays symmetric. The guest
    # itself is not on that bind - it lives at /opt/hello - so neither arm can accidentally run the
    # host's binary instead of the fixture's.
    prefix_bin = os.environ.get("PREFIX", "/data/data/com.termux/files/usr") + "/bin"
    bind = ["-b", f"{prefix_bin}:/bin"]
    arms = (
        ("proot (default)", [binary, "run", CONTAINER, *bind, "--", "/opt/hello"]),
        ("native", [binary, "--engine", "native", "run", CONTAINER, *bind, "--", "/opt/hello"]),
    )
    results = {}
    for label, argv in arms:
        print(f"\n=== {label} ===\n  {' '.join(argv)}")
        results[label] = run_arm(label, argv, base_env, rootfs)
        r = results[label]
        print(f"  exit   : {r['exit']}")
        print(f"  stdout : {r['stdout'].strip()[:200] or '<empty>'}")
        print(f"  stderr : {r['stderr'].strip()[:200] or '<empty>'}")
        print(f"  marker : {r['marker']!r}")
        print(f"  wall   : {r['wall_s']}")

    proot, native = results["proot (default)"], results["native"]

    def fields(arm: dict) -> dict:
        out = {}
        for line in arm["stdout"].splitlines():
            if "=" in line and not line.startswith("argv["):
                k, _, v = line.partition("=")
                out[k] = v
        return out

    fp, fn = fields(proot), fields(native)
    field_differences: list[str] = []
    print("\n=== guest-observed fields ===")
    for key in sorted(set(fp) | set(fn)):
        if key == "differential-ok":
            continue
        same = fp.get(key) == fn.get(key)
        print(f"  {key:<6}: {'same' if same else 'DIFFERS'}   proot={fp.get(key)!r} native={fn.get(key)!r}")
        # `cwd` is INFORMATIONAL, not a verdict field: proot virtualises getcwd() so its guest sees "/",
        # while the native engine translates guest paths to host paths and does not virtualise the
        # answer. Only the EFFECT of the starting directory is comparable, and `relmarker` measures it.
        if not same and key != "cwd":
            field_differences.append(key)
    print("\n=== differential ===")
    differences = []
    for key in ("exit", "stdout", "marker", "relmarker"):
        same = proot[key] == native[key]
        print(f"  {key:<7}: {'same' if same else 'DIFFERS'}   proot={proot[key]!r} native={native[key]!r}")
        if not same:
            differences.append(key)
    stderr_same = proot["stderr"] == native["stderr"]
    print(f"  stderr : {'same' if stderr_same else 'differs (diagnostics may legitimately differ)'}")
    if not stderr_same:
        print(f"           proot={proot['stderr'].strip()[:160]!r}")
        print(f"           native={native['stderr'].strip()[:160]!r}")
    print(f"  cost   : proot median {proot['wall_s']['median']}s vs native {native['wall_s']['median']}s "
          f"({RUNS} runs each)")

    # Kept honest: this list may only contain what the guest did NOT report. cwd, envp, uid/euid and
    # /proc/self/exe are MEASURED above, so they were removed from it - a list that still calls measured
    # things unmeasured understates the evidence on purpose-built grounds.
    print("\nNOT COMPARED here, so this is not a parity proof for them:")
    for item in (
        "guest-visible path virtualisation: proot reports getcwd() as \"/\" and /proc/self/exe as a guest path, the native engine reports host paths (only the EFFECT is compared here)",
        "the remaining synthesised mounts (/dev, /sys and the Android bind set) - only /proc was probed",
        "hostname, kernel release, and bind-mount readonly semantics",
        "namespaces and clone flags",
        "signals, ptrace visibility and process lifetime inside the guest",
    ):
        print(f"  * {item}")

    if differences or field_differences:
        named = ", ".join(differences + field_differences)
        print(f"\nDIFFERENTIAL: NOT EQUAL - {named} differ. Do not flip the default.")
        print("             Each one is a named gap, not a mystery: fix or model it, then re-run this harness.")
        return 1
    print("\nDIFFERENTIAL: EQUAL on exit, stdout and file effects for this guest.")
    print("             The default may only be flipped after the features listed above are compared too.")
    return 0


if __name__ == "__main__":
    sys.exit(main())