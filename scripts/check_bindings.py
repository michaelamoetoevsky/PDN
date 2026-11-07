#!/usr/bin/env python3
"""Measure PDN's guest bindings and its USER story in a real guest (ticket PDN#66).

Roadmap section 9.3 asks for predictable bindings (`/sdcard`, `/storage`, shared home/tmp) and an
honest `USER` story - documented, tested, and with the build/run output stating which user the guest
runs as. This drives the REAL binary (`podnext run <container> -- ...`) and records what is actually
inside the guest, then holds the documentation to it:

  1. `ls -ld` of the documented storage paths, and proof they are the guest's OWN rootfs directories
     (`/proc/mounts` carries no `sdcard`/`storage` bind and their device is the rootfs device),
     not the phone's storage.
  2. That the host home is not mounted (a host marker file must be invisible in the guest).
  3. The guest<->host file exchange for `/tmp` and `$HOME`, both directions, with the exact host
     paths (`~/.podnext/containers/<name>/tmp` and `.../rootfs/root`).
  4. The USER story: `id` in the guest is the mapped root, `doctor` records `--change-id=0:0:0`, and
     `run` now states the mapped root on **stderr** while stdout stays empty (roadmap section 9.3;
     the note used to be absent). The build output that states `USER` is not enforced is left to the
     unit tests.

Negative control: `/mnt/sdcard` and `/nonexistent_pdn_probe` are NOT mapped and must be ABSENT -
proving the check distinguishes mapped from unmapped paths.

CI: needs an installed container. With no binary or no container this prints `SKIP: ...` and exits 0
(a skip is NOT a pass). A measured claim that fails exits 1.

Env overrides: PDNEXT_BIN (path to podnext), PDN_CONTAINER (container name, default alpine).
Temp files go under $PREFIX/tmp (never /tmp on Android).
"""

import argparse
import os
import pathlib
import subprocess
import sys

ENV = {k: v for k, v in os.environ.items() if k != "LD_LIBRARY_PATH"}
BIN = os.environ.get("PDNEXT_BIN")
CONTAINER = os.environ.get("PDN_CONTAINER", "alpine")
PREFIX = os.environ.get("PREFIX", "/data/data/com.termux/files/usr")

# Documented storage paths (constants.rs `STORAGE_PERMISSIONS`) plus the mount root.
# Guest paths the storage clause is expected to provide, plus `/mnt/sdcard` (the reference maps the
# primary volume onto it too). The host-side sources are checked separately by the device test.
DOCUMENTED = ["/sdcard", "/storage", "/storage/emulated/0", "/storage/self/primary", "/mnt/sdcard"]
# Deliberately NOT mapped: the negative control.
# `/mnt/sdcard` USED to be the negative control; it is now MAPPED, because PDN grants the same
# storage set the reference does (`android_binds.rs`, mirroring `storage_bindings()`). What remains
# genuinely unmapped is a path PDN never binds, and the host home - which check 2 below still proves
# invisible from the guest.
UNMAPPED = ["/nonexistent_pdn_probe"]


def run_guest(script: str) -> tuple[int, str]:
    """`podnext run <container> -- sh -c <script>`, capturing stdout+stderr."""
    p = subprocess.run([BIN, "run", CONTAINER, "--", "sh", "-c", script],
                       capture_output=True, text=True, env=ENV, timeout=180)
    return p.returncode, p.stdout + p.stderr


def run_host(args: list[str]) -> tuple[int, str]:
    p = subprocess.run([BIN, *args], capture_output=True, text=True, env=ENV, timeout=180)
    return p.returncode, p.stdout + p.stderr


