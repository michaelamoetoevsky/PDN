"""Fixture guard shared by the PDN stress harnesses.

Every harness in `scripts/` renames containers, kills them, rewrites their `metadata.json`
or syncs over them. They were written against the operator's real store
(`~/.podnext/containers`) with fixed target names, which is what finding
`ASEC2-ARCH-013`/`014`/`015` recorded and what `ASEC2-ARCH-016` explains: the CLI resolves
`PN_TEST_ROOT -> HOME -> ...` and appends `.podnext`
(`crates/proot-distro-core/src/paths.rs:27-53`), while the scripts derived their own paths
from `HOME` - so a `PN_TEST_ROOT`-only redirect made the CLI act on a fixture while the
script acted on the real store.

Three guards, all of which refuse rather than guess:

1. `store_root()` - the harness's own paths come from `PN_TEST_ROOT/.podnext`, and a path
   that lands on the real store is refused unless `PDN_ALLOW_REAL_STORE=1` is set.
2. `resolve_binary()` - the binary under test is `PDNEXT_BIN` or this repo's debug build,
   never an arbitrary installed binary. An installed `podnext` predating `PN_TEST_ROOT`
   support ignores the fixture (measured 2026-11-08), and a harness that invokes it reaches
   the real store through the back door.
3. `assert_binary_honours_store()` - a read-only `list` must return exactly the fixture's
   containers before the harness is allowed to do anything. This is the check that would
   have caught the 2026-11-08 incident, in which a chaos run renamed a container in the
   operator's real store while the harness believed it was on a fixture.

Every refusal exits 2 and prints "REFUSING is not a pass." so a skipped guard is never
mistaken for a passing harness.
"""

import json
import os
import subprocess
import sys
from pathlib import Path

REAL_STORE = Path(os.path.expanduser("~")) / ".podnext"


def _refuse(lines):
    """Print a refusal and exit non-zero - a guard that cannot run is not a pass."""
    for line in lines:
        print(line)
    print("REFUSING is not a pass.")
    sys.exit(2)


def resolve_binary() -> str:
    """The binary under test: `PDNEXT_BIN`, else this repo's debug build."""
    explicit = os.environ.get("PDNEXT_BIN")
    if explicit:
        return explicit
    candidate = Path(__file__).resolve().parent.parent / "target" / "debug" / "podnext"
    if candidate.exists():
        return str(candidate)
    _refuse(
        [
            f"REFUSING: no binary given and {candidate} does not exist.",
            "Build it (`cargo build -p podnext --bin podnext`) or export PDNEXT_BIN=<path>.",
        ]
    )
    raise AssertionError("unreachable")


def store_root() -> Path:
    """`PN_TEST_ROOT/.podnext`, or the real store only under the explicit opt-out."""
    test_root = os.environ.get("PN_TEST_ROOT")
    root = Path(test_root) / ".podnext" if test_root else REAL_STORE
    real, real_store = root.resolve(), REAL_STORE.resolve()
    if (real == real_store or real_store in real.parents) and os.environ.get(
        "PDN_ALLOW_REAL_STORE"
    ) != "1":
        _refuse(
            [
                f"REFUSING: store {root} is the operator's real store.",
                "These harnesses rename, kill and rewrite containers, so they must run against",
                "a fixture:  export PN_TEST_ROOT=<fixture-home>   (store = <fixture-home>/.podnext)",
                "The fixture containers are seeded automatically. To run against the real store",
                "deliberately, also export PDN_ALLOW_REAL_STORE=1.",
            ]
        )
    return root


def seed_fixture(store: Path, names, *, copy_rootfs: bool = True) -> None:
    """Make `<store>/containers/<name>` exist for every name before the chaos starts.

    A missing fixture container makes every operation fail for the wrong reason. With
    `copy_rootfs` the rootfs is copied out of the operator's store, which only READS it -
    `tar` rather than `cp -a`, because a rootfs holds mode-000 bind directories that make
    `cp -a` stop with exit 1. Without it an empty rootfs is seeded and said out loud: the
    store-side operations (rename, reset, sync, lock races) are then meaningful and guest
    commands are expected to fail.
    """
    for name in names:
        target_dir = store / "containers" / name
        rootfs = target_dir / "rootfs"
        if rootfs.is_dir():
            continue
        target_dir.mkdir(parents=True, exist_ok=True)
        rootfs.mkdir(parents=True, exist_ok=True)
        source = REAL_STORE / "containers" / name / "rootfs"
        if copy_rootfs and source.is_dir():
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
                f"seeded {target_dir} with an EMPTY rootfs"
                + (
                    f": {source} does not exist"
                    if copy_rootfs
                    else " (this harness only needs the store-side operations)"
                )
                + ", so guest commands are expected to fail"
            )
        meta = target_dir / "metadata.json"
        if not meta.exists():
            meta.write_text(json.dumps({"name": name, "image": "alpine", "rootfs": "rootfs"}))


def list_containers(binary: str, env) -> set:
    """Container names from the CLI's own table, read-only."""
    proc = subprocess.run(
        [binary, "list"], env=env, capture_output=True, text=True, timeout=60
    )
    names = set()
    for line in proc.stdout.splitlines():
        if not line.startswith("|"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if cells and cells[0] and cells[0] != "NAME" and set(cells[0]) != {"-"}:
            names.add(cells[0])
    return names


def assert_binary_honours_store(binary: str, store: Path) -> None:
    """Prove the binary acts on the fixture BEFORE it is allowed to act at all.

    Read-only `list`: what it returns must be exactly the fixture's containers. A binary
    that ignores `PN_TEST_ROOT` would otherwise rename, kill and rewrite containers in the
    operator's real store while the harness believes it is on a fixture - measured
    2026-11-08. Skipped only under the deliberate `PDN_ALLOW_REAL_STORE=1` opt-out.
    """
    if os.environ.get("PDN_ALLOW_REAL_STORE") == "1":
        return
    containers = store / "containers"
    expected = (
        {p.name for p in containers.iterdir() if p.is_dir()} if containers.is_dir() else set()
    )
    env = dict(os.environ, PN_TEST_ROOT=str(store.parent))
    try:
        seen = list_containers(binary, env)
    except Exception as exc:  # noqa: BLE001 - a probe that cannot run must refuse, not guess
        _refuse(
            [
                f"REFUSING: could not run `{binary} list` to prove the store is the fixture:",
                f"  {exc}",
            ]
        )
        raise AssertionError("unreachable")
    if seen != expected:
        _refuse(
            [
                f"REFUSING: {binary} does not act on the fixture store.",
                f"  PN_TEST_ROOT={store.parent}",
                f"  the fixture holds {sorted(expected)}",
                f"  the binary listed {sorted(seen)}",
                "A binary that ignores PN_TEST_ROOT would touch the REAL store, so this harness",
                "refuses. Use the repo build (target/debug/podnext) or export PDNEXT_BIN=<path>.",
            ]
        )
