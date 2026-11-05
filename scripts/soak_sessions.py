#!/usr/bin/env python3
"""Soak: repeated guest start/stop must leave no container process (PDN#58).

Starts a guest, runs a trivial command, lets it exit -- `SOAK_ITERS` times
(default 20). After every iteration it counts the processes that belong to the
container and fails on the FIRST leak, printing which iteration leaked. The
final count must equal the count measured before the loop.

A process "belongs to the container" if its cmdline names the container's
`.podnext/containers/<name>/` tree (the proot leader) or if it is a descendant
of such a process (the guest's own children, e.g. busybox `sh` -> `true`).

Negative control (`--inject-leak N`): at iteration N the guest is started in the
background instead of waited on, so a guest process is deliberately left
running. The check must then report a leak at iteration N and exit non-zero.
(In-guest `&` alone does NOT leak: proot runs `--kill-on-exit`, which reaps the
whole traced tree when the leader exits -- the control therefore leaves the
leader alive, which is the leak that actually matters.)

CI: needs an installed container. With none it prints `SKIP: ...` and exits 0 --
a skip is NOT a pass, it is a skip (exit 0 only so CI stays green). A real leak
exits 1.

Env: PDNEXT_BIN, PDN_CONTAINER, PDN_ROOTFS, SOAK_ITERS, SOAK_LEAK_AT.
Timings go to stdout, never into the pass/fail decision.
Temp files, when used, go under $PREFIX/tmp.
"""
import glob
import os
import shutil
import signal
import subprocess
import sys
import time
from typing import NoReturn

ENV = {k: v for k, v in os.environ.items() if k != "LD_LIBRARY_PATH"}


def find_bin():
    b = os.environ.get("PDNEXT_BIN")
    if b and os.path.exists(b):
        return b
    return shutil.which("podnext")


def list_containers(binary):
    try:
        out = subprocess.run(
            [binary, "list"], env=ENV, capture_output=True, text=True, timeout=60
        ).stdout
    except Exception:
        return []
    names = []
    for line in out.splitlines():
        if not line.startswith("|"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) >= 2 and cells[0] and cells[0] != "NAME" and set(cells[0]) != {"-"}:
            names.append(cells[0])
    return names


def find_container(binary):
    explicit = os.environ.get("PDN_CONTAINER")
    if explicit:
        return explicit
    names = list_containers(binary)
    if not names:
        return None
    return "alpine" if "alpine" in names else names[0]


def container_rootfs(name):
    explicit = os.environ.get("PDN_ROOTFS")
    if explicit:
        return explicit
    return os.path.expanduser(f"~/.podnext/containers/{name}/rootfs")


def skip(reason) -> NoReturn:
    print(f"SKIP: {reason}")
    print("SKIP is not a pass.")
    sys.exit(0)


def read_procs():
    procs, cmd, cwd = {}, {}, {}
    for p in glob.glob("/proc/[0-9]*"):
        pid = int(p.rsplit("/", 1)[1])
        try:
            with open(p + "/stat") as f:
                procs[pid] = int(f.read().rsplit(")", 1)[1].split()[1])
        except Exception:
            continue
        try:
            with open(p + "/cmdline", "rb") as f:
                cmd[pid] = f.read().replace(b"\0", b" ").decode("utf8", "replace")
        except Exception:
            cmd[pid] = ""
        try:
            cwd[pid] = os.readlink(p + "/cwd")
        except Exception:
            cwd[pid] = ""
    return procs, cmd, cwd


def container_procs(rootfs):
    """PIDs that belong to the container: proot leaders + their descendants."""
    procs, cmd, cwd = read_procs()
    leaders = {pid for pid in procs if rootfs in cmd.get(pid, "") or cwd.get(pid) == rootfs}
    kids = {}
    for pid, ppid in procs.items():
        kids.setdefault(ppid, []).append(pid)
    seen, stack = set(), list(leaders)
    while stack:
        x = stack.pop()
        if x in seen:
            continue
        seen.add(x)
        stack.extend(kids.get(x, []))
    return seen


def describe(pids):
    _, cmd, _ = read_procs()
    return [(pid, cmd.get(pid, "")[:120]) for pid in sorted(pids)]


def kill_container_procs(rootfs):
    pids = container_procs(rootfs)
    for pid in pids:
        try:
            os.kill(pid, signal.SIGKILL)
        except OSError:
            pass
    end = time.time() + 5
    while time.time() < end and container_procs(rootfs):
        time.sleep(0.2)
    return container_procs(rootfs)


def main():
    inject = None
    argv = sys.argv[1:]
    if "--inject-leak" in argv:
        inject = int(argv[argv.index("--inject-leak") + 1])
    if inject is None and os.environ.get("SOAK_LEAK_AT"):
        inject = int(os.environ["SOAK_LEAK_AT"])
    iters = int(os.environ.get("SOAK_ITERS", "20"))

    binary = find_bin()
    if not binary:
        skip("podnext binary not found (set PDNEXT_BIN or install it on PATH)")
    container = find_container(binary)
    if not container:
        skip("no container installed (`podnext list` returned nothing)")
    rootfs = container_rootfs(container)
    if not os.path.isdir(rootfs):
        skip(f"container rootfs not found at {rootfs}")

    base = container_procs(rootfs)
    print(f"binary       : {binary}")
    print(f"container    : {container}")
    print(f"rootfs       : {rootfs}")
    print(f"iterations   : {iters}")
    print(f"inject-leak  : {inject if inject else 'no'}")
    print(f"baseline     : {len(base)} container process(es)")
    print()

    if base:
        print(f"FAIL: {len(base)} container process(es) already running before the loop; "
              f"refusing to soak a dirty starting state")
        for pid, c in describe(base):
            print(f"    {pid} :: {c}")
        sys.exit(1)

    leaked_at = None
    for i in range(1, iters + 1):
        if inject and i == inject:
            print(f"iteration {i}: [inject-leak] backgrounding the guest (not waited on)")
            subprocess.Popen(
                [binary, "login", container, "--", "sh", "-c", "sleep 120"],
                env=ENV, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
            time.sleep(1.5)  # let the leader come up
        else:
            subprocess.run(
                [binary, "login", container, "--", "sh", "-c", "true"],
                env=ENV, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                timeout=180,
            )
        pids = container_procs(rootfs)
        print(f"iteration {i}: container_procs={len(pids)}")
        if len(pids) > len(base):
            leaked_at = i
            print(f"LEAK at iteration {i}: {len(pids)} process(es) belong to the container "
                  f"(baseline {len(base)})")
            for pid, c in describe(pids):
                print(f"    {pid} :: {c}")
            break

    print()
    if leaked_at is not None:
        print(f"cleaning up the leaked container process(es) from iteration {leaked_at} ...")
        left = kill_container_procs(rootfs)
        print(f"after cleanup: container_procs={len(left)}")
        if inject and leaked_at == inject:
            print(f"NEGATIVE CONTROL OK: injected leak was caught at iteration {inject} "
                  f"and the check exited non-zero")
        sys.exit(1)

    final = container_procs(rootfs)
    print(f"final count          : {len(final)}")
    print(f"count before the loop: {len(base)}")
    if len(final) != len(base):
        print(f"FAIL: final count {len(final)} != baseline {len(base)}")
        for pid, c in describe(final):
            print(f"    {pid} :: {c}")
        sys.exit(1)
    print("PASS: no container process left behind.")


if __name__ == "__main__":
    main()
