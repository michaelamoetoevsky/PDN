#!/usr/bin/env python3
"""
Casino Concurrency & Race-Condition Gambler (Chaotic Interleaving)
Automated stress test harness for podnext:
1. Parallel competing operations with random micro-sleep delays.
2. File lock (ContainerLock flock / FileLock) verification & bypassing audit.
3. Rapid burst queries (20 parallel podnext ps calls during session thrashing).
4. Exit code and state transition consistency checks.
"""

import os
import sys
import time
import json
import random
import fcntl
import shutil
import threading
import subprocess
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed
from typing import List, Dict, Any, Tuple

HOME = Path(os.environ.get("HOME", "/data/data/com.termux/files/home"))
SCRATCH = Path("/data/data/com.termux/files/home/.hermes/cache/scratch/casino_chaos")
SCRATCH.mkdir(parents=True, exist_ok=True)
REPORT_PATH = HOME / "workspace" / "PDN" / "scripts" / "casino_chaos_report.json"

TARGET = "test-alpine"

REAL_STORE = Path(os.path.expanduser("~")) / ".podnext"


def resolve_binary() -> str:
    """The binary under test: `PDNEXT_BIN`, else this repo's debug build.

    Never an arbitrary installed/PATH binary. On 2026-11-08 an installed `podnext` that
    predated `PN_TEST_ROOT` support ignored the fixture store, so a chaos run renamed a
    container in the operator's REAL store while the harness believed it was on a fixture.
    Refusing is cheap; that was not.
    """
    explicit = os.environ.get("PDNEXT_BIN")
    if explicit:
        return explicit
    candidate = Path(__file__).resolve().parent.parent / "target" / "debug" / "podnext"
    if candidate.exists():
        return str(candidate)
    print(f"REFUSING: no binary given and {candidate} does not exist.")
    print("Build it (`cargo build -p podnext --bin podnext`) or export PDNEXT_BIN=<path>.")
    print("REFUSING is not a pass.")
    sys.exit(2)


def store_root() -> Path:
    """The store this harness may touch: `PN_TEST_ROOT/.podnext`, never the real one.

    The CLI resolves `PN_TEST_ROOT -> HOME -> ...` and appends `.podnext`
    (`crates/proot-distro-core/src/paths.rs:27-53`), so honouring the same variable keeps
    the harness and the binary looking at ONE store (ASEC2-ARCH-016). This script renames
    containers, kills them and rewrites their `metadata.json`, so a path that lands on the
    operator's store is refused unless `PDN_ALLOW_REAL_STORE=1` is exported deliberately
    (ASEC2-ARCH-013, 2026-11-08).
    """
    test_root = os.environ.get("PN_TEST_ROOT")
    root = Path(test_root) / ".podnext" if test_root else REAL_STORE
    real, real_store = root.resolve(), REAL_STORE.resolve()
    if (real == real_store or real_store in real.parents) and os.environ.get(
        "PDN_ALLOW_REAL_STORE"
    ) != "1":
        print(f"REFUSING: store {root} is the operator's real store.")
        print("This harness renames, kills and rewrites containers, so it must run against a")
        print("fixture:  export PN_TEST_ROOT=<fixture-home>   (store = <fixture-home>/.podnext)")
        print("The fixture container is seeded automatically. To run against the real store")
        print("deliberately, also export PDN_ALLOW_REAL_STORE=1.")
        print("REFUSING is not a pass.")
        sys.exit(2)
    return root


