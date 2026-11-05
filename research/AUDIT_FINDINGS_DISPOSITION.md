# Dispositions for the audit findings (roadmap section 9.1)

Section 9.1 asks that every finding be triaged: fixed, filed, or declined with a reason. The
ledger enforces the rule mechanically - an issue with neither a predicate nor a hand-written
reason now fails the gate, and a generated fallback no longer masquerades as a disposition. This
file is the prose form of the same thing, so a reader of the release does not have to read Python.

Generated from `gh issue list --state all` and the ledger's own tables; regenerate with
`scripts/build_issue_ledger.py` and compare. Counts in the ledger at the time of writing:
`33` issues carry predicates, `14` carry a written reason, and
`14` are
wayfinder work items with their acceptance criteria in the map issue.

## Findings

| # | state | title | disposition |
|---|---|---|---|
| #1 | OPEN | Add shell completion for PowerShell | declined: OPEN feature request (PowerShell completion) — nothing on the tree to verify. |
| #2 | OPEN | Add example service configurations for common applications | declined: OPEN documentation task (example service configs) — no code fix to assert. |
| #3 | OPEN | Create tutorial: Running a full-stack development environment in PDN | declined: OPEN documentation task (tutorial) — no code fix to assert. |
| #4 | OPEN | Add VS Code Dev Container support | declined: OPEN feature request (VS Code Dev Container) — no landed implementation. |
| #5 | OPEN | Add systemd service unit generator for Linux hosts | declined: OPEN feature request (systemd unit generator) — no landed implementation. |
| #6 | CLOSED | Add Docker Compose export for PDN services | declined: OPEN feature request (Docker Compose export) — partially landed via PR #26; see #28. |
| #7 | CLOSED | Add GitHub Actions workflow for automated dependency updates | fixed or present: 2 predicate(s) in the ledger |
| #8 | CLOSED | Add benchmark suite with Criterion for performance regression detection | fixed or present: 3 predicate(s) in the ledger |
| #9 | OPEN | Add Prometheus metrics exporter for PDN services | declined: OPEN feature request (Prometheus exporter) — no landed implementation. |
| #10 | OPEN | Add WebSocket support for real-time service events | declined: OPEN feature request (WebSocket events) — no landed implementation. |
| #11 | CLOSED | [BUG-C1] install accepts empty container name and creates an unremovable orpha | fixed or present: 1 predicate(s) in the ledger |
| #12 | CLOSED | [BUG-C2] backup delete deadlocks permanently (tokio RwLock read guard held acr | fixed or present: 2 predicate(s) in the ledger |
| #13 | CLOSED | [BUG-C3] reset destroys the container before verifying the image is downloadab | fixed or present: 1 predicate(s) in the ledger |
| #14 | CLOSED | [BUG-M1] podnext ps does not report non-interactive run sessions | fixed or present: 1 predicate(s) in the ledger |
| #15 | CLOSED | [BUG-M2] concurrent guest writes to one file clobber each other (4/5) | fixed or present: 2 predicate(s) in the ledger |
| #16 | CLOSED | [BUG-M3] deep tree extraction scales super-linearly and times out past ~200 le | fixed or present: 3 predicate(s) in the ledger |
| #17 | CLOSED | [BUG-L1] podnext build local rejects ARG, skips RUN, and produces no usable OC | fixed or present: 4 predicate(s) in the ledger |
| #18 | CLOSED | [BUG-L2] copy/sync expand sparse files to dense allocation | fixed or present: 3 predicate(s) in the ledger |
| #19 | CLOSED | [CI-1] test_restart_policy_unless_stopped fails: explicit stop loses to restar | fixed or present: 1 predicate(s) in the ledger |
| #20 | CLOSED | [BUG-C4] FileLock::lock flocks fd 0 (stdin) and then panics on a None field | fixed or present: 1 predicate(s) in the ledger |
| #21 | CLOSED | [BUG-C5] a guest command containing -r aborts with 'rootfs is required for dir | fixed or present: 1 predicate(s) in the ledger |
| #22 | CLOSED | [CI-2] dependency-updates workflow references a nonexistent action (peter-evan | fixed or present: 1 predicate(s) in the ledger |
| #23 | CLOSED | [CI-3] Cargo.lock is gitignored in an application workspace, so --locked was i | fixed or present: 1 predicate(s) in the ledger |
| #24 | CLOSED | [SEC-1] 35 advisories in the dependency tree: inventory, exposure analysis and | fixed or present: 1 predicate(s) in the ledger |
| #25 | CLOSED | [CI-4] Benchmark workflow is a false green: cargo bench fails to compile and t | fixed or present: 2 predicate(s) in the ledger |
| #27 | CLOSED | [BUG-DOC1] Documented gateway fail-safe config (bulkhead, retry_policy) does n | declined: OPEN defect: documented [gateway.fail_safe] (bulkhead/retry_policy) absent from pn-gateway; |
| #28 | CLOSED | Compose export: empty command emits an invalid service; follow-ups from PR #26 | declined: OPEN defect: empty command yields a null Compose service in export_to_compose; no fix landed. |
| #29 | CLOSED | podnext build local computes a layer diff but produces no image | declined: OPEN defect: podnext build local reports success without an image; no fix landed. |
| #30 | CLOSED | [CI-5] Benchmark job is unpredictable: 25.7 min then 43+ min, and a baseline s | fixed or present: 2 predicate(s) in the ledger |
| #31 | CLOSED | [CI-6] test_restart_policy_on_abnormal is flaky: assertions bet on wall-clock  | fixed or present: 3 predicate(s) in the ledger |
| #32 | CLOSED | Signal lifecycle: stopping a PDN process orphans the guest, and podnext-daemon | fixed or present: 4 predicate(s) in the ledger |
| #33 | CLOSED | Session files and service logs grow without bound (46 dead session files measu | fixed or present: 2 predicate(s) in the ledger |
| #34 | CLOSED | pn-gateway: documented duration form ("60s") cannot be loaded by the new TOML  | fixed or present: 4 predicate(s) in the ledger |
| #35 | CLOSED | podnext: a tag containing ':' silently leaves /dev/shm and the /proc stubs unb | fixed or present: 6 predicate(s) in the ledger |
| #37 | OPEN | image push/tag/rm/history cannot see any image: ImageManager never loads from  | declined: OPEN defect: image ls/push said they could not see any image because ImageManager read only |
| #38 | CLOSED | A FROM scratch image is unreachable by run/list/remove after a successful buil | fixed or present: 4 predicate(s) in the ledger |
| #39 | OPEN | push: the store holds no layer blobs, so push cannot work even once the lookup | declined: OPEN defect: push cannot work because the store holds no layer blobs. The CLI now refuses |
| #40 | CLOSED | LICENSE, Cargo.toml and README.md disagree about the project licence | fixed or present: 4 predicate(s) in the ledger |
| #41 | OPEN | A non-UTF-8 byte in any argument aborts podnext with SIGABRT (std::env::args p | fixed or present: 4 predicate(s) in the ledger |
| #42 | OPEN | [BUG-DOC] CLI reference documented options that do not exist (exit codes 3-12, | fixed or present: 7 predicate(s) in the ledger |
| #43 | OPEN | [BUG] --bind/--volume were no-ops: the flag was discarded and the rendered bin | fixed or present: 6 predicate(s) in the ledger |
| #44 | CLOSED | [BUG] image ls printed a size that was never measured and called the container | fixed or present: 4 predicate(s) in the ledger |
| #45 | OPEN | [GAP §9.3] podnext doctor did not exist: no host/guest diagnostic report | fixed or present: 6 predicate(s) in the ledger |
| #46 | OPEN | [GAP §9.2] Path defaults were hard-coded to one machine's Termux home | fixed or present: 7 predicate(s) in the ledger |
| #47 | OPEN | [BUG] run accepted nine options and dropped all of them, while --help advertis | fixed or present: 6 predicate(s) in the ledger |
| #48 | OPEN | Wayfinder map: v0.1.9 shipped (roadmap section 9 met) | work item: tracked by the map #48 |
| #49 | CLOSED | CLI split: extract the observability domains (policy, quota, log, dbus, socket | work item: tracked by the map #48 |
| #50 | CLOSED | CLI split: extract the supervision domains (service, daemon, watchdog, boot, w | work item: tracked by the map #48 |
| #51 | CLOSED | CLI split: move the Clap surface out of main.rs into cli.rs | work item: tracked by the map #48 |
| #52 | CLOSED | Section 9.2: cache the container index with mtime invalidation (p50 < 10 ms) | work item: tracked by the map #48 |
| #53 | CLOSED | Release checklist: CHANGELOG, README What's New, known limitations, roadmap st | work item: tracked by the map #48 |
| #54 | OPEN | [dept/build] Extract the build, gitops and compose domains out of commands/mod | work item: tracked by the map #48 |
| #55 | CLOSED | [shell] Assert the argv quoting table against a real guest (roadmap 9.3) | work item: tracked by the map #48 |
| #56 | OPEN | [shell] Measure terminal fidelity: resize, TERM, PS1, UTF-8 locale (roadmap 9. | work item: tracked by the map #48 |
| #57 | OPEN | [shell] detach/attach round trip and session hygiene (roadmap 9.3) | work item: tracked by the map #48 |
| #58 | OPEN | [shell] Soak: repeated start/stop leaves nothing behind (roadmap 9.3) | work item: tracked by the map #48 |
| #59 | OPEN | [perf] Route read-only metadata through podnext-daemon (roadmap 9.2, last crit | work item: tracked by the map #48 |
| #60 | OPEN | [security] Decide the seccomp profiles: verify per distro or decline with the  | work item: tracked by the map #48 |
| #61 | CLOSED | [docs] Record the v0.1.9 dispositions: what is fixed, what is declined, what m | work item: tracked by the map #48 |
| #62 | CLOSED | [BUG] run --seccomp is accepted and never applied | fixed or present: 3 predicate(s) in the ledger |
| #63 | OPEN | [BUG] direct-execution argv scanner parses --no-kill-on-exit and never uses it | declined: OPEN defect of the compiler-warning class: the direct-execution scanner assigns |

