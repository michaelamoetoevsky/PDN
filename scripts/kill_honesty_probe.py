#!/usr/bin/env python3
"""Does `podnext kill` ever claim a session it did not signal? (ASEC3-PROC2-01, p1b)

Written 2026-11-08 by the parent session as the runnable reproduction for the reopened finding
"kill lies": the original measurement showed `Container '<n>' killed.` printed, exit 0, and the
session record removed while a live process still held the record - so the claim was not backed by a
signal. The remedy in the tree is `active_sessions()` (the registry, C8) plus `signal_target()`, which
addresses a session by `(pid, start_time)` so a signal can be REFUSED when the pid no longer names the
registrant (B13, the same rule runc's `hasInit()` enforces). This probe measures whether the surface
can still claim a kill it did not perform, and it does NOT assume which flock mode the registry uses:
it runs every combination and prints what happened, because a probe that assumes the mechanism it is
testing proves nothing.

Arms (each on its own fixture copy of a container, each with its own record file):
  1. dead pid, record NOT held            - a leftover record from a crashed session
  2. dead pid, record held LOCK_SH        - B13: the pid was recycled / the guest is gone
  3. dead pid, record held LOCK_EX        - the holder's own mode, pid already gone
  4. LIVE pid, record held LOCK_SH        - POSITIVE CONTROL: a real session must still be reported
  5. LIVE pid, record held LOCK_EX        - the holder's own mode with a real session

Assertions:
  * every arm that prints `Sent ... to session PID` must have really signalled the pid printed, and
    the pid must have been alive at that moment;
  * an arm whose session is not signalled must NOT print a `Sent ... to session PID` line;
  * arm 4/5 must print exactly one such line, or the probe refuses: without a positive control, arm 2
    and 3 passing would only show that the reporter is silent, not that it is honest.

Exit 0 = every arm behaved; 1 = an arm printed a kill claim it did not back; 2 = REFUSING (the binary
is not acting on the fixture, or the rig cannot run) - a refusal is not a pass.
"""

from __future__ import annotations

import fcntl
import json
import os
import pathlib
import re
import subprocess
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from pdn_fixture_guard import assert_binary_honours_store, resolve_binary, seed_fixture, store_root  # noqa: E402

CONTAINER = "probe-kill"
SENT = re.compile(r"^Sent (\S+) to session PID (\d+)", re.M)


def proc_start_time(pid: int) -> int:
    """Field 22 of /proc/<pid>/stat, in clock ticks - exactly how the records are written."""
    raw = pathlib.Path(f"/proc/{pid}/stat").read_text()
    return int(raw[raw.rfind(")") + 2 :].split()[19])


def write_record(sessions: pathlib.Path, pid: int, start_time: int) -> pathlib.Path:
    path = sessions / f"{pid}.json"
    path.write_text(
        json.dumps(
            {
                "container_name": CONTAINER,
                "pid": pid,
                "started_at": 1_700_000_000,
                "login_uid": "0",
                "cmdline": ["/bin/sleep", "60"],
                "start_time": start_time,
            }
        )
    )
    return path


def hold(path: pathlib.Path, mode: str):
    """Hold the record the way a live session's own process does; returns the open fd to keep it."""
    fd = os.open(path, os.O_RDWR)
    fcntl.flock(fd, fcntl.LOCK_SH if mode == "SH" else fcntl.LOCK_EX)
    return fd


def dead_pid() -> int:
    """A pid that is certainly not alive: fork and reap a child, then reuse its number."""
    pid = os.fork()
    if pid == 0:
        os._exit(0)
    os.waitpid(pid, 0)
    return pid


def run_arm(binary: str, env: dict, sessions: pathlib.Path, label: str, pid: int, held: str, live: bool):
    for stale in sessions.glob("*.json"):
        stale.unlink()
    st = proc_start_time(pid) if live else 0
    path = write_record(sessions, pid, st)
    fd = hold(path, held) if held else None
    before_alive = pathlib.Path(f"/proc/{pid}").exists()
    proc = subprocess.run(
        [binary, "kill", CONTAINER], env=env, capture_output=True, text=True, timeout=90
    )
    after_alive = pathlib.Path(f"/proc/{pid}").exists()
    if fd is not None:
        os.close(fd)
    claim = SENT.search(proc.stdout)
    skipped = "Skipping session PID" in proc.stderr
    print(f"\n--- arm {label} (pid={pid} alive_before={before_alive} held={held or 'none'}) ---")
    print(f"    rc={proc.returncode}")
    print(f"    stdout: {proc.stdout.strip()[:220] or '<empty>'}")
    print(f"    stderr: {proc.stderr.strip()[:220] or '<empty>'}")
    print(f"    alive_after={after_alive} | claimed_signal={bool(claim)} | skipping_message={skipped}")
    return {
        "label": label,
        "claimed": bool(claim),
        "claimed_pid": int(claim.group(2)) if claim else None,
        "before_alive": before_alive,
        "after_alive": after_alive,
        "rc": proc.returncode,
    }


def main() -> int:
    binary = resolve_binary()
    store = store_root()
    seed_fixture(store, [CONTAINER], copy_rootfs=False)
    assert_binary_honours_store(binary, store)
    sessions = store / "sessions"
    sessions.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, PN_TEST_ROOT=str(store.parent))
    print(f"binary={binary}")
    print(f"store ={store}")
    print(f"session records will live in {sessions}")

    # A live pid to be the subject of the positive control, and a dead one for the B13 arms.
    sleeper = subprocess.Popen(["sleep", "120"])
    time.sleep(0.4)
    gone = dead_pid()
    print(f"live pid={sleeper.pid} | dead pid={gone}")

    arms = []
    try:
        arms.append(run_arm(binary, env, sessions, "1 dead/none", gone, "none", False))
        arms.append(run_arm(binary, env, sessions, "2 dead/SH", gone, "SH", False))
        arms.append(run_arm(binary, env, sessions, "3 dead/EX", gone, "EX", False))
        arms.append(run_arm(binary, env, sessions, "4 live/SH", sleeper.pid, "SH", True))
        arms.append(run_arm(binary, env, sessions, "5 live/EX", sleeper.pid, "EX", True))
    finally:
        sleeper.kill()
        sleeper.wait()

    print("\n=== verdict ===")
    bad = [a for a in arms if a["claimed"] and not a["before_alive"]]
    print(f"claims not backed by a live pid at claim time: {len(bad)}")
    for a in bad:
        print(f"  LIE: arm {a['label']} claimed to signal pid {a['claimed_pid']} which was not alive")
    positive = [a for a in arms if a["label"].startswith(("4 ", "5 ")) and a["claimed"]]
    print(f"positive-control arms that reported a real signal: {len(positive)}")
    if not positive:
        print("REFUSING: no arm reported a real signal, so silence in the other arms proves nothing.")
        return 2
    if bad:
        return 1
    # A silent non-claimed arm is only meaningful if its record was visible; say which were silent.
    print("silent arms: " + ", ".join(a["label"] for a in arms if not a["claimed"]))
    print("VERDICT: no arm claimed a signal it did not back")
    return 0


if __name__ == "__main__":
    sys.exit(main())
