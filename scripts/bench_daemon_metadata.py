#!/usr/bin/env python3
"""Daemon-routed metadata queries vs the in-process fallback (roadmap section 9.2, #59).

`list`, `info` and `ps` ask `podnext-daemon` while it runs and fall back to reading
the store in-process when it does not. This harness proves, with output rather than
by reading the code:

  * identical output on both paths -- a byte diff of every query, in JSON and table;
  * the fallback is silent -- no stderr the daemon-up path did not also produce;
  * latency with 1 and with 10 containers, p50 over the runs, for both paths;
  * the daemon round trips per query, which must not grow with the container count;
  * the negative control: a container created *after* the daemon started is answered
    correctly, not from a stale snapshot.

Everything resolves under one throwaway `$HOME`, so the store (`$HOME/.podnext`) and
the daemon socket (`$HOME/.local/run/pn-next/daemon.sock`) are both private to a run
and the real store is never touched.

Usage::

    python3 scripts/bench_daemon_metadata.py --binary-dir <dir with podnext + podnext-daemon>
"""

import argparse
import json
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path
from temp_base import temp_base  # one rule, one place (see that module)

# Each (argv, label) is run on both paths and diffed byte for byte.
QUERIES = [
    (["list", "-a", "--format", "json"], "list-json"),
    (["list", "-a", "--format", "table"], "list-table"),
    (["info", "--format", "json"], "info-json"),
    (["info", "--format", "table"], "info-table"),
    (["ps", "--format", "json"], "ps-json"),
    (["ps", "--format", "table"], "ps-table"),
]

# The env the child process must not inherit, or it would resolve the real home.
SCRUB = (
    "LD_LIBRARY_PATH",
    "RUNTIME_DIR",
    "TERMUX__HOME",
    "XDG_RUNTIME_DIR",
    "XDG_CONFIG_HOME",
    "XDG_STATE_HOME",
    "PN_TEST_ROOT",
    "PODNEXT_BENCH",
    "PODNEXT_NO_INDEX_CACHE",
)


def temp_base() -> Path:
    return temp_base()


def child_env(home: Path) -> dict:
    env = dict(os.environ)
    for key in SCRUB:
        env.pop(key, None)
    env["HOME"] = str(home)
    return env


def socket_path(home: Path) -> Path:
    return home / ".local/run/pn-next/daemon.sock"


def build_store(home: Path, names) -> None:
    store = home / ".podnext" / "containers"
    now = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    for name, image, tag in names:
        c = store / name
        (c / "rootfs" / "etc").mkdir(parents=True)
        (c / "rootfs" / "etc" / "os-release").write_text(f'ID={image}\nVERSION_ID="{tag}"\n')
        (c / "metadata.json").write_text(
            json.dumps(
                {
                    "name": name,
                    "image": image,
                    "tag": tag,
                    "rootfs": str(c / "rootfs"),
                    "created_at": now,
                    "status": "Stopped",
                }
            )
        )


def names(n: int):
    return [(f"c{i:02d}", f"distro{i}", f"1.{i}") for i in range(n)]


def run(binary: str, env: dict, args, bench: bool):
    e = dict(env)
    if bench:
        e["PODNEXT_BENCH"] = "1"
    t = time.perf_counter()
    p = subprocess.run([binary, *args], capture_output=True, text=True, env=e)
    wall = (time.perf_counter() - t) * 1000.0
    inproc = rt = None
    for line in p.stderr.splitlines():
        if line.startswith("PODNEXT_BENCH_MS="):
            inproc = float(line.split("=", 1)[1])
        elif line.startswith("PODNEXT_BENCH_RT="):
            rt = int(line.split("=", 1)[1])
    return wall, inproc, rt, p


def stats(samples):
    if not samples or any(s is None for s in samples):
        return None
    s = sorted(samples)
    return {"n": len(s), "p50": statistics.median(s), "min": s[0], "max": s[-1],
            "raw": [round(x, 3) for x in s]}


def start_daemon(daemon: str, home: Path, env: dict):
    log = open(home / "daemon.log", "wb")
    proc = subprocess.Popen([daemon, "--foreground"], env=env, stdout=log, stderr=log)
    sock = socket_path(home)
    for _ in range(100):
        if sock.exists():
            return proc
        if proc.poll() is not None:
            sys.exit(f"FAIL: daemon exited {proc.returncode}; see {home}/daemon.log")
        time.sleep(0.05)
    sys.exit(f"FAIL: daemon never bound {sock}")


def stop_daemon(proc, home: Path) -> None:
    proc.terminate()
    proc.wait(timeout=15)
    sock = socket_path(home)
    for _ in range(100):
        if not sock.exists():
            return
        time.sleep(0.05)
    sys.exit(f"FAIL: socket {sock} survived shutdown")


def measure(binary: str, env: dict, runs: int):
    walls, inprocs, rts = [], [], []
    for _ in range(runs):
        wall, inproc, rt, p = run(binary, env, ["list", "-a", "--format", "json"], True)
        if p.returncode != 0:
            sys.exit(f"FAIL: list exited {p.returncode}\n{p.stderr}")
        walls.append(wall)
        inprocs.append(inproc)
        rts.append(rt)
    return stats(walls), stats(inprocs), sorted(set(rts))