Untriaged issues: **0** - the rule holds for every issue on the tracker.

## What changed in this round

Three findings had never been triaged, and the ledger used to paper over that with a default
sentence. They are now handled with evidence rather than assumption:

- **#41** (a non-UTF-8 argument aborts the process) was *claimed* fixed and had a regression test
  that only ever ran `list`. Adding the "the panicking reader is gone from the tree" predicate
  found a live occurrence at `crates/podnext/src/commands/mod.rs` - the `build local` success path
  called `std::env::args()`, which panics re-reading cached argv. Fixed to `args_os()` with
  `to_string_lossy()`; the predicate now passes, and it caught the same line coming *back* when the
  integration took that file from a branch cut before the fix. It is the guard that keeps the
  reader from returning.
- **#39** and **#37** (push cannot work; the image commands could not see an image) are declined
  for v0.1.9 with the reason recorded: a real image store is the v0.2.0 milestone (roadmap section
  4.4). What the release does instead is refuse honestly, which the stub checks assert.
- **#62** (`--seccomp` accepted and never applied) was measured inert - the proot argv is
  byte-identical with and without it - and now refuses with that measurement on stderr.
- **#63** (the direct-execution scanner assigns `no_kill_on_exit` and never reads it) is filed
  with the compiler's own warning as evidence. It predates this round, so it is reported rather
  than folded into an unrelated commit, and its disposition says why no check exists: the honest
  check is a build-warning gate, which this ledger does not have.