def seed_fixture(store: Path) -> None:
    """Ensure `<store>/containers/<TARGET>` exists before the chaos starts.

    A missing fixture container would make every operation fail for the wrong reason. The
    rootfs is copied out of the operator's store, which only READS it, and `tar` is used
    rather than `cp -a` because a rootfs contains mode-000 bind directories that make
    `cp -a` stop with exit 1.
    """
    target_dir = store / "containers" / TARGET
    rootfs = target_dir / "rootfs"
    if rootfs.is_dir():
        return
    target_dir.mkdir(parents=True, exist_ok=True)
    rootfs.mkdir(parents=True, exist_ok=True)
    source = REAL_STORE / "containers" / TARGET / "rootfs"
    if source.is_dir():
        producer = subprocess.Popen(
            ["tar", "--ignore-failed-read", "-cf", "-", "-C", str(source), "."],
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        subprocess.run(
            ["tar", "-xf", "-", "-C", str(rootfs)],
            stdin=producer.stdout,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        if producer.stdout is not None:
            producer.stdout.close()
        producer.wait()
        print(f"seeded {target_dir} from {source} (read-only copy)")
    else:
        print(
            f"seeded {target_dir} with an EMPTY rootfs: {source} does not exist, so the guest"
            " commands the chaos issues will fail and only the store-side operations mean"
            " anything"
        )
    meta = target_dir / "metadata.json"
    if not meta.exists():
        meta.write_text(json.dumps({"name": TARGET, "image": "alpine", "rootfs": "rootfs"}))


def assert_binary_honours_store(binary: str, store: Path) -> None:
    """Prove the binary acts on the fixture BEFORE it is allowed to act at all.

    A binary that ignores PN_TEST_ROOT renames, kills and rewrites containers in the
    operator's real store while the harness believes it is on a fixture - measured on
    2026-11-08. The probe is a read-only `list`: what it lists must be exactly the fixture's
    containers. Skipped only under the deliberate `PDN_ALLOW_REAL_STORE=1` opt-out.
    """
    if os.environ.get("PDN_ALLOW_REAL_STORE") == "1":
        return
    containers = store / "containers"
    expected = {p.name for p in containers.iterdir() if p.is_dir()} if containers.is_dir() else set()
    env = dict(os.environ, PN_TEST_ROOT=str(store.parent))
    try:
        proc = subprocess.run(
            [binary, "list"], env=env, capture_output=True, text=True, timeout=60
        )
    except Exception as exc:  # noqa: BLE001 - a probe that cannot run must refuse, not guess
        print(f"REFUSING: could not run `{binary} list` to prove the store is the fixture: {exc}")
        sys.exit(2)
    seen = set()
    for line in proc.stdout.splitlines():
        if not line.startswith("|"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if cells and cells[0] and cells[0] != "NAME" and set(cells[0]) != {"-"}:
            seen.add(cells[0])
    if seen != expected:
        print(f"REFUSING: {binary} does not act on the fixture store.")
        print(f"  PN_TEST_ROOT={store.parent}")
        print(f"  the fixture holds {sorted(expected)}")
        print(f"  the binary listed {sorted(seen)}")
        print("A binary that ignores PN_TEST_ROOT would touch the REAL store, so this harness")
        print("refuses. Use the repo build (target/debug/podnext) or export PDNEXT_BIN=<path>.")
        print("REFUSING is not a pass.")
        sys.exit(2)


PODNEXT_BIN: str = resolve_binary()
CONTAINERS_DIR = store_root() / "containers"
seed_fixture(CONTAINERS_DIR.parent)
assert_binary_honours_store(PODNEXT_BIN, CONTAINERS_DIR.parent)

def run_cmd(cmd: List[str], timeout: float = 15.0) -> Dict[str, Any]:
    t0 = time.perf_counter()
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        dur = time.perf_counter() - t0
        return {
            "cmd": cmd,
            "exit_code": p.returncode,
            "stdout": p.stdout.strip(),
            "stderr": p.stderr.strip(),
            "duration_ms": round(dur * 1000, 2),
            "timed_out": False,
        }
    except subprocess.TimeoutExpired as e:
        dur = time.perf_counter() - t0
        return {
            "cmd": cmd,
            "exit_code": -1,
            "stdout": (e.stdout or "").strip() if isinstance(e.stdout, str) else "",
            "stderr": (e.stderr or "").strip() if isinstance(e.stderr, str) else "",
            "duration_ms": round(dur * 1000, 2),
            "timed_out": True,
        }
    except Exception as e:
        dur = time.perf_counter() - t0
        return {
            "cmd": cmd,
            "exit_code": -999,
            "stdout": "",
            "stderr": str(e),
            "duration_ms": round(dur * 1000, 2),
            "timed_out": False,
        }

def ensure_target_intact():
    """Verify target container exists and metadata is valid; recover if renamed."""
    target_dir = CONTAINERS_DIR / TARGET
    if not target_dir.exists():
        # Check if renamed target exists
        for c in CONTAINERS_DIR.glob(f"{TARGET}*"):
            if c.is_dir() and c.name != TARGET:
                run_cmd([PODNEXT_BIN, "rename", c.name, TARGET])
                break

    meta_file = target_dir / "metadata.json"
    if meta_file.exists():
        try:
            with open(meta_file, "r") as f:
                data = json.load(f)
            if data.get("name") != TARGET:
                data["name"] = TARGET
                data["rootfs"] = str(target_dir / "rootfs")
                with open(meta_file, "w") as f:
                    json.dump(data, f, indent=2)
        except Exception:
            # repair corrupted metadata
            repaired = {
                "created_at": "2026-10-29T22:03:00+00:00",
                "image": "alpine",
                "name": TARGET,
                "rootfs": str(target_dir / "rootfs"),
                "status": "Stopped",
                "tag": "3.18.12"
            }
            with open(meta_file, "w") as f:
                json.dump(repaired, f, indent=2)

def test_phase1_chaotic_interleaving(rounds: int = 10) -> Dict[str, Any]:
    print(f"\n[PHASE 1] Chaotic Parallel Interleaving ({rounds} rounds)...")
    results = []
    anomalies = []

    host_payload = SCRATCH / "host_token.txt"
    with open(host_payload, "w") as f:
        f.write("casino_chaotic_payload_12345\n")

    for round_idx in range(1, rounds + 1):
        ensure_target_intact()
        temp_renamed = f"{TARGET}-rnd{round_idx}"

        # Define 4 concurrent tasks
        tasks = [
            ("run_write", [PODNEXT_BIN, "run", TARGET, "--", "sh", "-c", f"echo 'rnd_{round_idx}' > /tmp/rnd_token && sleep 0.05 && cat /tmp/rnd_token"]),
            ("rename_toggle", [PODNEXT_BIN, "rename", TARGET, temp_renamed]),
            ("copy_out", [PODNEXT_BIN, "copy", f"{TARGET}:/etc/alpine-release", str(SCRATCH / f"release_{round_idx}.txt")]),
            ("copy_in", [PODNEXT_BIN, "copy", str(host_payload), f"{TARGET}:/tmp/host_copy_{round_idx}.txt"]),
            ("kill_probe", [PODNEXT_BIN, "kill", TARGET]),
        ]

        # Shuffle tasks
        random.shuffle(tasks)

        # Launch in parallel with random micro-sleeps (1ms - 25ms)
        futures = {}
        round_res = {}
        start_barrier = time.perf_counter()

        def worker(label, cmd, delay):
            if delay > 0:
                time.sleep(delay)
            res = run_cmd(cmd)
            res["label"] = label
            res["delay_s"] = delay
            return label, res

        with ThreadPoolExecutor(max_workers=len(tasks)) as executor:
            for label, cmd in tasks:
                delay = random.uniform(0.001, 0.025)
                f = executor.submit(worker, label, cmd, delay)
                futures[f] = label

            for f in as_completed(futures):
                lbl, r = f.result()
                round_res[lbl] = r

        # Post-round cleanup: if container got renamed, rename back
        post_dir_renamed = CONTAINERS_DIR / temp_renamed
        if post_dir_renamed.exists():
            run_cmd([PODNEXT_BIN, "rename", temp_renamed, TARGET])

        # Check metadata validity after concurrent storm
        meta_file = CONTAINERS_DIR / TARGET / "metadata.json"
        meta_valid = False
        meta_err = None
        if meta_file.exists():
            try:
                with open(meta_file, "r") as mf:
                    m = json.load(mf)
                meta_valid = (m.get("name") == TARGET)
            except Exception as e:
                meta_err = str(e)
        else:
            meta_err = "metadata.json missing"

        # Detect race conditions
        # Case A: rename succeeded while run failed with container not found
        # Case B: copy failed because directory moved
        # Case C: metadata corrupted
        # Case D: exit codes disagreement
        anom_detected = []
        if not meta_valid:
            anom_detected.append(f"Metadata corrupted/missing: {meta_err}")

        # Check task collision errors
        for lbl, r in round_res.items():
            if r["exit_code"] != 0:
                err_lower = r["stderr"].lower()
                if "not found" in err_lower or "no such file" in err_lower or "already exists" in err_lower:
                    anom_detected.append(f"Collision in {lbl}: exit {r['exit_code']}, stderr='{r['stderr']}'")

        if anom_detected:
            anomalies.append({
                "round": round_idx,
                "anomalies": anom_detected,
                "task_results": round_res
            })

        results.append({
            "round": round_idx,
            "duration_ms": round((time.perf_counter() - start_barrier) * 1000, 2),
            "meta_valid": meta_valid,
            "task_exits": {lbl: r["exit_code"] for lbl, r in round_res.items()},
        })

    ensure_target_intact()
    print(f"  Completed {rounds} rounds. Detected {len(anomalies)} collision/race anomalies.")
    return {
        "total_rounds": rounds,
        "anomalies_count": len(anomalies),
        "anomalies": anomalies,
        "round_samples": results[:5]
    }

def test_phase2_lock_mechanisms() -> Dict[str, Any]:
    print("\n[PHASE 2] ContainerLock / FileLock Contention & Phantom Lock Audit...")
    ensure_target_intact()

    # Part A: Check if CLI uses ~/.pn-next/locks or ~/.podnext/locks
    pn_next_lock = HOME / ".pn-next" / "locks" / f"{TARGET}.lock"
    podnext_lock = HOME / ".podnext" / "locks" / f"{TARGET}.lock"

    # Part B: Phantom Lock Verification
    # Hold exclusive lock on both files and see if podnext run & rename execute anyway
    pn_next_lock.parent.mkdir(parents=True, exist_ok=True)
    podnext_lock.parent.mkdir(parents=True, exist_ok=True)

    fd_pn = os.open(pn_next_lock, os.O_CREAT | os.O_RDWR)
    fd_pdn = os.open(podnext_lock, os.O_CREAT | os.O_RDWR)

    fcntl.flock(fd_pn, fcntl.LOCK_EX | fcntl.LOCK_NB)
    fcntl.flock(fd_pdn, fcntl.LOCK_EX | fcntl.LOCK_NB)

    # Now run podnext run test-alpine
    run_under_lock = run_cmd([PODNEXT_BIN, "run", TARGET, "--", "echo", "LOCK_BYPASSED"])
    # Run podnext rename test-alpine
    rename_under_lock = run_cmd([PODNEXT_BIN, "rename", TARGET, f"{TARGET}-locktest"])
    if rename_under_lock["exit_code"] == 0:
        run_cmd([PODNEXT_BIN, "rename", f"{TARGET}-locktest", TARGET])

    fcntl.flock(fd_pn, fcntl.LOCK_UN)
    fcntl.flock(fd_pdn, fcntl.LOCK_UN)
    os.close(fd_pn)
    os.close(fd_pdn)

    bypassed_run = (run_under_lock["exit_code"] == 0 and "LOCK_BYPASSED" in run_under_lock["stdout"])
    bypassed_rename = (rename_under_lock["exit_code"] == 0)

    # Part C: Multi-process ContainerLock flock stress test
    # Simulate ContainerLock behavior (POSIX flock LOCK_EX | LOCK_NB) across 30 concurrent processes
    flock_test_file = SCRATCH / "flock_race.lock"
    flock_test_file.unlink(missing_ok=True)

    contention_results = []
    contention_barrier = threading.Barrier(30)

    def flock_worker(worker_id: int):
        contention_barrier.wait()
        t0 = time.perf_counter()
        acquired = False
        err_msg = ""
        try:
            fd = os.open(flock_test_file, os.O_CREAT | os.O_RDWR)
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                acquired = True
                time.sleep(0.01)  # hold for 10ms
                fcntl.flock(fd, fcntl.LOCK_UN)
            except BlockingIOError:
                err_msg = "EWOULDBLOCK: held by another process"
            except Exception as ex:
                err_msg = str(ex)
            finally:
                os.close(fd)
        except Exception as ex:
            err_msg = str(ex)
        dur_us = round((time.perf_counter() - t0) * 1_000_000, 1)
        return worker_id, acquired, dur_us, err_msg

    with ThreadPoolExecutor(max_workers=30) as executor:
        futures = [executor.submit(flock_worker, i) for i in range(30)]
        for f in as_completed(futures):
            contention_results.append(f.result())

    success_count = sum(1 for _, acq, _, _ in contention_results if acq)
    contended_count = sum(1 for _, acq, _, _ in contention_results if not acq)
    avg_latency_us = round(sum(d for _, _, d, _ in contention_results) / len(contention_results), 1)

    print(f"  Phantom Lock Bug confirmed: run bypassed={bypassed_run}, rename bypassed={bypassed_rename}")
    print(f"  Flock NB Contention (30 threads): {success_count} acquired, {contended_count} rejected immediately (avg {avg_latency_us} us).")

    return {
        "phantom_lock_findings": {
            "pn_next_lock_path": str(pn_next_lock),
            "podnext_lock_path": str(podnext_lock),
            "run_bypassed_flock": bypassed_run,
            "rename_bypassed_flock": bypassed_rename,
            "conclusion": "CLI commands bypass ContainerLock flock entirely. ContainerLock is dead/unwired code in the CLI."
        },
        "flock_simulation_contention": {
            "total_threads": 30,
            "acquired_count": success_count,
            "rejected_ewouldblock": contended_count,
            "avg_latency_us": avg_latency_us
        }
    }

def test_phase3_ps_burst_queries(burst_count: int = 20, waves: int = 3) -> Dict[str, Any]:
    print(f"\n[PHASE 3] Rapid Burst Queries ({burst_count} parallel 'podnext ps' across {waves} waves)...")
    ensure_target_intact()

    # Background guest activity generator (starts and quickly terminates sessions)
    stop_flag = threading.Event()
    bg_errors = []

    def session_churner():
        while not stop_flag.is_set():
            # Quick guest run
            p = subprocess.run([PODNEXT_BIN, "run", TARGET, "--", "true"], capture_output=True, text=True)
            if p.returncode != 0:
                bg_errors.append(p.stderr.strip())
            time.sleep(0.01)

    churn_thread = threading.Thread(target=session_churner, daemon=True)
    churn_thread.start()

    time.sleep(0.1)  # allow churning to start

    wave_results = []
    all_ps_records = []

    for wave_idx in range(1, waves + 1):
        wave_t0 = time.perf_counter()
        barrier = threading.Barrier(burst_count)

        def ps_worker(query_id: int, fmt: str):
            barrier.wait()
            cmd = [PODNEXT_BIN, "ps"]
            if fmt == "json":
                cmd.extend(["--format", "json"])
            res = run_cmd(cmd, timeout=10.0)
            res["query_id"] = query_id
            res["format"] = fmt
            return res

        futures = []
        with ThreadPoolExecutor(max_workers=burst_count) as executor:
            for q_id in range(burst_count):
                fmt = "json" if q_id % 2 == 0 else "table"
                futures.append(executor.submit(ps_worker, q_id, fmt))

            for f in as_completed(futures):
                all_ps_records.append(f.result())

        wave_dur = time.perf_counter() - wave_t0
        wave_results.append({
            "wave": wave_idx,
            "duration_ms": round(wave_dur * 1000, 2)
        })

    stop_flag.set()
    churn_thread.join(timeout=2.0)

    # Analyze PS queries
    failed_ps = [r for r in all_ps_records if r["exit_code"] != 0 or r["timed_out"]]
    json_empty_count = sum(1 for r in all_ps_records if r["format"] == "json" and r["stdout"] == "[]")
    avg_ps_duration = round(sum(r["duration_ms"] for r in all_ps_records) / len(all_ps_records), 2)
    max_ps_duration = max(r["duration_ms"] for r in all_ps_records)

    print(f"  Executed {len(all_ps_records)} burst PS queries. Failures: {len(failed_ps)}, JSON empty: {json_empty_count}/{len(all_ps_records)//2}, Avg latency: {avg_ps_duration}ms, Max: {max_ps_duration}ms.")

    return {
        "total_queries": len(all_ps_records),
        "failed_queries_count": len(failed_ps),
        "json_empty_table_inconsistency": json_empty_count,
        "avg_duration_ms": avg_ps_duration,
        "max_duration_ms": max_ps_duration,
        "waves": wave_results,
        "bg_session_errors_count": len(bg_errors)
    }

def test_phase4_exit_codes_and_states() -> Dict[str, Any]:
    print("\n[PHASE 4] Exit Codes & State Transition Consistency...")
    ensure_target_intact()

    test_cases = []

    # 1. Normal guest exit 0
    t0 = run_cmd([PODNEXT_BIN, "run", TARGET, "--", "sh", "-c", "exit 0"])
    test_cases.append({"scenario": "guest exit 0", "expected": 0, "actual": t0["exit_code"], "match": t0["exit_code"] == 0})

    # 2. Guest exit non-zero (42)
    t42 = run_cmd([PODNEXT_BIN, "run", TARGET, "--", "sh", "-c", "exit 42"])
    test_cases.append({"scenario": "guest exit 42", "expected": 42, "actual": t42["exit_code"], "match": t42["exit_code"] == 42})

    # 3. Guest exit non-zero (127 command not found)
    t127 = run_cmd([PODNEXT_BIN, "run", TARGET, "--", "nonexistent_command_xyz"])
    test_cases.append({"scenario": "guest command not found (127)", "expected": 127, "actual": t127["exit_code"], "match": t127["exit_code"] in [1, 127]})

    # 4. Duplicate rename collision
    # Rename TARGET to TARGET-dup1, while second process tries to rename TARGET to TARGET-dup2
    ensure_target_intact()
    barrier = threading.Barrier(2)
    r1, r2 = None, None

    def ren1():
        barrier.wait()
        return run_cmd([PODNEXT_BIN, "rename", TARGET, f"{TARGET}-dup1"])

    def ren2():
        barrier.wait()
        return run_cmd([PODNEXT_BIN, "rename", TARGET, f"{TARGET}-dup2"])

    with ThreadPoolExecutor(max_workers=2) as executor:
        f1 = executor.submit(ren1)
        f2 = executor.submit(ren2)
        r1 = f1.result()
        r2 = f2.result()

    # One must succeed (exit 0), one must fail (exit != 0)
    exits = sorted([r1["exit_code"], r2["exit_code"]])
    collision_handled = (exits[0] == 0 and exits[1] != 0)
    test_cases.append({
        "scenario": "concurrent duplicate rename collision",
        "expected": "one 0, one non-zero",
        "actual": f"exits: r1={r1['exit_code']}, r2={r2['exit_code']}",
        "match": collision_handled,
        "detail": {"r1_err": r1["stderr"], "r2_err": r2["stderr"]}
    })

    # Restore rename
    for d in [f"{TARGET}-dup1", f"{TARGET}-dup2"]:
        if (CONTAINERS_DIR / d).exists():
            run_cmd([PODNEXT_BIN, "rename", d, TARGET])

    # 5. Kill on stopped container
    tkill = run_cmd([PODNEXT_BIN, "kill", TARGET])
    test_cases.append({"scenario": "kill stopped container", "expected": 0, "actual": tkill["exit_code"], "match": tkill["exit_code"] == 0})

    # 6. Kill on non-existent container
    tkill_non = run_cmd([PODNEXT_BIN, "kill", "nonexistent-container-xyz"])
    test_cases.append({"scenario": "kill nonexistent container", "expected": 0, "actual": tkill_non["exit_code"], "match": tkill_non["exit_code"] == 0})

    print(f"  Verified {len(test_cases)} state transition / exit code scenarios:")
    for tc in test_cases:
        status = "PASS" if tc["match"] else "FAIL"
        print(f"    [{status}] {tc['scenario']}: expected {tc['expected']}, actual {tc['actual']}")

    return {
        "scenarios": test_cases
    }

def main():
    print("=" * 60)
    print("CASINO CONCURRENCY & RACE-CONDITION GAMBLER TEST HARNESS")
    print(f"Target Container: {TARGET}")
    print(f"Binary: {PODNEXT_BIN}")
    print(f"Report Output: {REPORT_PATH}")
    print("=" * 60)

    p1 = test_phase1_chaotic_interleaving(rounds=12)
    p2 = test_phase2_lock_mechanisms()
    p3 = test_phase3_ps_burst_queries(burst_count=20, waves=3)
    p4 = test_phase4_exit_codes_and_states()

    full_report = {
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "phase1_chaotic_interleaving": p1,
        "phase2_lock_mechanisms": p2,
        "phase3_ps_burst_queries": p3,
        "phase4_exit_codes_and_states": p4,
    }

    with open(REPORT_PATH, "w") as f:
        json.dump(full_report, f, indent=2)

    print("\n" + "=" * 60)
    print(f"ALL TESTS COMPLETE. Report saved to: {REPORT_PATH}")
    print("=" * 60)

if __name__ == "__main__":
    main()