def capture(binary: str, env: dict):
    out = {}
    for args, name in QUERIES:
        _, _, _, p = run(binary, env, args, False)
        if p.returncode != 0:
            sys.exit(f"FAIL: {name} exited {p.returncode}\n{p.stderr}")
        out[name] = (p.stdout, p.stderr)
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--binary-dir", default="target/debug",
                    help="directory holding the `podnext` and `podnext-daemon` binaries")
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--containers", type=int, default=10)
    ap.add_argument("--keep", action="store_true")
    args = ap.parse_args()
    if args.runs < 5:
        sys.exit("FAIL: --runs must be >= 5 (acceptance criteria)")

    bindir = Path(args.binary_dir).resolve()
    binary = str(bindir / "podnext")
    daemon = str(bindir / "podnext-daemon")
    for b in (binary, daemon):
        if not Path(b).exists():
            sys.exit(f"FAIL: no binary at {b}")

    roots = []
    report = {"runs": args.runs, "containers": args.containers, "binaries": {"podnext": binary, "daemon": daemon},
              "sizes": {}}
    try:
        for n in (1, args.containers):
            home = Path(tempfile.mkdtemp(prefix=f"pdn-daemon-{n}-", dir=temp_base()))
            roots.append(home)
            build_store(home, names(n))
            env = child_env(home)

            # --- daemon path ---
            dp = start_daemon(daemon, home, env)
            run(binary, env, ["list", "-a", "--format", "json"], True)  # warm the index
            d_wall, d_inproc, d_rt = measure(binary, env, args.runs)
            d_cap = capture(binary, env)

            # --- fallback: the daemon is killed mid-session, the CLI must not notice ---
            stop_daemon(dp, home)
            run(binary, env, ["list", "-a", "--format", "json"], True)
            f_wall, f_inproc, f_rt = measure(binary, env, args.runs)
            f_cap = capture(binary, env)

            diffs = {}
            for _, label in QUERIES:
                d_out, d_err = d_cap[label]
                f_out, f_err = f_cap[label]
                diffs[label] = {"stdout_identical": d_out == f_out, "stderr_identical": d_err == f_err}
                if not args.keep and d_out != f_out:
                    # Keep the mismatching bytes so the parent can see the raw diff.
                    (home / f"{label}.daemon").write_text(d_out)
                    (home / f"{label}.fallback").write_text(f_out)

            report["sizes"][str(n)] = {
                "daemon": {"wall": d_wall, "inproc": d_inproc, "round_trips": d_rt,
                           "stderr_empty": all(e == "" for _, e in d_cap.values())},
                "fallback": {"wall": f_wall, "inproc": f_inproc, "round_trips": f_rt,
                             "stderr_empty": all(e == "" for _, e in f_cap.values())},
                "identical": all(v["stdout_identical"] and v["stderr_identical"] for v in diffs.values()),
                "per_query": diffs,
            }

        # --- negative control: a container created after the daemon started ---
        home = Path(tempfile.mkdtemp(prefix="pdn-daemon-late-", dir=temp_base()))
        roots.append(home)
        build_store(home, names(3))
        env = child_env(home)
        dp = start_daemon(daemon, home, env)
        _, _, _, before = run(binary, env, ["list", "-a", "--format", "json"], False)
        build_store(home, [("late", "debian", "12")])  # appears only now
        _, _, _, after = run(binary, env, ["list", "-a", "--format", "json"], False)
        sock_alive = socket_path(home).exists()
        stop_daemon(dp, home)
        before_names = [c["name"] for c in json.loads(before.stdout)]
        after_names = [c["name"] for c in json.loads(after.stdout)]
        report["negative_control"] = {
            "daemon_running_at_query": sock_alive,
            "before": before_names,
            "after": after_names,
            "late_visible": "late" in after_names,
            "ok": sock_alive and "late" not in before_names and "late" in after_names,
        }

        # --- report ---
        print(f"binaries      podnext={binary}")
        print(f"              daemon ={daemon}")
        print(f"runs          {args.runs}   (p50 over the runs)")
        print()
        ok = True
        for n in (1, args.containers):
            s = report["sizes"][str(n)]
            print(f"== {n} container(s) ==")
            for path in ("daemon", "fallback"):
                p = s[path]
                print(f"  {path:8}  in-process p50 {fmt(p['inproc'])}   "
                      f"wall p50 {p['wall']['p50']:.2f} min {p['wall']['min']:.2f} max {p['wall']['max']:.2f}   "
                      f"round trips {p['round_trips']}   stderr empty {p['stderr_empty']}")
            print(f"  identical output across both paths: {s['identical']}")
            for label, d in s["per_query"].items():
                if not (d["stdout_identical"] and d["stderr_identical"]):
                    ok = False
                    print(f"    MISMATCH {label}: {d}")
            if s["daemon"]["round_trips"] != [1] or s["fallback"]["round_trips"] != [0]:
                ok = False
                print("    round trips are not the expected [1] / [0]")
            print()
        nc = report["negative_control"]
        print("== negative control: container created after the daemon started ==")
        print(f"  daemon running at query: {nc['daemon_running_at_query']}")
        print(f"  before: {nc['before']}")
        print(f"  after : {nc['after']}")
        print(f"  late container visible (not stale): {nc['late_visible']}")
        print()
        print(f"RESULT {'PASS' if ok and nc['ok'] else 'FAIL'}")
        if not (ok and nc["ok"]):
            sys.exit(1)
        print("JSON " + json.dumps(report))
    finally:
        if not args.keep:
            for r in roots:
                shutil.rmtree(r, ignore_errors=True)


def fmt(s):
    return "-" if s is None else f"{s['p50']:.2f} (min {s['min']:.2f} max {s['max']:.2f})"


if __name__ == "__main__":
    main()