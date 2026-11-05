#!/usr/bin/env python3
"""Benchmark the container index cache (roadmap section 9.2).

Builds a synthetic store of ten containers in a temp root and measures the
metadata query of ``podnext list -a``. Two numbers per mode:

  in-process  what the command itself spends building the manager and listing it,
              printed by the binary under ``PODNEXT_BENCH``. Excludes process
              start, so it is the latency the cache actually targets.
  wall        the whole subprocess, dominated by process start on this device.

Modes:

  cold   ``PODNEXT_NO_INDEX_CACHE=1`` -- the pre-cache algorithm (full scan of
         every metadata.json + os-release). This is the negative control.
  warm   cache present -- the index fast path.
  start  ``podnext --version`` -- process start only, for context.

A binary without the ``PODNEXT_BENCH`` hook still runs; only the in-process
column is blank.

Usage::

    python3 scripts/bench_container_index.py --binary target/debug/podnext
    python3 scripts/bench_container_index.py --assert-inproc-ms 10
    python3 scripts/bench_container_index.py --force-uncached --assert-inproc-ms 10
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

# The last container has no os-release, so its distro/tag are read straight from
# metadata.json: that is the value the invalidation proof changes.
SPECIAL = "scratch-meta"


def make_names(n: int):
    names = [(f"c{i:02d}", f"distro{i}", f"1.{i}") for i in range(n - 1)]
    names.append((SPECIAL, "scratch", "1.0"))
    return names


def temp_base() -> Path:
    return temp_base()


def build_store(root: Path, names) -> None:
    """One directory per container, shaped like ``save_container_metadata`` output."""
    store = root / ".podnext" / "containers"
    now = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    for name, image, tag in names:
        c = store / name
        (c / "rootfs" / "etc").mkdir(parents=True)
        if name != SPECIAL:
            (c / "rootfs" / "etc" / "os-release").write_text(
                f'ID={image}\nVERSION_ID="{tag}"\n'
            )
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


def run(binary, root, extra_env, args):
    env = dict(os.environ)
    env.pop("LD_LIBRARY_PATH", None)
    env["PN_TEST_ROOT"] = str(root)
    env.update(extra_env)
    t = time.perf_counter()
    p = subprocess.run([binary, *args], capture_output=True, text=True, env=env)
    wall = (time.perf_counter() - t) * 1000.0
    inproc = None
    for line in p.stderr.splitlines():
        if line.startswith("PODNEXT_BENCH_MS="):
            inproc = float(line.split("=", 1)[1])
    return wall, inproc, p


def stats(samples):
    if not samples:
        return None
    s = sorted(samples)
    return {
        "n": len(s),
        "p50": statistics.median(s),
        "min": s[0],
        "max": s[-1],
        "raw": [round(x, 3) for x in s],
    }


def measure(binary, root, runs, env):
    walls, inprocs = [], []
    for _ in range(runs):
        wall, inproc, p = run(binary, root, env, ["list", "-a", "--format", "json"])
        if p.returncode != 0:
            sys.exit(f"FAIL: list exited {p.returncode}\n{p.stderr}")
        walls.append(wall)
        if inproc is not None:
            inprocs.append(inproc)
    return stats(walls), stats(inprocs)


def list_json(binary, root, env):
    _, _, p = run(binary, root, env, ["list", "-a", "--format", "json"])
    return json.loads(p.stdout)


def invalidate_proof(binary, root, cache):
    """Cached query, rewrite one container's metadata.json, cached query again.

    ``scratch-meta`` has no os-release, so its distro/version are read straight
    from metadata.json: a change there is the value the cache must stop serving.
    """
    meta = root / ".podnext" / "containers" / "scratch-meta" / "metadata.json"
    before_all = list_json(binary, root, {})
    before = next(c for c in before_all if c["name"] == "scratch-meta")
    cache_before = cache.stat().st_mtime if cache.exists() else None

    meta.write_text(
        json.dumps(
            {
                "name": "scratch-meta",
                "image": "renamed-image",
                "tag": "9.9.9",
                "created_at": "2020-01-02T03:04:05Z",
                "status": "Stopped",
            }
        )
    )
    after_all = list_json(binary, root, {})
    after = next(c for c in after_all if c["name"] == "scratch-meta")
    cache_after = cache.stat().st_mtime if cache.exists() else None
    return before, after, cache_before, cache_after, cache


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    default_bin = os.environ.get("PODNEXT") or "target/debug/podnext"
    ap.add_argument("--binary", default=default_bin)
    ap.add_argument("--runs", type=int, default=7)
    ap.add_argument("--containers", type=int, default=10)
    ap.add_argument("--assert-inproc-ms", type=float, default=None)
    ap.add_argument(
        "--force-uncached",
        action="store_true",
        help="set PODNEXT_NO_INDEX_CACHE=1 for the warm runs too (negative control)",
    )
    ap.add_argument("--keep", action="store_true", help="keep the temp store")
    args = ap.parse_args()
    if args.runs < 5:
        sys.exit("FAIL: --runs must be >= 5 (acceptance criteria)")
    if args.containers < 2:
        sys.exit("FAIL: --containers must be >= 2")

    binary = str(Path(args.binary).resolve())
    root = Path(tempfile.mkdtemp(prefix="pdn-bench-", dir=temp_base()))
    cache = root / ".podnext" / "cache" / "container_index.json"
    try:
        names = make_names(args.containers)
        build_store(root, names)
        n = len(names)

        start, _ = measure(binary, root, 5, {})
        cold_wall, cold_inproc = measure(
            binary,
            root,
            args.runs,
            {"PODNEXT_NO_INDEX_CACHE": "1", "PODNEXT_BENCH": "1"},
        )

        warm_env = {"PODNEXT_BENCH": "1"}
        if args.force_uncached:
            warm_env["PODNEXT_NO_INDEX_CACHE"] = "1"
        # Warm the cache with one unmeasured run, then measure.
        run(binary, root, {"PODNEXT_BENCH": "1"}, ["list", "-a", "--format", "json"])
        warm_wall, warm_inproc = measure(binary, root, args.runs, warm_env)

        # The cold in-process runs never wrote a cache; make sure one exists before
        # the proof (a run without NO_INDEX_CACHE).
        if not args.force_uncached:
            run(binary, root, {"PODNEXT_BENCH": "1"}, ["list", "-a", "--format", "json"])
        before, after, cb, ca, _ = invalidate_proof(binary, root, cache)

        served = list_json(binary, root, {})
        print(f"binary        {binary}")
        print(f"store         {root}  ({n} containers, listed {len(served)})")
        print(f"runs          {args.runs}")
        print()
        print(f"process start      p50 {start['p50']:.2f}  min {start['min']:.2f}  max {start['max']:.2f}   {start['raw']}")
        print(f"cold in-process    p50 {fmt(cold_inproc)}   raw {raw(cold_inproc)}")
        print(f"cold wall          p50 {cold_wall['p50']:.2f}  min {cold_wall['min']:.2f}  max {cold_wall['max']:.2f}   {cold_wall['raw']}")
        print(f"warm in-process    p50 {fmt(warm_inproc)}   raw {raw(warm_inproc)}")
        print(f"warm wall          p50 {warm_wall['p50']:.2f}  min {warm_wall['min']:.2f}  max {warm_wall['max']:.2f}   {warm_wall['raw']}")
        if cold_inproc is None or warm_inproc is None:
            print("\nnote: binary has no PODNEXT_BENCH hook; in-process latency unavailable")
        print()
        print("invalidation proof (scratch-meta, distro/version read from metadata.json):")
        print(f"  before  distro={before['distro']} version={before['version']} created_at={before['created_at']}")
        print(f"  after   distro={after['distro']} version={after['version']} created_at={after['created_at']}")
        print(f"  cache file mtime before={cb} after={ca}  ({'rewritten' if cb != ca else 'unchanged'})")
        ok = before["distro"] != after["distro"] and after["distro"] == "renamed-image"
        print(f"  -> changed value returned: {ok}")

        result = {
            "start": start,
            "cold_inproc": cold_inproc,
            "cold_wall": cold_wall,
            "warm_inproc": warm_inproc,
            "warm_wall": warm_wall,
            "invalidation": {"before": before, "after": after, "ok": ok},
        }
        if args.assert_inproc_ms is not None:
            got = warm_inproc["p50"] if warm_inproc else None
            if got is None:
                sys.exit("FAIL: --assert-inproc-ms needs a binary with the PODNEXT_BENCH hook")
            if got >= args.assert_inproc_ms:
                print(f"\nFAIL: warm in-process p50 {got:.2f} ms >= {args.assert_inproc_ms} ms")
                sys.exit(1)
            print(f"\nPASS: warm in-process p50 {got:.2f} ms < {args.assert_inproc_ms} ms")
        print("\nJSON " + json.dumps(result))
    finally:
        if not args.keep:
            shutil.rmtree(root, ignore_errors=True)


def fmt(s):
    return "-" if s is None else f"{s['p50']:.2f}"


def raw(s):
    return "[]" if s is None else str(s["raw"])


if __name__ == "__main__":
    main()