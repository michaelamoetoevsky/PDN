# Issue inventory for the PDN tracker

Every issue in `michaelamoetoevsky/PDN`, checked against the code rather than against its own
label, on 5 November 2026. Generated from `gh issue list --state all` and the ledger's own tables;
regenerate by re-running the script that produced this file and comparing the counts.

**Reproduce the classification:**

```sh
gh issue list --state all --limit 300 --json number,title,state,labels
python3 scripts/build_issue_ledger.py --offline    # the predicate verdicts
```

## Counts

| measure | value |
|---|---|
| issues on the tracker | 61 |
| closed | 44 |
| open | 17 |
| open, wayfinder work tickets | 6 |
| open, declined with a written reason | 10 |
| open, backlog feature requests | 0 |
| issues carrying ledger predicates | 33 |
| issues declined with a written reason | 14 |

## What this pass found: six issues were open with the work already done

#41, #42, #43, #45, #46, #47 each had every predicate passing while the issue stayed open -
the defect was fixed and verified, the ticket was never closed. They were closed in this pass, each
with its own predicate list and verdicts quoted from the ledger at head `32594ee`, where all 102
predicates carried verdicts (102 PASS, 0 FAIL, 0 SKIPPED). This is the failure mode a tracker has
and a gate does not: a gate cannot go stale, a status field can.

The other open issues are open on purpose: the declined ones say why they are not fixed, and the
backlog items are feature requests with no commitment attached.

## Every issue

| # | state | class | disposition | title |
|---|---|---|---|---|
| #1 | OPEN | declined | declined with a reason | Add shell completion for PowerShell |
| #2 | OPEN | declined | declined with a reason | Add example service configurations for common applications |
| #3 | OPEN | declined | declined with a reason | Create tutorial: Running a full-stack development environment in PDN |
| #4 | OPEN | declined | declined with a reason | Add VS Code Dev Container support |
| #5 | OPEN | declined | declined with a reason | Add systemd service unit generator for Linux hosts |
| #6 | CLOSED | declined | declined with a reason | Add Docker Compose export for PDN services |
| #7 | CLOSED | checked | 2 predicate(s) | Add GitHub Actions workflow for automated dependency updates |
| #8 | CLOSED | checked | 3 predicate(s) | Add benchmark suite with Criterion for performance regression detection |
| #9 | OPEN | declined | declined with a reason | Add Prometheus metrics exporter for PDN services |
| #10 | OPEN | declined | declined with a reason | Add WebSocket support for real-time service events |
| #11 | CLOSED | checked | 1 predicate(s) | [BUG-C1] install accepts empty container name and creates an unremovable orp |
| #12 | CLOSED | checked | 2 predicate(s) | [BUG-C2] backup delete deadlocks permanently (tokio RwLock read guard held a |
| #13 | CLOSED | checked | 1 predicate(s) | [BUG-C3] reset destroys the container before verifying the image is download |
| #14 | CLOSED | checked | 1 predicate(s) | [BUG-M1] podnext ps does not report non-interactive run sessions |
| #15 | CLOSED | checked | 2 predicate(s) | [BUG-M2] concurrent guest writes to one file clobber each other (4/5) |
| #16 | CLOSED | checked | 3 predicate(s) | [BUG-M3] deep tree extraction scales super-linearly and times out past ~200  |
| #17 | CLOSED | checked | 4 predicate(s) | [BUG-L1] podnext build local rejects ARG, skips RUN, and produces no usable  |
| #18 | CLOSED | checked | 3 predicate(s) | [BUG-L2] copy/sync expand sparse files to dense allocation |
| #19 | CLOSED | checked | 1 predicate(s) | [CI-1] test_restart_policy_unless_stopped fails: explicit stop loses to rest |
| #20 | CLOSED | checked | 1 predicate(s) | [BUG-C4] FileLock::lock flocks fd 0 (stdin) and then panics on a None field |
| #21 | CLOSED | checked | 1 predicate(s) | [BUG-C5] a guest command containing -r aborts with 'rootfs is required for d |
| #22 | CLOSED | checked | 1 predicate(s) | [CI-2] dependency-updates workflow references a nonexistent action (peter-ev |
| #23 | CLOSED | checked | 1 predicate(s) | [CI-3] Cargo.lock is gitignored in an application workspace, so --locked was |
| #24 | CLOSED | checked | 1 predicate(s) | [SEC-1] 35 advisories in the dependency tree: inventory, exposure analysis a |
| #25 | CLOSED | checked | 2 predicate(s) | [CI-4] Benchmark workflow is a false green: cargo bench fails to compile and |
| #27 | CLOSED | declined | declined with a reason | [BUG-DOC1] Documented gateway fail-safe config (bulkhead, retry_policy) does |
| #28 | CLOSED | declined | declined with a reason | Compose export: empty command emits an invalid service; follow-ups from PR # |
| #29 | CLOSED | declined | declined with a reason | podnext build local computes a layer diff but produces no image |
| #30 | CLOSED | checked | 2 predicate(s) | [CI-5] Benchmark job is unpredictable: 25.7 min then 43+ min, and a baseline |
| #31 | CLOSED | checked | 3 predicate(s) | [CI-6] test_restart_policy_on_abnormal is flaky: assertions bet on wall-cloc |
| #32 | CLOSED | checked | 4 predicate(s) | Signal lifecycle: stopping a PDN process orphans the guest, and podnext-daem |
| #33 | CLOSED | checked | 2 predicate(s) | Session files and service logs grow without bound (46 dead session files mea |
| #34 | CLOSED | checked | 4 predicate(s) | pn-gateway: documented duration form ("60s") cannot be loaded by the new TOM |
| #35 | CLOSED | checked | 6 predicate(s) | podnext: a tag containing ':' silently leaves /dev/shm and the /proc stubs u |
| #37 | OPEN | declined | declined with a reason | image push/tag/rm/history cannot see any image: ImageManager never loads fro |
| #38 | CLOSED | checked | 4 predicate(s) | A FROM scratch image is unreachable by run/list/remove after a successful bu |
| #39 | OPEN | declined | declined with a reason | push: the store holds no layer blobs, so push cannot work even once the look |
| #40 | CLOSED | checked | 4 predicate(s) | LICENSE, Cargo.toml and README.md disagree about the project licence |
| #41 | CLOSED | checked | 4 predicate(s) | A non-UTF-8 byte in any argument aborts podnext with SIGABRT (std::env::args |
| #42 | CLOSED | checked | 7 predicate(s) | [BUG-DOC] CLI reference documented options that do not exist (exit codes 3-1 |
| #43 | CLOSED | checked | 6 predicate(s) | [BUG] --bind/--volume were no-ops: the flag was discarded and the rendered b |
| #44 | CLOSED | checked | 4 predicate(s) | [BUG] image ls printed a size that was never measured and called the contain |
| #45 | CLOSED | checked | 6 predicate(s) | [GAP §9.3] podnext doctor did not exist: no host/guest diagnostic report |
| #46 | CLOSED | checked | 7 predicate(s) | [GAP §9.2] Path defaults were hard-coded to one machine's Termux home |
| #47 | CLOSED | checked | 6 predicate(s) | [BUG] run accepted nine options and dropped all of them, while --help advert |
| #48 | OPEN | map | wayfinder map | Wayfinder map: v0.1.9 shipped (roadmap section 9 met) |
| #49 | CLOSED | work | wayfinder ticket | CLI split: extract the observability domains (policy, quota, log, dbus, sock |
| #50 | CLOSED | work | wayfinder ticket | CLI split: extract the supervision domains (service, daemon, watchdog, boot, |
| #51 | CLOSED | work | wayfinder ticket | CLI split: move the Clap surface out of main.rs into cli.rs |
| #52 | CLOSED | work | wayfinder ticket | Section 9.2: cache the container index with mtime invalidation (p50 < 10 ms) |
| #53 | CLOSED | work | wayfinder ticket | Release checklist: CHANGELOG, README What's New, known limitations, roadmap  |
| #54 | OPEN | work | wayfinder ticket | [dept/build] Extract the build, gitops and compose domains out of commands/m |
| #55 | CLOSED | work | wayfinder ticket | [shell] Assert the argv quoting table against a real guest (roadmap 9.3) |
| #56 | OPEN | work | wayfinder ticket | [shell] Measure terminal fidelity: resize, TERM, PS1, UTF-8 locale (roadmap  |
| #57 | OPEN | work | wayfinder ticket | [shell] detach/attach round trip and session hygiene (roadmap 9.3) |
| #58 | OPEN | work | wayfinder ticket | [shell] Soak: repeated start/stop leaves nothing behind (roadmap 9.3) |
| #59 | OPEN | work | wayfinder ticket | [perf] Route read-only metadata through podnext-daemon (roadmap 9.2, last cr |
| #60 | OPEN | work | wayfinder ticket | [security] Decide the seccomp profiles: verify per distro or decline with th |
| #61 | CLOSED | work | wayfinder ticket | [docs] Record the v0.1.9 dispositions: what is fixed, what is declined, what |
| #62 | CLOSED | checked | 3 predicate(s) | [BUG] run --seccomp is accepted and never applied |
| #63 | OPEN | declined | declined with a reason | [BUG] direct-execution argv scanner parses --no-kill-on-exit and never uses  |