## Partial roadmap items

These are the items the roadmap describes but where only part of the work exists. Each line says
what is landed, what is missing, and where the rest is tracked.

| Roadmap item | landed | missing | tracked |
|---|---|---|---|
| 5.3 CRI and OCI compliance | OCI is used for image layers; a `cri` feature exists | the feature is off: no gRPC surface | #60 decides its disposition |
| 6.4 build | `build local` produces a container | layer dedup, image prune | #54 (extraction) and v0.2.0 store work |
| 6.7 networking | DNS resolution inside the guest | userspace TCP/IP (gvproxy/slirp) | not in this release; belongs with the UML backend |
| 6.8 storage | whiteouts are obeyed on extract | layer dedup, opaque-directory marker, import | #54 and the v0.2.0 import work |
| 7.1 zero-trust boundary | profiles, path-traversal refusal, no-exec policy | measurement of what is enforced | #60 |
| 7.2 seccomp | profile and rule types, a `--seccomp` flag | **measured inert** (below); per-distro tailoring absent | #60 |
| 8 (phases 2-4) | builders, dual-runtime client, boot and wakelock | dedup/prune, gvproxy, runit unit not installed, CRI | #54, #59, #60 |
| 9.1 | labels, stub refusals, honest sizes, path env, doctor, exit-code taxonomy, triage enforced | documented examples not executed | this release |
| 9.2 | the CLI split (main 1,040 / cli 1,445 / commands 1,972), size measurement, path resolution, CI relocation proof, the index cache in-process | the end-to-end latency criterion | #59 |
| 9.3 | `doctor` including the proot argv, the argv quoting table | terminal fidelity, detach/attach, soak | #56, #57, #58 |

## The seccomp flag is inert, measured

`podnext run --seccomp` changes nothing. Measured on this device, not read out of the source:

- the captured proot argv is token-for-token identical with and without the flag, and contains no
  seccomp token at all (0 occurrences; 133 `--bind` entries both times);
- this PRoot build has no seccomp option: `proot --help` lists `--sysvipc` and `--kernel-release`
  and nothing else in that family;
- the field the flag fed is never read - `crates/proot-core/src/seccomp.rs` carries the compiler
  warning `field filter_program is never read`, the same fact from the compiler's side;
- no container sets `security.seccomp_profile`, and no test expects the flag to work, so refusing
  it breaks nothing.

Refusing an option that asks for isolation and applies none is the same rule the eight stubs
follow. Roadmap sections 7.1 and 7.2 stay declined for v0.1.9 until a runtime that can enforce a
filter exists to assert against.

## What this document does not claim

It does not claim the partial items are done. It does not claim anything about behaviour that was
never run: the ledger's SKIPPED checks have no verdict without `--cargo`, and in CI they all
carried verdicts at head `32594ee` (102 PASS, 0 FAIL, 0 SKIPPED) - which is a statement about one
commit, not a promise about the next.
