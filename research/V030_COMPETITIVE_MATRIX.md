# PDN v0.3.0 - competitive matrix against Termux's proot-distro

Four axes, because the goal was stated as four axes. Every number below says HOW it was measured; where a number
does not exist yet it is marked MEASURE and given the command that would produce it. Prose verdicts come from
`research/V030_COMPONENT_CONFIRMATION.md` and its three evidence files; nothing here is inferred from them that they
do not say.

## 1. Mobility - the axis that decides the comparison

| what | reference (proot-distro v5.10.3) | PDN today | measured how |
|---|---|---|---|
| login/run option surface | 11 flags (`--bind`, `--shared-tmp/home/termux-home`, `--work-dir`, `--hostname`, `--redirect-ports`, `--no-sysvipc/--no-link2symlink`, `--detach`, `--env`, `--get-proot-cmd`, `--user`) - `parser.py:116-158` | 3 accepted things (`user` refused, `--isolated`, a trailing command) - `cli.rs:353` | source, both trees |
| install from a registry reference | yes, with alias derivation | yes, `install <image>` with a name | source |
| install from tarball / OCI layout | yes (layout marker-detected, blobs verified) | no | roadmap 10.1, confirmed by round 2 |
| install from a URL | yes, **length-checked only - there is no `--checksum`** | no | `grep checksum` across `install*`/`download` -> 0 hits |
| portable export | `build -o FILE` (`oci_writer.py:155`) | no | round 2; there is no `export` subcommand in either tool |
| command surface | 18 commands (`pages.py:1061`) | 123 help screens, pinned as a fixture (`ebdd50e2d8aa629f`) | both counted on this device |
| install wall time | MEASURE: `time pd install alpine` | median **12,474 ms** over the campaign's installs | campaign METRICS.md, 606 records |
| `list` latency | MEASURE: same command on the reference | **33-88 ms**, five runs, metadata-only store | `workbench/R2-PDN-BASELINE.md` |
| store footprint | MEASURE: `du -sh` of its containers dir | **853 MB** across 10 containers, with two near-duplicate Debian trees (427 MB + 173 MB) - no layer dedup | `workbench/R2-PDN-BASELINE.md` |
| code size, for context | **29,015 lines of Python** | **61,713 lines of Rust** in 157 files (pn-next 22,831, podnext 12,677, proot-distro-core 9,994, proot-core 3,598) | counted on this device |

Verdict: mobility is where PDN is behind, and it is also where round 2 found **23 mechanisms** the comparison
rewards. R3 re-orders v0.3.0 to attack C4 and C3 first for exactly this reason.

## 2. Capability

| what | reference | PDN | verdict |
|---|---|---|---|
| container engine inside (Docker/Podman) | no | no | parity; the refusal now cites dockerd's own gate (`daemon/daemon.go:1130-1132`, B15) |
| init systems (systemd/OpenRC) | no | no | parity |
| namespaces / mount(2) / cgroups | no | no | parity - round 2: these belong to the UML backend (roadmap 4.1), and **omitting them costs PDN nothing against proot-distro** |
| image/content store with layer blobs | no (#37/#39) | no (#37/#39) | **the one capability YES**: five mechanisms named in the confirmation table |
| guest FUSE | no | no | parity |

Verdict: capability is a parity axis today, not a losing one. The only thing that changes it for v0.3.0 is the image
store, and that changes the *mobility* story (import/export/dedup), not the envelope.

## 3. Stability - the axis where PDN has been spending its effort, measured

| evidence | count |
|---|---|
| defect classes found and fixed across six waves | 39 items: A1-A18 + B1-B15 (33 done and CI-verified), plus A19-A24 opened by this round |
| of those, "a claim with no mechanism" (the honesty class) | 6: A7 backup, A8 inspect, A10 daemon status, A14 daemon stop/reload/logs, **A20 cgroup limits**, **A21 seccomp install** |
| CI state | `2374ebd` and every wave head: 5/5 green; the two reds since were a pinned-fixture drift and a non-goal ceiling, both fixed at their source |
| orphan survivors | measured in the campaign: 2.730 s and 3.046 s with PPID 1 - **A22 is closing this now**, the one stability mechanism round 2 marked YES |
| gate gaps found in the parent's own tooling | 3: a piped gate, a `--cargo` half skipped, and a non-goal stage that certified itself instead of measuring - all three now fixed in `scripts/local_gate.sh` |

Verdict: this is PDN's strongest axis and it is measurable rather than asserted - and the two honesty defects found by
reading the comparators show the class is not exhausted.

## 4. Optimization

| what | number | how |
|---|---|---|
| `list` (metadata read path) | 33-88 ms, median ~35 ms | five runs on this device |
| daemon status latency | p50 **5.20 ms** vs the 1.48 ms target - the one roadmap 9 exception | campaign METRICS |
| install (network-bound) | median 12,474 ms | campaign METRICS |
| the reference's shape, as a target | shroot (a third implementation) measures 12.3x faster than PRoot on 16,384 `stat` calls, 6.6x on `open/close`, 3.6x on file churn - **but 1.53x SLOWER on `fork/exec/wait`** | `docs/shroot-benchmarks.md`, columns as published |
| why that matters here | selective interception is a different execution model, not a v0.3.0 patch: round 2 marked it **NO** for this release, and the worse number is published by its own authors | round 2, R2-EXECUTION |

Verdict: optimization has one honest open exception (daemon latency) and no v0.3.0 mechanism attached to it. Claiming
a speed win for v0.3.0 would be inventing one.

## What this matrix says v0.3.0 is

Not a capability release and not a performance release. It is the release where PDN becomes the better **mobility**
tool - the login polygon, archive and OCI import, portable export, a checksummed URL install the reference cannot
match - while closing the honesty defects and the orphan leak that its own testing found. The envelope stays narrow,
and the documents say so.

## Limits of this document

Every MEASURE row is a gap in evidence, not a claim. `install` and `list` timings for the *reference* on this device
were not taken; the PDN numbers come from the campaign's 606 records and from a read-only baseline run. The comparison
of code size is context, not a verdict - fewer lines with fewer features is not a win.