## What remains, and where each is tracked

| Remaining roadmap work | Ticket |
|---|---|
| Extract the build, gitops and compose domains out of `commands/mod.rs` | #54 |
| Terminal fidelity: resize, `TERM`, `PS1`, UTF-8 locale measured inside the guest | #56 |
| `detach`/`attach` round trip and session hygiene | #57 |
| Soak: repeated start and stop leaves no guest process behind | #58 |
| Route read-only metadata through `podnext-daemon` (the end-to-end latency criterion) | #59 |
| Decide the seccomp profiles: verify per distro, or decline with the measurement | #60 |
| `--no-kill-on-exit` in the direct-execution path: measure, then decide | #63 |
| Execute the documented examples in `cli-reference.md`, with a written exclusion list | #64 |
| Bring `cli.rs`, `main.rs` and `commands/mod.rs` under the ~800-line ceiling, or record the exception | #65 |
| Measure the bindings and the `USER` story inside a guest | #66 |
| The map that holds all of the above | #48 |

Release state: `Cargo.toml` is `0.1.8`, `v0.1.9` was withdrawn, and the next tag is `v0.2.0`
according to `docs/src/roadmap.md` section 10. Sections 7.1 and 7.2 (seccomp, zero-trust boundary)
are **declined** for v0.1.9 with the measurement recorded; nothing in this list is claimed as done
until its ticket carries evidence.

## What this document does not claim

It does not claim an issue is correct because it is closed, nor that a closed issue's fix is
complete - only that its predicates pass. A predicate proves the specific thing it greps for and
nothing more; the limits of each are written next to the check in `scripts/build_issue_ledger.py`.