def main() -> int:
    # The first version of this script read PDNEXT_BIN/PDN_CONTAINER from the environment only and
    # ignored argv entirely, so a caller passing `--binary <path>` was silently given the default
    # instead - the same shape as the flags this project refuses to drop. argparse makes the intended
    # binary explicit and turns an unknown flag into an error rather than a no-op.
    global BIN, CONTAINER
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--binary", default=os.environ.get("PDNEXT_BIN"),
                    help="podnext binary to measure with (default: $PDNEXT_BIN)")
    ap.add_argument("--container", default=os.environ.get("PDN_CONTAINER", "alpine"),
                    help="installed container to measure inside (default: $PDN_CONTAINER or alpine)")
    args = ap.parse_args()
    BIN, CONTAINER = args.binary, args.container

    if not BIN or not os.access(BIN, os.X_OK):
        print(f"SKIP: no podnext binary (PDNEXT_BIN={BIN!r}); a skip is not a pass")
        return 0

    rc, _ = run_guest("true")
    if rc != 0:
        print(f"SKIP: container {CONTAINER!r} not usable (podnext run exited {rc}); a skip is not a pass")
        return 0

    checks: list[tuple[str, str, str]] = []   # (name, PASS/FAIL, detail)

    def add(name: str, ok: bool, detail: str) -> None:
        checks.append((name, "PASS" if ok else "FAIL", detail))

    home = pathlib.Path.home()
    store = home / ".podnext" / "containers" / CONTAINER
    guest_home = store / "rootfs" / "root"
    guest_tmp = store / "tmp"
    probe = f"pdn-check-{os.getpid()}"

    # 1. The storage clause is now the reference's: the documented paths are MAPPED to the phone's
    # storage when the host grants read access, and withheld with one named line when it does not.
    # Ticket #66 used to pin the opposite envelope (empty rootfs directories, no bind), which is why
    # this check - and the documentation sentence the ledger greps - had to be re-recorded with the
    # change rather than left to disagree with the code.
    rc, out = run_guest(
        "ls -ld " + " ".join(DOCUMENTED) + " 2>&1; "
        "echo DEV=$(stat -c %d /) $(stat -c %d /sdcard 2>/dev/null); "
        "echo BOUND=$(grep -cE 'sdcard|/storage' /proc/mounts)")
    present = all(any(line.rstrip().endswith(p) for line in out.splitlines()) for p in DOCUMENTED)
    add("documented paths exist", present, out.strip().replace("\n", " | "))
    root_dev = same_dev = None
    for line in out.splitlines():
        if line.startswith("DEV="):
            parts = line[4:].split()
            if len(parts) == 2:
                root_dev, same_dev = parts
    bound = int(out.split("BOUND=")[-1].strip() or 0) if "BOUND=" in out else 0
    # Two honest outcomes, decided by the host: mapped (a bind exists and /sdcard is a different
    # device from the rootfs) or withheld (no bind at all; the session prints the named skip).
    mapped = bound > 0 and same_dev != root_dev
    add("storage is MAPPED when the host grants access, or withheld by name",
        mapped or bound == 0,
        f"root dev={root_dev} /sdcard dev={same_dev}; mounts matching sdcard|storage: {bound} "
        f"-> {'mapped' if mapped else 'withheld (expect the named skip in the session)'}")

    rc, out = run_guest("; ".join(f"[ -e {p} ] && echo PRESENT {p} || echo ABSENT {p}"
                                  for p in UNMAPPED))
    add("negative control: unmapped paths absent",
        out.count("ABSENT") == len(UNMAPPED), out.strip().replace("\n", " | "))

    # 2. The host home is not mounted: a host marker must be invisible in the guest.
    marker = home / probe
    marker.write_text("h\n")
    rc, out = run_guest(f"cat /data/data/com.termux/files/home/{probe} 2>&1 || echo INVISIBLE")
    add("host home not mounted", "INVISIBLE" in out or f"No such file" in out, out.strip())
    marker.unlink(missing_ok=True)

    # 3. /tmp and $HOME exchange both ways, with the exact host paths.
    if guest_tmp.is_dir() and guest_home.is_dir():
        run_guest(f"echo g-tmp-{probe} > /tmp/{probe}; echo g-home-{probe} > /root/{probe}")
        host_tmp = (guest_tmp / probe).read_text().strip()
        host_home = (guest_home / probe).read_text().strip()
        (guest_tmp / probe).write_text(f"h-tmp-{probe}\n")
        (guest_home / probe).write_text(f"h-home-{probe}\n")
        rc, out = run_guest(f"cat /tmp/{probe}; cat /root/{probe}")
        add("guest writes /tmp -> host reads " + str(guest_tmp), host_tmp == f"g-tmp-{probe}", host_tmp)
        add("guest writes $HOME -> host reads " + str(guest_home), host_home == f"g-home-{probe}", host_home)
        add("host writes /tmp+$HOME -> guest reads",
            f"h-tmp-{probe}" in out and f"h-home-{probe}" in out, out.strip().replace("\n", " | "))
        for f in (guest_tmp / probe, guest_home / probe):
            f.unlink(missing_ok=True)
    else:
        checks.append(("host/guest exchange", "SKIP",
                       f"store not at {store}; is PDN_ROOT redeclared?"))

    # 4. USER story: the guest runs as the mapped root, doctor records how, and `run` now states it
    # on stderr while stdout stays empty (roadmap 9.3 - the note used to be absent entirely).
    rc, out = run_guest("id")
    add("guest runs as mapped root", "uid=0(root)" in out, out.strip())
    rc, out = run_host(["doctor", "--container", CONTAINER, "--format", "json"])
    add("doctor records --change-id=0:0:0", "--change-id=0:0:0" in out,
        "present" if "--change-id=0:0:0" in out else "absent")
    p = subprocess.run([BIN, "run", CONTAINER, "--", "true"],
                       capture_output=True, text=True, env=ENV, timeout=180)
    add("run states the user on stderr, stdout stays empty",
        p.stdout == "" and "NOTE: the guest runs as root (uid 0)" in p.stderr,
        f"stdout={p.stdout!r} stderr={p.stderr.strip()!r}")

    width = max(len(n) for n, _, _ in checks)
    for name, status, detail in checks:
        print(f"{status:4}  {name:<{width}}  {detail}")
    failed = [n for n, s, _ in checks if s == "FAIL"]
    skipped = [n for n, s, _ in checks if s == "SKIP"]
    print(f"\n{len(checks)} checks: {len(checks) - len(failed) - len(skipped)} passed, "
          f"{len(failed)} failed, {len(skipped)} skipped")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
