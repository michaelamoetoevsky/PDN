# Change log — source fixes, with provenance

One entry per source change: what was found, what was measured, what was changed (file and line), when, and
the verification that actually ran. Timestamps are WIB (UTC+7) on the device.

---

## ASEC3-SEC-001 / ASEC3-SEC-002 — `copy` and `sync` wrote outside the rootfs through a symlink whose leaf did not exist

**Found.** `guest_to_host()` in `crates/podnext/src/commands/sync.rs` refuses `..` lexically, then performs
its real-path containment check only when `std::fs::canonicalize(&resolved)` succeeds — that is, only when
the destination already exists. A copy creates its destination, so the interesting case never reached the
check: with `<rootfs>/esc -> /somewhere/else`, a destination of `c:esc/pwn.txt` skipped the check entirely
and the write followed the link out of the rootfs.

**Reproduced 2026-11-08, 07:50 WIB**, against the binary built from commit `48e86b7`
(`./target/debug/podnext`, 349,099,888 bytes, built 08 Nov 00:37) with a fixture store selected through
`PN_TEST_ROOT` — the operator's real store was not touched:

| case | command | result |
|---|---|---|
| subject | `podnext copy $F/src/payload.txt esc:esc/pwn.txt` | `rc=0`, printed `Copied …`, and `$F/outside/pwn.txt` contained `hello-from-host` — **written outside the rootfs** |
| control 1 | `podnext copy … esc:plain/ok.txt` (no symlink) | written inside the rootfs, as intended |
| control 2 | `podnext copy … esc:esc/existing.txt` (leaf pre-created) | refused: `path 'esc/existing.txt' resolves outside the container rootfs (…)` |

Control 2 is what proves the hole is precisely the missing leaf: the same link is refused the moment the
leaf exists.

**Action.** The containment check now walks up to the deepest ancestor that does exist and verifies *that*
one, stopping at the lexical rootfs boundary. Legitimate shapes are unaffected: a plain path with a missing
leaf is a normal copy, and a relative symlink that stays inside the rootfs (Debian's `/etc/os-release` is
this shape) still resolves.

**Change.** `crates/podnext/src/commands/sync.rs`
- lines 133–171 (was 133–146): the single `if let (Ok(root), Ok(real)) = (canonicalize(rootfs), canonicalize(resolved))`
  block is replaced by an ancestor walk with the same refusal message.
- lines 610–677 (new): `#[cfg(test)] mod tests` with `missing_leaf_under_a_symlinked_parent_is_refused`,
  which asserts the subject plus four controls (plain missing leaf allowed, in-rootfs relative link allowed,
  existing leaf through the escaping link still refused, existing leaf behind the in-rootfs link still
  reachable).

**Verification.** Two routes, each with a negative control on the pre-fix binary
(`~/.hermes/cache/scratch/wt-asec-fs/target/debug/podnext`, built 01:43) and a positive result on the
fixed one (built 07:56), all against fixture stores selected with `PN_TEST_ROOT`:

| route | pre-fix binary | fixed binary |
|---|---|---|
| `copy $F/src/payload.txt esc:esc/pwn.txt` | `rc=0`, `Copied …`, wrote `$F/outside/pwn.txt` | `rc=1`, `path 'esc/pwn.txt' resolves outside the container rootfs`, nothing written outside |
| `sync $F/src/tree esc:esc/newdir` (subdirectory missing) | `Synced … 2 copied`, then 2 files landed outside (`outside/newdir/a.txt`, `outside/newdir/sub/b.txt`) | `rc=1`, same refusal message, nothing written outside |

Controls, all passing after the fix: `copy … esc:plain/ok.txt` and `sync … esc:inj/` still write inside the
rootfs (the latter reports `2 copied`); an in-rootfs relative symlink (`good -> real`) still resolves;
`copy … esc:esc/existing.txt` with a pre-created leaf is still refused; `copy … esc:../pwn2.txt` is still
refused for `..`. Note for the record: for `sync` the pre-fix hole only appeared when the destination
**subdirectory did not exist yet** — syncing onto an existing directory such as `esc:esc/` was already
refused, because that path canonicalized successfully. That is why the first sync attempt was not accepted
as evidence for this finding.

Unit test: `cargo test -p podnext --bin podnext -- sync::tests` → `1 passed; 0 failed` in 34 s (heavy label
`p1-sync`, log `~/.hermes/logs/heavy/20261108T005228Z-p1-sync.log`). Formatting: `cargo fmt --all -- --check`
→ rc=0. Commit `edd8d45`.

**Deliberately not changed.** The source-side symlink resolution in `cmd_copy` (lines 165–180) keeps
following a leaf link when copying *out* of a container, because that is `cp` semantics and the containment
check now covers it. `sync.rs`'s other write paths (`mirror_dir`, `prune_extras`) reach the same
`guest_to_host` translation through the command layer, so they inherit the fix; verifying that each of them
calls it before writing is recorded as a follow-up rather than assumed here.

---

## ASEC3-PROC2-01 — `kill` reported success and deleted the session record while the session was still live

**Found.** `cmd_kill` decided "gone" from the **registered pid** alone: after the ladder it pushed every
signalled session into `killed`, and the teardown called `session::wait_and_clear(pid, start_time, …)`,
which returns true as soon as `process_is_alive(pid)` is false — then deletes the record, the socket and the
transcript. But the registry's own liveness rule is the **inherited descriptor** (`active_sessions` probes
the session file's `flock`), and a guest that outlived its supervisor keeps that descriptor. So the record
could go while the session was still live by the rule `ps` uses: `Container 'x' killed.` printed, exit 0, and
`podnext ps` blind to a running container.

**Reproduced 2026-11-08, 08:15 WIB**, against the binary built before this change, using the same primitive
the code itself uses for liveness — `flock` on the session file — as an in-vitro stand-in for the surviving
guest:

| step | observation |
|---|---|
| live session in a real PTY (`tmux` + `podnext login fixt`, fixture store via `PN_TEST_ROOT`) | `podnext ps` → `sess-17425 fixt Running`, record `17425.json`, tree `tmux → podnext login → proot` |
| a second process takes `flock -x` on `17425.json` | "lock tertahan oleh proses lain" — the registry would call this session live |
| registered pid signalled, then `podnext kill fixt` | `Sent SIGTERM`, `Sent SIGQUIT`, **`Container 'fixt' killed.`**, **rc=0** |
| immediately after | session directory **empty** — the record was deleted — while the descriptor holder was still alive |

The first honest `kill` (no holder) was also measured and was **not** dishonest: the whole process tree
disappeared, the record was cleared, `ps` was empty, rc=0. So the defect is specifically the
pid-only liveness decision, not the ladder.

**Action.** Before clearing anything, ask the registry the same question `ps` asks, and keep the record while
the answer is yes; report success only when nothing survived.

**Change.** `crates/podnext/src/commands/container.rs`
- new `session_still_active()` next to `wait_until_gone` (around line 546): true when `active_sessions()`
  still lists this `(pid, container_name)`.
- in `cmd_kill`'s teardown (around lines 640–692): a session that is still active is pushed to `survivors`
  and skipped **before** `wait_and_clear`, so its files survive; a failed `wait_and_clear` also records a
  survivor; and a `survivors` list that is not empty ends the command with `anyhow::bail!` naming the PIDs
  and the ladder, instead of the success line.

**Verification** (fixed binary, 08:18, heavy label `p1b-bin`, build exit 0, peak 1410 MB):

| case | expected | measured |
|---|---|---|
| descriptor holder alive | refuse, keep the record | `rc=1`; `Error: session PID 7131 (fixt) survived the full ladder (SIGTERM, SIGQUIT, SIGKILL) and still holds its session descriptor, so the container is still running; its record was kept and podnext ps still lists it. Nothing was reported as killed.`; `7131.json` still present; `podnext ps` still lists `sess-7131` |
| fresh live session, no holder | succeed, no false refusal | `rc=0`; `Container 'fixt' killed.`; record cleared; 0 guest processes left |

Both cases used the real PTY harness. `cargo fmt --all -- --check` → rc=0. Repo gate: `local_gate.sh --cargo`
(log `~/.hermes/logs/heavy/20261108T012046Z-p1b-gate.log`).

**Note on the control's honesty.** The refusal case models a surviving guest with a lock holder rather than
producing one; what it tests is the *decision rule* the code uses, and on that rule the old binary was wrong
and the new one is right. A guest that survives its supervisor end-to-end was not produced in this harness
(it needs the detached-session path, which exits 1 in a fixture store — recorded as a separate anomaly).

---

## ASEC2-ARCH-013 / 014 / 015 / 016 and QUOTA-014 — the stress harnesses pointed at the operator's real store

**Found.** Three harnesses derived their paths from `HOME` and used fixed container names, so
`casino_chaos_runner.py` renamed and killed the real `test-alpine`, `dice_monkey_test.py` renamed and
reset the real `deb-renamed`/`ubuntu`/`alpine`, and `soak_sessions.py` logged into and SIGKILLed the real
`alpine` (logins and kills measured by the assessment). `ASEC2-ARCH-016` explains why a
`PN_TEST_ROOT`-only redirect was not enough: the CLI resolves `PN_TEST_ROOT -> HOME -> ...` and appends
`.podnext` (`crates/proot-distro-core/src/paths.rs:27-53`) while the scripts resolved their own paths from
`HOME`, so the CLI acted on a fixture and the script acted on the real store. `QUOTA-014` is the same shape
inside the test rig: `tests/integration/common.rs` redirected services/runtime/log/cni to a tempdir but never
the home, and `ResourceQuotaManager::new` persists to `resolve_home()/.podnext/quotas.json`
(`crates/pn-next/src/resource_quota/mod.rs:190-196`). Evidence that it was not theoretical: the operator's
`~/.podnext/quotas.json` existed with the single key `quota-service`, which is the harness's own test
service name (`tests/integration/v2_criteria.rs:488,492`), and the harness ignored the write's `Result`
(`common.rs:517`) so the leak made no noise.

**An incident, recorded because it changed the fix.** The first version of the new guard was script-side
only. Running it with `PN_TEST_ROOT` set to a fixture, the harness's own paths were on the fixture but the
binary it invoked - `/data/data/com.termux/files/usr/bin/podnext`, hardcoded in the script - **ignores
`PN_TEST_ROOT`**, so `podnext rename test-alpine test-alpine-rnd1` renamed the container in the OPERATOR's
store. Measured before restoring: with `PN_TEST_ROOT` pointing at a fixture that held only `test-alpine`,
the installed binary listed `alpine, deb-pdn, deb-renamed, pdn-bench, test-alpine, test-pdn-install, ubuntu`
while the repo build listed only `test-alpine`. Restored and verified: the directory renamed back, and the
two fields the rename had rewritten returned to their previous values
(`name: test-alpine-rnd1 -> test-alpine`, `rootfs: .../test-alpine-rnd1/rootfs -> .../test-alpine/rootfs`),
then a read (`podnext copy test-alpine:/etc/alpine-release` -> `3.18.12`) proved the record resolves again.

**Action.** One shared guard (`scripts/pdn_fixture_guard.py`) with three refusals, used by every harness:

1. `store_root()` - harness paths come from `PN_TEST_ROOT/.podnext`; a path landing on the real store is
   refused unless `PDN_ALLOW_REAL_STORE=1` is set deliberately.
2. `resolve_binary()` - the binary under test is `PDNEXT_BIN` or the repo's debug build, never an arbitrary
   installed binary (the incident's cause).
3. `assert_binary_honours_store()` - a read-only `list` must return exactly the fixture's containers
   **before** the harness may do anything. This is the check that would have caught the incident.

Every refusal exits 2 and prints "REFUSING is not a pass." so a skipped guard can never be read as a
passing harness. `seed_fixture()` creates the named containers; `casino` seeds one from the real rootfs
(read-only copy, `tar` because a rootfs holds mode-000 bind directories that stop `cp -a`) and `dice` seeds
four with an empty rootfs and says so, because it only exercises the store side.

**Change (2026-11-08).**
- new `scripts/pdn_fixture_guard.py`
- `scripts/soak_sessions.py`: `container_rootfs()` derives from `PN_TEST_ROOT` and refuses the real store
  (`_guard`), with `PDN_ALLOW_REAL_STORE=1` as the explicit opt-out.
- `scripts/casino_chaos_runner.py`: `store_root()` + `seed_fixture()` + `resolve_binary()` +
  `assert_binary_honours_store()` before any chaos; `CONTAINERS_DIR` now derives from the fixture.
- `scripts/dice_monkey_test.py`: imports the guard; `PODNEXT_BIN`, `LOCK_DIR_PN_NEXT`, `LOCK_DIR_PODNEXT`,
  `SESSIONS_DIR` and the seeded targets all derive from the fixture.
- `tests/integration/common.rs`: `std::env::set_var("PN_TEST_ROOT", &temp_dir)` before the quota manager is
  constructed (edition 2021, so no `unsafe`; the same pattern already exists at
  `crates/proot-distro-core/tests/store_integrity.rs:59`).

**Verification.**
- `soak`: refusal `rc=2` with a named message; predicate checked three ways - `SystemExit(2)` with no
  opt-out, the path returned under `PDN_ALLOW_REAL_STORE=1`, and a path outside the store passing with no
  opt-out (no false refusal).
- `casino`: with the installed binary the probe refuses `rc=2` and prints the fixture's names against the
  real ones; with the repo build the probe passes and the chaos starts. Real store unchanged: 10 containers,
  `test-alpine` present, 0 `rnd*`, 0 sessions.
- `dice`: same two directions, fixture holding `['alpine','deb-pdn','deb-renamed','ubuntu']`; real store
  unchanged.
- `QUOTA-014`: `cargo test -p pdn-integration-tests --test v2_criteria -- test_all_v2_criteria` ->
  `1 passed; 0 failed` in 247.88 s (heavy label `v2crit`, peak 1839 MB). The operator's
  `~/.podnext/quotas.json` is byte-identical before and after - sha256 `d9e496855b74f52f`, mtime
  `05 Nov 13:55` - while that test does call `set_resource_quota("quota-service", "memory", 50 MiB)`.
- `cargo fmt --all -- --check` rc=0; `scripts/local_gate.sh --cargo` (label `p2-gate`).

**Not done, and said plainly.** `casino_chaos_runner.py` still carries an inline copy of the guard rather
than importing the shared module; the copy is behaviourally identical and was verified, but two copies of a
safety guard can drift, so folding it onto `pdn_fixture_guard` is a follow-up. The harness report path
(`scripts/casino_chaos_report.json` is a tracked file) means a run rewrites a tracked artifact; that was
reverted after the test run and is a harness-hygiene item outside these findings.

---

## WITHDRAWN, 8 Nov 2026 — the `kill` change in commit `6a9cd2a` is reverted, and ASEC3-PROC2-01 is open again

CI on `6a9cd2a` reported `Rust Host Build` failed with one test failing
(job 128590332193; the other 14 checks were green, including PRoot Distro Integration Test and Issue ledger):

```
test a_guest_ignoring_term_is_escalated_term_quit_kill ... FAILED
thread 'a_guest_ignoring_term_is_escalated_term_quit_kill' panicked at crates/podnext/tests/image_config_env.rs:497:5:
assertion `left == right` failed: kill failed: Error: session PID 7049 (pdn-kill-escalation) survived the
full ladder (SIGTERM, SIGQUIT, SIGKILL) and still holds its session descriptor, ...
```

That test states a deliberate contract: it starts `trap '' TERM QUIT; exec sleep 60`, registers that pid as a
live session, and asserts that a completed ladder prints `Container 'x' killed.` with exit 0. Its
`live_session()` helper **holds the session file's lock itself**, which is exactly what `active_sessions()`
probes, so the guard added in `6a9cd2a` saw a live session no matter what the ladder did. The CI log shows
the same shape in the wild: `Terminate orphan process: pid (4638) (sleep)`.

What this proves: the registry's descriptor rule answers "is anything still holding the session record",
which is **not** the same question as "is the guest still running". The change conflated the two, so it is
withdrawn rather than patched in place. `crates/podnext/src/commands/container.rs` is restored to its
`18523ea` state in the commit that carries this note.

Why the local gate missed it: the gate deliberately excludes test suites, and says so in its own header -
"No builds or test suites. Those belong in CI (and on this device they need the memory guard), so a local
gate that ran them would be skipped rather than respected" (`scripts/local_gate.sh:14-15`). So the remedy is
not to widen the gate: it is to run the target that owns the changed code before claiming the fix. For this
change that is
`cargo test -p podnext --test image_config_env -- a_guest_ignoring_term_is_escalated_term_quit_kill`, which
after the revert reports `1 passed; 0 failed` (heavy label `killtest`, 36 s). A gate widened against its own
stated rationale is a decision, not a cleanup, and it is not taken here.

The in-vitro reproduction recorded above still stands: the pre-fix binary printed `Container 'fixt' killed.`
with rc=0 while a descriptor holder was alive. What does not stand is that remedy. ASEC3-PROC2-01 returns to
open with sharper evidence: the remedy has to distinguish the guest from other descriptor holders (the
guest's own pid, or its process group), because refusing on any holder breaks the ladder's contract.

---

## Metrics 010 / 011 — the dashboard served invented host numbers, not measurements

**Found.** `update_system_stats` (`crates/pn-next/src/web_dashboard/mod.rs:800-807`) ended with

```rust
// Simulate CPU/memory
stats.cpu_percent = 25.0 + (rand::random::<f64>() * 30.0);
stats.memory_total = 8 * 1024 * 1024 * 1024; // 8GB
stats.memory_used = (stats.memory_total as f64 * 0.4) as u64;
stats.disk_total = 100 * 1024 * 1024 * 1024; // 100GB
stats.disk_used = (stats.disk_total as f64 * 0.3) as u64;
```

so a monitor that reads this dashboard as fact is told the machine has 8 GiB and a 100 GiB filesystem at
40 % and 30 % used, and a CPU percentage that is random. These values flow out through `/api/metrics`
(`:744-746`) and the JSON API, which is where the dishonesty stops being cosmetic.

**Action.** Read the host instead of decorating it, and make "could not measure" a state the surface can
express: `SystemStats` gains `measured: bool`, and the metric lines are emitted only when it is true, with
an explanatory comment line when it is not. A measured zero and "not measured here" are different
statements, and only a flag can tell them apart.

**Change (2026-11-08).** `crates/pn-next/src/web_dashboard/mod.rs`
- `:160-168` - `pub measured: bool` on `SystemStats`, with the reason it exists in its doc comment.
- `:784-848` - new `read_cpu_percent()` (delta of `/proc/stat` between polls; the first call reports the
  since-boot average, which is also a real figure), `read_memory_bytes()` (`/proc/meminfo`), and
  `read_disk_bytes()` (`statvfs`).
- `:858-884` - the fabricated block is replaced by those readings, and `measured` is their conjunction.
- `:752-762` - `/api/metrics` emits the three samples only when `measured`, otherwise a comment naming why.
- `:915-986` - two tests: the readers must satisfy relations a fabricated value need not (available never
  exceeds total, a percentage stays in range), and the served numbers must equal the host's own readings -
  the second is the discriminating one, since the old constants would pass any range check.

**Verification.** `cargo test -p pn-next web_dashboard` -> `4 passed; 0 failed` (heavy label `metrics2`,
75 s, peak 2763 MB), with the test printing which sources answered:
`answered here: memory=true disk=true cpu=false`. Formatting: `cargo fmt --all -- --check` rc=0.

**A measurement that corrected the change.** `/proc/stat` is **denied** to an app-uid process on Android
(measured: DITOLAK, while `/proc/meminfo` and `statvfs` answer). So the first version of the test - which
required all three sources - failed at `:936` on this device, and it was the test that was wrong: the code
had already returned `None` and flagged `measured = false`, which is the honest answer where a source is
denied. The tests now assert the contract instead of assuming availability, and they print which sources
answered rather than implying all three did.

---

## Media/UX 010 (wake lock) — the manager reported success for a lock that was never taken

**Found.** `acquire_platform` (`crates/pn-next/src/wake_lock/mod.rs:224-247`) logged `"Android: Would
acquire … wake lock"` on Android and `"Non-Android: Simulated acquire …"` elsewhere, then returned `Ok(())`.
`acquire()` therefore set `held = true` and `podnext wakelock acquire <name> <type>` printed
`Wake lock acquired for '<name>'` - for a lock nothing had taken.

**The facility exists, so this was fixable rather than merely disclosable.** Measured 2026-11-08:
`termux-wake-lock` exists at `$PREFIX/bin/termux-wake-lock`, and with the full path to the observer,
`/system/bin/dumpsys power` then lists
`PARTIAL_WAKE_LOCK 'termux:service-wakelock' ACQ=-2s86ms (uid=10396 pid=23284 pkg=com.termux)`, which
`termux-wake-unlock` removes. A trap on the way: `dumpsys` is **not on PATH** for a Termux shell
(`rc=127 No such file or directory`), so an earlier "0 matching lines" was a tool that could not run, not
evidence of absence.

**Change (2026-11-08).** `crates/pn-next/src/wake_lock/mod.rs`
- `:224-262` - `acquire_platform` runs `termux-wake-lock` and returns an error when the facility is
  missing or the command exits non-zero; `Full` and `DozeExemption` are refused by name because the
  facility provides a **partial** lock only, rather than being silently downgraded.
- `:264-286` - `release_platform` runs `termux-wake-unlock` with the same failure discipline.
- `:190-202` - `release` now asks the platform even when the in-process map has no record for the name.
  The lock is **app-level and outlives the process**, so a fresh `podnext wakelock release` always had an
  empty map: the old path did nothing and the CLI printed `Wake lock released for 'x'` anyway.
- `:407-428` - a test that an unsupported lock type is refused **and not recorded as held**.

**Verification, on the device.** `cargo test -p pn-next wake_lock` -> `4 passed; 0 failed` (heavy label
`wl2`, 80 s). End to end with the real binary, counting the platform's own lock through
`/system/bin/dumpsys power`:

| step | count of `PARTIAL_WAKE_LOCK … termux:service-wakelock` |
|---|---|
| before | 0 |
| `podnext wakelock acquire e1 partial` | 1 (`ACQ=-2s220ms pkg=com.termux`) |
| `podnext wakelock acquire demo3 full` | refused, `rc=1`, the name is in the message, count unchanged |
| `podnext wakelock release e1` | 0 |

Two of my own measurements were wrong before this and are recorded because they cost a round each: the
CLI takes the lock type **positionally** (`wakelock acquire <NAME> <LOCK_TYPE>`), so `acquire demo` and
`--type full` were rejected by clap and exercised nothing; and the first release check used a `release
<name> <type>` form that does not exist, so the release path went untested while the lock stayed.

**Limitation stated rather than hidden.** The app-level lock is process-agnostic: there is no unit test
that can assert "the platform lock is gone" without a device, so the release arm is proved by the table
above and by nothing else. Both directions were also observed to be genuinely observable (the count went
0 -> 1 -> 0 with the observer, not inferred from the CLI's own message).

---

## ASEC2-ARCH-006 / 007 / 009 (guardrails) — the engine claimed enforcement it does not perform

**Found.** In `check_all` (`crates/pn-gateway/src/guardrails.rs:57-81`) only `Deny` returns an error. The
`Throttle` arm logged `"Throttling due to guardrail: <rule>"` under a `// Apply throttling` comment while
applying nothing, and `Quarantine` logged `"Quarantining due to guardrail"` likewise. `check_condition`
(`:89-95`) returned `Ok(false)` unconditionally, which reads as "the condition does not hold" - a claim
about a condition it never evaluated.

**Two facts larger than the messages, measured while fixing them.**
- `GuardrailManager` is **never constructed outside its own tests**: a grep for `GuardrailManager` and
  `guardrails` across `crates/podnext/src` and `crates/pn-next/src` returns nothing, so no CLI path reaches
  this engine (the live policy path is the separate `pn-next` policy engine, used by `podnext policy eval`).
- Every metric the evaluators read is a **mock**: `get_container_count() -> Ok(0)`,
  `get_memory_usage_percent() -> Ok(0.0)`, `get_cpu_usage_percent() -> Ok(0.0)`,
  `get_disk_usage_percent() -> Ok(0.0)`, `check_privileged_containers() -> Ok(false)` (`:415-433`). Even if
  it were reachable, it could not judge anything real.

**Change (2026-11-08).** `crates/pn-gateway/src/guardrails.rs`
- `:57-89` - the `Throttle` and `Quarantine` arms now say the action was *requested and NOT applied by this
  engine* and carry the rule's own message; `Warn` and `Alert` name the rule and the message.
- `:96-112` - `check_condition` refuses by name: no condition language is implemented here, and a silent
  `false` is a claim the engine never made. Nothing in the tree called it, so this costs no caller.
- `:437-503` - a test module (the file had none): a triggered `Deny` must stop the check while
  `Throttle`/`Quarantine`/`Alert`/`Warn`/`Allow` must not fail it, and `check_condition` must refuse. The
  rule condition is `container_count >= 0`, which the evaluator really triggers - a condition that never
  fires would make every arm look correct.

**Verification.** `cargo test -p pn-gateway guardrails` -> `2 passed; 0 failed` (heavy label `guard`, 31 s,
peak 1015 MB). `cargo fmt --all -- --check` clean.

**Not fixed, and named as such.** Reachability (nothing constructs the manager) and the mock inputs are
product decisions - wiring an engine that cannot measure anything into the CLI would be worse than leaving
it unreachable - so they are recorded in the honesty register instead of patched around. What is fixed is
that the engine no longer *claims* what it does not do.

---

## ASEC2-ARCH-008 (policy) — a decision that nothing applies, printed as if it were an action

**Found.** `PolicyAction` (`crates/pn-next/src/security.rs:132,138`, re-exported at `pn-next/src/lib.rs:87`)
has **no consumer outside `policy_engine`**: nothing matches on it, so a rule can be evaluated and its action
never enforced. `podnext policy eval` printed the decided action on a line labelled `action :`, which reads
as "the action that was taken". Measured 2026-11-08 by searching for consumers across the tree: only the
definition and the re-export appear.

**Change (2026-11-08).**
- `crates/pn-next/src/policy_engine/mod.rs:52-58` - the doc comment on `PolicyAction` now states that the
  engine decides and nothing applies, names the finding, and says that wiring an enforcement point is a
  product decision rather than a rename.
- `crates/podnext/src/commands/policy.rs:126-129` - `policy eval` prints, next to the action,
  `applied by : nothing - this command evaluates; no component enforces the action`.

**Verification, and its limit.** The owning targets were run **before** committing, because this change
edits CLI output: `cargo test -p podnext --test stdout_discipline` -> `4 passed; 0 failed` and
`cargo test -p podnext --test cli_reference_contract` -> `6 passed; 0 failed`; `cargo fmt --all -- --check`
clean. The new line itself was **not** observed end to end: `podnext policy eval demo` against a fixture
store answers `Error: Service not found: demo`, because the command needs a service record and seeding one
means reproducing the supervisor's persisted `ServiceConfig` shape. So the line's presence rests on the
contract targets above, not on a captured run - stated here rather than implied by a green suite.

---

## ORCH (watchdog) — the README documented a command that does not load

**Found.** `README.md:246` showed `podnext watchdog policy web-server restart -m 3 -w 60`. Measured
2026-11-08 against the built binary: `watchdog policy` is **not a subcommand** (`error: unrecognized
subcommand 'policy'`, with clap suggesting `set-policy`/`setpolicy`), `-m`/`-w` are not flags at all, and
`-w` is already the global `--pwd`, so the short form could not be added later without a collision. The
stage spec carries the same form (`workbench/asec/SPEC_STAGE3.txt:128,305`), which is where the README
inherited it. The implemented surface is
`podnext watchdog setpolicy <NAME> <POLICY> <MAX_RESTARTS> <WINDOW_SECS>` with the policy vocabulary
`never, on-failure, always, unless-stopped, on-failure-or-crash`.

**Change (2026-11-08).** `README.md:245-246` now shows the form that loads:

```bash
# Configure automatic restart policy: up to 3 restarts within a 60 second window
podnext watchdog setpolicy web-server on-failure 3 60
```

**Verification, and one correction of my own first attempt.** The fixed line was run:
`rc=0`, `Watchdog policy set for 'web-server': OnFailure (max restarts: 3, window: 60s)`. The negative
control is the old form: `rc=2` with `unrecognized subcommand 'policy'`. My first edit kept the spec's
policy word `restart`, and the measurement rejected it (`error: invalid value 'restart' for '<POLICY>'`) -
so the documented form is now the one the binary accepts, checked by running it rather than by reading the
help. The rest of that README block was load-tested too (`watchdog status`, `gitops sync`, `gitops drift`,
`wakelock acquire web-server partial`, `boot enable web-server`): all reach their own logic, none are
rejected by the parser.

---

## SUPPLY-07 (signature) — a disabled verifier reported `verified: true`

**Found.** `verify_image` (`crates/proot-distro-core/src/signature.rs:132`) returned
`VerificationResult { verified: true, ... }` whenever `signature.enabled` was false - and
`SignatureConfig::default()` sets `enabled: false` (`:33`). So the struct a caller receives asserted a
verification that never ran, with only a warning string as a hint. Measured 2026-11-08: nothing in the
install path reads `SignatureConfig`/`SignatureVerifier` at all (grep in `podnext/` and `pn-*`: only the
`lib.rs:95-96` re-exports), and the sole readers of `.verified` are this module's own tests.

**Change (2026-11-08).** The early return now hands back `verified: false`, and the warning names the
remedy: `Signature verification disabled: not verified (set signatures.enabled to verify)`. Nothing else
changed: verification itself is still off by default, because turning it on is a product decision (it
needs trusted keys, and a wrong default would refuse installs).

**Verification.** Owner target `cargo test -p proot-distro-core --lib` -> `105 passed; 0 failed`,
including `signature::tests::test_simple_verification` (the enabled path, so the flip cannot pass silently)
and `test_signature_config_default`; `cargo fmt --all -- --check` clean. The safety argument for flipping
the value is the caller measurement above: `image.rs:54` writes `self.verify_image(&image)?;` - it keeps the
error and discards the flag.

**A reading mistake of my own, recorded.** The first owner-target run printed
`0 passed ... filtered out`, and I briefly took that as "the module's tests ran and matched nothing". It
was the opposite: `tail -1` shows only the **last** test target's summary, and the lib target's line had
scrolled past. The corrected run (`--lib`, all summary lines read) shows 105 passing. The lesson matches
one already in this repo's memory - read every `test result` line, never a single one.

---

## SUPPLY-10 / issue #74 (release integrity) - half one: the release publishes checksums

**Found (measured 2026-11-08).** The installer downloads at **four** sites and verifies none of them:
`install.sh:84` (`curl -fsSL -o "$TEMP_DEB" "$DEB_URL"`) then `:87` (`dpkg -i "$TEMP_DEB"`), the binary
fallback at `:100-102` (`curl` -> `chmod +x` -> `mv`), the `podnext-daemon`/`pn-gateway` loop right after
it, and `install-short.sh:21-37`. Behind that, the release itself published **no checksums**: `v0.2.0`'s
assets are `install-short.sh`, `install.sh`, `podnext-aarch64-linux-android`,
`podnext-x86_64-linux-android` and nothing else (no `.sha256`, and no `.deb` either - so the `dpkg` path
targets an asset the release never ships), and `git grep sha256 .github/workflows/` was empty.

**Change (2026-11-08), half one only.** `Combine artifacts` in `.github/workflows/release.yml` now writes
`<asset>.sha256` for each of the four shipped assets from the artefact that is actually shipped, refuses
when one is missing (`ERROR: release-artifacts/<f> is missing; refusing to publish a partial asset set`),
prints the checksums, and refuses when it did not write exactly four. The release step's `files:` list
gained the four `.sha256` paths, so they are uploaded beside the assets.

**Deliberately not done in the same change.** The installer half - a verify-or-refuse helper called at all
four download sites - is not started, because a partial installer edit would leave the other paths
unverified while looking fixed. It is the next block, with its shape recorded: refuse on a mismatched
checksum with no escape hatch; refuse when no checksum is published unless `PDN_ALLOW_UNVERIFIED=1` is set
explicitly (and print that it is installing unverified); verify locally by stubbing **only** `curl` in
`PATH`, never by blanking `PATH`, which would also remove `sha256sum` and make every case look the same.

---

## SUPPLY-10 / issue #74 - half two: the installers verify their downloads

*(Seam note: the paragraph beginning "**Verification.** The workflow's own `run:` block" further down
closes the **half one** entry above, not this one. This section was inserted before it by a later edit; the
order of the file is misleading here and this note marks it rather than leaving a reader to guess.)*

**Change (2026-11-08).** Both installers now carry a `verify_download` helper and call it at every download
site before anything is installed or executed: `install.sh` at the `.deb` download, the `podnext` binary
and the `podnext-daemon`/`pn-gateway` loop; `install-short.sh` at the `.deb` and the binary loop. A
mismatch always refuses with no override; a missing checksum refuses unless `PDN_ALLOW_UNVERIFIED=1` is
set, in which case it prints that it is installing unverified. `sha256sum` joins the dependency check in
`install.sh` so a host without it fails early and by name. Two other defects found while reading the same
files: `install-short.sh` hardcoded `/tmp` for the `.deb` (`:28`), which does not exist for a Termux app
uid, and both scripts left the `.sha256` file behind on the refusal paths.

**Verification: a fixture harness, one stub.** `curl` is the only stubbed binary; `HOME` and `TMPDIR`
point inside the fixture, so no real install target is touched. Eight cases: positive, tampered checksum
with no override, missing checksum with no override, and missing checksum with `PDN_ALLOW_UNVERIFIED=1`,
for each installer. Measured: positive `rc=0` with `Checksum verified: ...` then `Installing via dpkg...`
and `Installation complete!`; tampered `rc=1` with `hecksum mismatch for ...` and **dpkg never called**;
missing `rc=1` with `no checksum published for ...: refusing to install an unverified download`; override
`rc=0` with `installing UNVERIFIED` (and `dpkg` called, which is the point of the override). Both scripts
pass `bash -n`.

**Five harness bugs of my own, because a control that fails for the wrong reason proves nothing.** Each
looked like "the change is broken": a stub with `#!/bin/bash`, which does not exist on this Android host
(bash is at `$PREFIX/bin/bash`); a stub that wrote to `"$out"` when `install.sh` calls `curl` **without**
`-o` and reads the pipe; `shift` before reading `$1` for the script path; `-` as the "no env vars"
sentinel, which made `env` try to execute a command named `-`; and grep patterns that missed the message
they were looking for (`hecksum` vs `Checksum`, `no checksum` vs `No checksum`), so `rc` alone carried the
verdict. The harness lives at `$PREFIX/tmp/instfix/harness2.sh` and is scratch, not a repo artifact.


**Verification.** The workflow's own `run:` block was extracted from the YAML and executed with `bash` in a
fixture holding four fake assets: `rc=0` and all four `sha256sum -c` report `OK`. Negative control, and it
had to be corrected once: deleting only the combined copy proved nothing (`rc=0`) because the step re-copies
from the per-target directories - removing `install.sh` from **both** sources gives `rc=1` with the refusal
message, and restoring it returns `rc=0` with four `OK`s. A detail stated rather than hidden: when it
refuses, the two checksums written before the missing asset are left on disk; harmless because the step
exits non-zero and nothing is published, but a reader should not have to infer that.

---

## TC-PARITY-01 / issue #76 - re-measured, and narrower than it was written

**What was claimed.** That `install` does not write the `env`/`working_dir` that
`apply_image_runtime_config` reads (`native/meta.rs:99-110`), with the OCI configuration being dropped.

**What measurement shows (2026-11-08).**
- `build` **does** write them: `commands/build.rs:342-344` emits `"cmd"`, `"env"` (as an object) and
  `"working_dir"` into the container's `metadata.json`. So the reader's branches are **not** dead code, and
  a claim I briefly wrote in this session ("nothing in the tree writes the `env` key") was **wrong**.
- `install` does **not**: the OCI/registry path calls `save_container_metadata` (`native/image.rs:555`),
  whose six keys are `name`, `image`, `tag`, `rootfs`, `created_at`, `status` (`:960-983`) - no `env`, no
  working dir. That half of #76 stands.
- The owner test target agrees with that split: `cargo test -p podnext --test image_config_env` ->
  **6 passed, 0 failed**, including `the_image_config_is_persisted_by_a_build` and
  `a_login_delivers_the_images_env_to_the_guest` - both about **build**.

**Why the wrong claim almost shipped, and the rule it re-confirms.** The empty result came from
`git grep -n '"env"\|"working_dir"\|"cmd"'`: `git grep` does not treat `\|` as alternation the way GNU grep
does, so the pattern was searched literally and matched nothing. This is the **second** time in one day
(the other was `git grep -n 'dpkg -i\|dpkg --install'` returning nothing while `install.sh:87` contains that
text). Use `git grep -nE 'a|b'` or separate invocations, and treat an empty result as a question about the
lookup before it becomes a finding about the code.

**Next measurement, not a guess.** Whether the `install` path even fetches the image configuration (a
registry pull may only fetch a layer plus a manifest), which decides whether propagating `env`/`working_dir`
into the metadata is a small wiring change or a feature. That question is what the remaining half of #76
now rests on.

---

## p4 step 1 - the engine reads the syscall number (*771b817*)

**Found.** `Tracee::read_syscall` and `read_retval` returned `0` for every stop, so the 155 handler
patterns in `syscall.rs` could only ever be selected by `0`, and no `PTRACE_GETREGSET` call existed
anywhere in the tree (the `PTRACE_CONT`/`PTRACE_SYSCALL`/`PTRACE_GETREGSET` search of the assessment was
true of those three symbols; `ptrace` itself is used, see `NATIVE_ENGINE_STATE.md`).

**Change (2026-11-08), files touched.**
- `crates/proot-core/src/tracee.rs:110-190` - `read_registers()` reads the register file through
  `PTRACE_GETREGSET` with `NT_PRSTATUS`; `read_syscall`/`read_retval` use it; `observe_syscall()` is public
  so a test can stop a child at a known syscall. Four measured platform details are recorded in the code
  because each one cost a compile cycle: nix 0.27 has no aarch64 `ptrace::getregs`; libc does not export
  `NT_PRSTATUS` for this target (so it is named where it is used, with its ABI source); Android's
  `libc::ptrace` takes `request: c_int` and is variadic; and `Registers` is the aarch64 layout, so on
  x86_64 it is left at its default rather than filled with values that mean something else.
- `crates/proot-core/tests/native_syscall_read.rs` (new) - stops a real child at `getpid` and asserts the
  number the engine reports is `SYS_getpid`, not merely non-zero.

**Verification, both directions on the same assertion.**
- Real read: `cargo test -p proot-core --test native_syscall_read` -> `1 passed`, 
  `the_engine_reports_the_syscall_the_tracee_made ... ok`.
- Read stubbed (API intact): `FAILED`, `left: None right: Some(172)` - naming `SYS_getpid`, so the
  assertion fails for the right reason.
- A first attempt at the control was **invalid** and is recorded as such: reverting all of `tracee.rs`
  produced `error[E0599]: no method named observe_syscall found`, which proved the API had gone, not that
  the assertion can fail. The valid control stubs only the register read.

**What this does not do.** It does not make the engine reachable: `ProotEngine::run()` still has no caller
in any binary, and `syscall.rs`'s handlers still have no dispatch wired to the number this now reads.
Steps 2-3 of the p4 plan are untouched.

---

## Incident: master went CI-red at 2999c9e, and the cause was p4 step 1 (fixed in 163a242)

**What happened.** Pushing the p4 step 1 change turned five checks red on `2999c9e`: `Rust Host Build`,
`Relocated HOME and prefix`, `PRoot Distro Integration Test` (all in run 42449343351), `Run Benchmarks`
(run 42449343311) and `Issue ledger` (run 42449343320). The local gate for the same tree was **green**
(`p4-gate1`, exit 0, 371 s) and `cargo fmt --all -- --check` was clean, which is exactly the gap the
verification lesson names: a local gate says nothing about the targets and environments it does not run.

**Cause, quoted from CI's own log.** `Run Benchmarks`, step "Verify benchmark targets exist and compile":
`error[E0308]: mismatched types --> crates/proot-core/src/tracee.rs:149:17`, on
`nix::libc::PTRACE_GETREGSET as nix::libc::c_int`. Android's libc declares `ptrace` variadic with
`request: c_int`; the Linux x86_64 runner's libc takes `request: c_uint` and is not variadic. One line
compiled on the device and broke every runner job - the other three build-failing checks build the same
crate. `Issue ledger` failed for the same reason and shows it as per-predicate lines: `#25 the benchmark
targets compile (the false-green defect)` FAIL, `#18 the sparse-copy unit test`, `#16 the deep-recursion
test`, `#31 the abnormal-restart test`, `#41 both regression tests`, `#60 the guard is unit tested` - every
predicate that runs tests - and `#42 every documented command and option exists in the real CLI`, which
builds the binary. A workspace that does not compile fails all of them.

**Diagnostic notes worth keeping.** The check-run annotations were empty apart from "Process completed with
exit code 101/1"; `gh run view <id> --log-failed` answered "run ... is still in progress; logs will be
available when it is complete", and the two-stage fetch (read the `Location` from
`gh api -i .../actions/jobs/<id>/logs`, then `curl` it with no auth header) returned no Location at all,
because an unfinished job has no log blob. The ledger's own failure text is a `grep FAIL` over its step
output, not the check summary.

**Fix (2026-11-08, 163a242).** `as _` instead of a concrete integer type, so the parameter's own type
decides, with a comment recording the asymmetry and the run that measured it.

**Verification status.** Local: `cargo fmt --all -- --check` clean and
`cargo test -p proot-core --test native_syscall_read` -> 1 passed. CI: 13 checks for `163a242`, all
pending except one, **zero failures** at the time of writing - so the fix is **not confirmed** yet, and
this entry will be wrong if those checks do not go green. The previous run for `2999c9e` is now complete
and its log matches what is quoted above.

**Rule this incident adds.** Every commit that touches `crates/**` must have its CI verdict read, not only
its local gate: the device builds one libc and the runner another, and this is the second time in this
session that a green local result hid a red reality.













## p4 step 3a: the engine owns its tracee, and the execve translation is blocked by a measurement

**Finding (2026-11-08 14:58-15:20 WIB, master 19df16b).** "Run containers with podnext's own binary"
cannot be reached by wiring: `ProotEngine::run()` (`crates/proot-core/src/lib.rs:173-184`) installs
the seccomp filter and enters `run_loop` without ever spawning a child, so it can never see a tracee,
while `ProotEngine::start()` (`:187-193`) spawns the EXTERNAL `proot` binary with six argv features
(`--kill-on-exit --link2symlink --sysvipc -L --change-id=0:0:0 --rootfs=.`). The first increment of
that subsystem was therefore built as a new module rather than as a change to the existing engine.

**Action.** New module `crates/proot-core/src/native_run.rs` (560 lines) registered at
`crates/proot-core/src/lib.rs:24` (`pub mod native_run;`), with a new test target
`crates/proot-core/tests/native_exec_translation.rs` (327 lines). The module forks a child with
`PTRACE_TRACEME`, traces syscall stops with `PTRACE_SYSCALL`, and reports what it did NOT handle as a
named refusal carrying a reason; `strict` mode stops the run at the first such syscall instead of
implying parity. Which binary the kernel executed is read from `/proc/<pid>/exe` at the
`PTRACE_EVENT_EXEC` stop rather than inferred from an exit code. The guest path string lives in a
page mapped `MAP_SHARED` before the fork, and the CHILD publishes its own pointer to it in a header
inside that page, because measurement showed the engine's address for that page is not the child's.

**Measurements this increment produced (device, 2026-11-08).**
- `cargo test -p proot-core --test native_exec_translation` -> `1 passed; 0 failed; 1 ignored`.
- The proven run reports: `entries=1172` syscalls traced, `refusals=1172` (each with a name or a
  number plus a reason), `child_view=Some(ChildView { path_ptr: 524454457344, path_len: 9 })`, and
  `executed=Some("/data/data/com.termux/files/usr/bin/coreutils")` - a HOST path, which is exactly
  the false positive the test file exists to catch.
- `cargo test -p proot-core --lib` -> `13 passed; 0 failed`; `cargo fmt --all -- --check` clean;
  `python3 scripts/check_non_goals.py` -> "no non-goal was crossed".

**Blocker, kept runnable rather than described.** `the_engine_translates_execve_into_the_given_rootfs`
is `#[ignore]`d with the measurement in its reason: the engine's first entry stop is `48`
(`faccessat`), and the child's own `getpid` sync marker appears later, at position 9, carrying the
child's pid (21375 for the run whose fixture directory was `p4-3a-run-21373`). So the trace begins
AFTER an `execve` the engine never saw an entry stop for, and the path the engine prepared is not the
path being executed: arm A therefore runs the host's `/bin/true` (Termux maps `/bin` to
`$PREFIX/bin`, hence the `coreutils` path) and exits 0. The first `execve` the engine does observe, at
position 7 of that run, carries a path pointer that is neither the child's published pointer nor
page-aligned. Un-ignoring that test is the acceptance criterion for the fix; it must not be deleted or
weakened.

**Rule this increment adds.** An engine that owns a tracee must verify WHERE ITS TRACE BEGAN before it
trusts any syscall it observed: a sync marker emitted by the child immediately before the syscall of
interest must be the first entry stop, asserted as a gate. Without that assertion the engine can
believe it intercepted an `execve` while the kernel was running a host binary, and the resulting report
- refusal counts, "executed" evidence, strict failures - reads as a working capability.

## p4 step 3b: the trace now begins before `execve`, and the guest path is actually translated

**Finding (2026-11-08, device; measured by a delegated builder at 17:01-17:07 WIB - heavy logs
`20261108T100116Z-native-sabotage.log`, `20261108T100520Z-native-final-nocap.log`,
`20261108T100720Z-native-final2.log` - and re-verified by the parent in its own run).** Step 3a's
translation never happened for TWO independent reasons, each measured, not inferred:

1. **`PTRACE_TRACEME`'s initial stop is delivered AT the child's `execve`**, not before it. The trace
   therefore began on the STARTUP of an already-exec'd program (first entry stops `48 faccessat`,
   `56 openat`, `63 read`), and the child's own markers plus the `execve` whose path had to be
   rewritten were never observed at all. Fix: the child's FIRST syscall is a blocking `read(pipe_r)`;
   the engine `PTRACE_SEIZE`s it (its options in force from that first stop), `PTRACE_INTERRUPT`s it,
   consumes the `PTRACE_EVENT_STOP` group-stop, and only then releases the child with one byte. No
   `SIGSTOP`, no fork race. `PTRACE_SEIZE`/`PTRACE_INTERRUPT` are NOT declared by Android's libc and
   nix 0.27's wrappers are `#[cfg(target_os = "linux")]`, so both go through `libc::ptrace` with
   locally named request numbers 0x4206/0x4207, cast `as _`.
2. **`LD_PRELOAD=libtermux-exec-ld-preload.so` intercepts `libc::execve`**, so the child's exec never
   surfaced as a syscall stop (the engine saw no `SYS_execve` entry at all, and arm A ran the host
   binary and exited 0). A/B proof with the same bionic `libc::execve`: with the preload in force
   there is no `execve` stop; with `env -u LD_PRELOAD` the entry stop appears and the fixture runs.
   Fix: the child issues the exec as a raw `syscall(SYS_execve, ...)`, which bypasses the interposer.
   The marker became raw too (`syscall(SYS_getpid)`) because bionic caches `libc::getpid`.
   Also dropped: the invalid `syscall(-1)` marker added in step 3a - it raises `SIGSYS`, and the
   resulting signal-delivery stop poisons the following trace; and `at_entry` is reset to `false`
   after `PTRACE_EVENT_EXEC`, because `PTRACE_GET_SYSCALL_INFO` showed the next stop is the execve's
   syscall-EXIT.

**Finding that the fixture itself was vacuous (reported as a scope deviation, and it matters).**
Acceptance required arm A to exit 1 and arm B to exit 0, which was unachievable with the original
fixture on this device: Termux's `$PREFIX/bin/true` and `false` are symlinks to ONE `coreutils`
multicall that dispatches on `argv[0]` (measured: `exec -a /bin/true <copy-of-false>` exits 0), so
both arms were the SAME binary and the comparison proved nothing. The fixture now compiles
`int main(void) { return N; }` with `$CC`/`cc` when the two host applets resolve to one file, so the
arms genuinely differ; a failed compile PANICS rather than skipping, and each test gets its own
fixture directory because the two tests run concurrently in one process and their cleanup raced.

**Verification (the parent's own run, not the builder's report).**
`cargo test -p proot-core --test native_exec_translation` -> `2 passed; 0 failed; 0 ignored`,
exit 0, 10 s, peak 185 MB. Raw lines: `[arm-a] exit=1
translations=[Translation { guest: "/bin/true", host: "<fixture>/rootfs-a/bin/true",
syscall: "SYS_execve" }] executed=Some("<fixture>/rootfs-a/bin/true")`; `[arm-b] exit=0` with
`host=<fixture>/rootfs-b/bin/true`; `[control-c] translated path does not exist: /bin/true ->
<fixture>/rootfs-c/bin/true (this engine does not fall back to the host filesystem)`;
`[control-d] exit=127 translations=[] executed=None`; the recorded prefix shows the child's markers
then `execve` with `arg0 == the child's published path_ptr`. Assertions went 21 -> 25 and `#[ignore]`
2 -> 0, so nothing was weakened. The negative control (the rewrite made a no-op) went RED with
`arm A ran a copy of false, so a correct run exits 1 ... left: 0 right: 1`, restored -> green.
`cargo test -p proot-core --lib` -> 13 passed; `cargo fmt --all -- --check` clean;
`python3 scripts/check_non_goals.py` -> no non-goal crossed.

**Rule this adds.** A tracer's first stop is not necessarily its first event: with `PTRACE_TRACEME`
the initial stop is delivered AT `execve`, so a trace can begin on the far side of the very syscall
you meant to rewrite and still look healthy (refusal counts, an `executed` path, strict failures).
Assert WHERE the trace began. And on Termux, an `LD_PRELOAD` interposer can hide a syscall from
ptrace entirely - issue raw syscalls when the trace must see them.

**Files changed.** `crates/proot-core/src/native_run.rs` (module docs 11-47; `TRACE_OPTIONS` and the
SEIZE/INTERRUPT constants 97-114; `run_native` pipe+fork 288-306; `child_main` 310-392; `parent_loop`
406-478, 495-501, 569-573; `read_child_view` 598-616) and
`crates/proot-core/tests/native_exec_translation.rs` (docs 11-40; fixture helpers 58-168;
`#[ignore]` removed at 246; sync gate 280-328). Every change carries a dated
`2026-11-08, p4 step 3b` comment in the source.

## p1b: does `podnext kill` still claim a session it did not signal? (re-measured, ASEC3-PROC2-01)

**Finding that prompted this (recorded 2026-11-08 earlier in the ASEC campaign).** With a session
record whose pid was dead while a live holder kept the record, `podnext kill <container>` printed
`Container '<n>' killed.`, returned 0, removed the session record - and no signal had been sent. The
first attempted fix (`session_still_active`, commit 6a9cd2a) was WITHDRAWN in bb22ad0 because CI
proved it fired on a test helper that holds the record itself
(`a_guest_ignoring_term_is_escalated_term_quit_kill`, image_config_env.rs:497), and the remedy was
reopened: it must distinguish the guest from any other descriptor holder, not refuse because
"someone holds it".

**What the tree already says.** `crates/proot-distro-core/src/session.rs:24-29` documents the
contract: liveness is decided by the inherited descriptor (`active_sessions()`, C8), while
`(pid, start_time)` only ADDRESS a session - "and let a signal be refused when the pid has since been
recycled (B13, runc's `hasInit()`)". `signal_target()` (`session.rs:113-115`) is that refusal:
`process_is_alive(record.pid, record.start_time).then_some(record.pid)`.

**Action.** Instead of writing a fourth guard, the property was MEASURED. New harness
`scripts/kill_honesty_probe.py` (199 lines) commits the reproduction: it seeds a fixture store
through `scripts/pdn_fixture_guard.py`, proves the binary acts on that fixture before it is allowed to
act at all (`assert_binary_honours_store`), then runs five arms over the same container - pid dead or
live, crossed with the record unheld / held `LOCK_SH` / held `LOCK_EX` (the arm that used to lie is
dead+EX) - and a positive control that must report a real signal. It refuses (exit 2) when the
positive control is silent, because silence elsewhere then proves nothing.

**Measured result at 328e464 (device, 2026-11-08 18:2x WIB).** Verdict line:
`claims not backed by a live pid at claim time: 0`; `positive-control arms that reported a real
signal: 1`. In detail: dead+none and dead+EX both print
`Skipping session PID 26347 (probe-kill): its pid no longer names the process that registered it, so
nothing can be signalled safely there.` with `Container 'probe-kill' is not running.` and NO kill
claim - so the marked recipe is now refused by name; dead+SH and live+SH are silent with no claim;
live+EX reports `Sent SIGTERM to session PID 26337 (probe-kill)` with that pid alive at claim time.
The operator's store was untouched: 10 containers / 0 sessions before and after.

**Ledger.** A new line `ASEC3-PROC2-01-V` (status VALIDATED, validates=ASEC3-PROC2-01) records the
re-measurement; the earlier `-W` CORRECTED line stands as the record of the withdrawal.

**Rule this adds.** A withdrawn fix is not evidence that the defect is open, and a defect that was
real can stop being reachable when the surrounding mechanism changes: re-measure the ORIGINAL recipe
before writing a second guard. And a silence-based probe needs a positive control in the same run -
without an arm that must report, "no claim was printed" is indistinguishable from "the reporter never
ran".
## p4c slice 1+2: the native engine follows forks and honours bind mounts (measured 2026-11-08)

**Findings this closes (from the engine's own declaration of its gaps, native_run.rs lines 49-64 before
this change).** (a) `TRACE_OPTIONS` carried only `TRACESYSGOOD | TRACEEXEC`, so a grandchild produced by
`fork()`/`clone()` was never traced and its `execve` was never translated: any workload that runs a
shell (which forks for every command) silently escaped translation. (b) `pub fn resolve(rootfs, guest,
binds)` honoured a bind list that NO caller ever filled, so bind mounts were inert - the CLI's
`--bind` had no effect on the native path. Both gaps were declared in prose, which is why neither was
hidden; but a declared gap is still a gap.

**Action taken.** Slice 1: `TRACE_OPTIONS` now unions `PTRACE_O_TRACEFORK | PTRACE_O_TRACEVFORK |
PTRACE_O_TRACECLONE` (native_run.rs:114-119) and `parent_loop` was rebuilt as a per-tracee slot table
driven by `waitpid(-1, __WALL | __WNOTHREAD)` (native_run.rs:410-748): it reads the new pid from
`PTRACE_GETEVENTMSG` on the fork event, consumes that tracee's birth stop, resumes it with
`PTRACE_SYSCALL`, tracks entry/exit and the exec event per tracee, and returns only when every slot
is gone. Slice 2: `RunPlan` carries a bind list (native_run.rs:145-146) and `resolve`
(native_run.rs:1068-1084) now selects the LONGEST matching prefix instead of the first in list order,
with the rootfs fallback unchanged. Translation for a tracee that is not the original child goes
through `translate_execve_remote` (native_run.rs:823-1065: `PTRACE_GETREGSET`/`SETREGSET`, `PEEKDATA`
to read the guest path, `POKEDATA` to write the translated path into a stack scratch at `sp-0x200`,
and arg0 redirect), because the shared-page trick only works for the child the engine forked itself.

**Files.** `crates/proot-core/src/native_run.rs` (+598/-115), `crates/proot-core/tests/
native_exec_translation.rs` (+188, zero deletions). The module's own doc comment (lines 49-64) was
rewritten from "binds and forks are not done" to what now works, so the prose cannot drift from the code.

**Verification - the parent re-ran it, it is not a child self-report.**
`cargo test -p proot-core --test native_exec_translation -- --nocapture` ->
`test result: ok. 4 passed; 0 failed; 0 ignored` (exit 0, 96 s, 83 MB peak), with the engine's own report
lines: `[fork] exit=1 translations=[{guest:"/bin/forker"}, {guest:"/bin/child", host:"<fixture>/
rootfs-fork/bin/child"}] executed=Some("<fixture>/rootfs-fork/bin/child")` and
`[bind] exit=7 translations=[{guest:"/bin/forker", host:"<fixture>/rootfs-bind/bin/forker"},
{guest:"/opt/tool", host:"<fixture>/bind-target/tool"}] executed=Some("<fixture>/bind-target/tool")`.
`cargo test -p proot-core --lib` -> `13 passed; 0 failed`. `cargo fmt -p proot-core -- --check` clean.
Assertions 26 -> 32, `#[ignore]` 0 (was 2 before p4-3b).

**Negative controls (child, pasted in its transcript, and required by the brief).** With bind
resolution sabotaged the bind test fails on the assertion
`the bind must supply the guest path, but the engine refused the run: translated path does not exist:
/opt/tool -> <fixture>/rootfs-bind/opt/tool`; with the fork options removed the fork test fails on
`the forked child exec'd the rootfs /bin/child (exit 1) ...; 90 means the child's execve was never
translated and failed`. Both are assertions about behaviour, not compile errors; both restored green.

**Two real defects the implementer hit and fixed (recorded because they will bite the next tracer).**
`waitpid(-1)` cross-reaped children of OTHER tests running concurrently in the same process (`ESRCH`);
`__WALL | __WNOTHREAD` confines it to the calling thread's children. And a tracee killed while
ptrace-stopped leaves a zombie that the next `waitpid(-1)` returns, so `Exited`/`Signaled` for unknown
pids must be ignored - a blocking post-loop `waitpid(child)` deadlocks instead, because the forker
waits on a child ptrace will not let die.

**Limits stated, not hidden.** The stack scratch at `sp-0x200` fits `PATH_CAPACITY+1`; a tracee whose
SP is within 0x200 of the bottom of its stack mapping makes `POKEDATA` fail and the run ERRORS rather
than corrupting memory (not exercised by the fixtures). A fork-descendant's `at_entry` starts as
`false` because the kernel reports the clone-exit first; this affects only the `entries`/refusal labels,
because entry-versus-exit for translation is decided by whether the guest path is readable
(`peek_cstr` succeeding), so a wrong guess cannot cause a missed translation. `resolve`'s bind match is
still plain `starts_with` (no path-component boundary), preserving the pre-existing semantics - only
first-match became longest-match. Production callers still do NOT use this engine; it remains
experimental until parity is measured on the same load.## p5 slice-1: the daemon's socket IPC bounded and named (ASEC2-API-005/006/007/008, measured 2026-11-08)

**Findings.** Four recorded items on one mechanism - the line-oriented JSON control protocol the CLI and
the dashboard both speak over a unix socket. Measured at the audit commit: the socket inherited the
ambient umask (`mode 0o777` under `umask 0000`) because `run()` called `UnixListener::bind` with no
`set_permissions`; startup unlinked ANY existing socket path unconditionally, so a second daemon silently
stole a live daemon's socket; `read_line` on an uncapped `BufReader` accepted a 10,485,784-byte frame and
answered it; and there was no cap on concurrent connections (128/128 served) and no read deadline (an
unterminated frame pinned a handler until the peer chose to finish).

**Action.** One mechanism, four fixes in `crates/podnext/src/daemon.rs` (+389/-12), with the numbers named
as constants rather than scattered literals (lines 30-52): `MAX_FRAME_BYTES = 64 * 1024`,
`READ_DEADLINE = 2 s`, `MAX_CONCURRENT_CONNECTIONS = 64`, `SOCKET_MODE = 0o600`,
`LIVE_PROBE_TIMEOUT = 500 ms`, plus stable refusal tokens `FRAME_TOO_LARGE`, `READ_DEADLINE_EXCEEDED`,
`SERVER_AT_CAPACITY` so tests assert the token, not a sentence. -005: `perms.set_mode(0o600)` +
`set_permissions` immediately after bind (line 330), imitating what `daemon_runtime` already did. -006:
before binding, an existing path is probed by connect + ping (`daemon_is_live`, lines 412-438); a live
answer refuses startup by name (line 264, quoting the socket and how it decided), a genuinely stale path
is reclaimed. -007: each frame is read through `(&mut reader).take(MAX_FRAME_BYTES)` (lines 440-489) and
an over-cap frame is refused by name and the connection closed, because an over-cap frame cannot be
resynchronised. -008: `Arc<Semaphore>(64)` with `try_acquire_owned` (lines 341-395) refuses the 65th
connection by name instead of queueing it, and every read is wrapped in `timeout(READ_DEADLINE)`.

**Verification - the probe's own artifacts, read by the parent, BEFORE vs AFTER.**
`workbench/asec/evidence/asec2-api/probe_results.{BEFORE,AFTER}-fix.json`:
`socket_stat.mode` `0o777` -> `0o600` (both runs under `umask 0000`, so the mode is explicit);
`bigline_10MiB.resp` `{"type":"pong"}` -> `{"type":"error","message":"frame too large: exceeds the
65536-byte frame limit"}`; `bigline_10MiB_garbage` now named as a frame refusal instead of a JSON syntax
error, so the two are distinguishable; `second_daemon_on_live_socket.state` `still_running_no_refusal` ->
`exited` while `live_daemon_after_second.resp` `<connect failed: [Errno 2]>` -> `{"type":"pong"}`, i.e.
both halves of -006; `stale_socket_daemon.bound true` with `ping_resp pong` (stale path still reclaimed);
`conn_cap_boundary.over_cap_resp` `{"type":"pong"}` -> `{"type":"error","message":"server at capacity: 64
concurrent connections in use"}`; `read_deadline.waited_s` `8.0` -> `2.0`;
`partial_no_newline.early_recv` `<TIMEOUT>` -> `{"type":"error","message":"read deadline: no complete
frame within 2s"}`. No regression in `ping`, `list`, `start_fixture_svc`, `load_fixture_svc`, `malformed`.

**Parent's own runs on the integrated tree (not the child's numbers).**
`cargo test -p podnext --bin podnext-daemon -- --test-threads=2` -> `test result: ok. 8 passed; 0 failed`
(2 pre-existing + 6 contract tests at daemon.rs:811-947); `cargo test -p podnext --bin podnext --
--test-threads=2` -> `52 passed; 0 failed`; `cargo fmt --all -- --check` clean. Negative control (child,
required by the brief): all four sabotaged at once produced five assertion failures that name the
behaviour - `socket mode must be the explicit 0600, got 700`, `a second daemon neither refused nor exited:
it started`, `oversized frame must be refused by name, got pong`, `the connection over the cap must be
refused by name, got pong`, `daemon did not close an idle connection within the deadline` - then restored
green. Assertions about behaviour, not compile errors.

**Behaviour change stated plainly.** `READ_DEADLINE` is a PER-READ deadline, so a client that writes its
frame more than 2 s after connecting is now closed (`partial_completed` went from `pong` to
`<closed: Broken pipe>` in the probe). That is the intended consequence of bounding a hostile peer; the
CLI opens one connection per command and writes immediately, and the dashboard is local, so neither is
affected - but a slow third-party client would be. The per-connection cap means a legitimate burst above
64 simultaneous connections is refused rather than queued.

**Limits.** `SOCKET_MODE` restricts the socket itself; the parent directory stays umask-derived (the
finding names the socket). `flood_128_concurrent` still reports 128/128 because those connections are
short-lived and permits free faster than the accept loop saturates - the cap is proven deterministically
by the new boundary arm (64 held silent connections, the 65th refused by name) instead.

**Process note.** Both this change and the concurrent native-engine change were developed in the SAME
working tree rather than separate worktrees (the briefs did not require isolation). They stayed
file-disjoint so nothing was lost, but that is a real protocol deviation: two children in one tree can
observe each other's half-applied state, and the parent should have required `git worktree add` per
child as the delegation protocol says.## p5 slice-2: a Dockerfile's WORKDIR is confined to the image rootfs (ASEc-001, ASEC1-A07-001, -C1)

**Findings.** `podnext build local` applied a Dockerfile's `WORKDIR` without confining it to the image
rootfs. Measured at the audit commit: `FROM scratch` + `WORKDIR ../../etc` exited 0 and created
`<store>/containers/etc`, a sibling of the image directory (`ASEC1-A07-001`), and the parent's own run
created a directory outside both the rootfs and the store (`ASEc-001`). A 1000x chained `../..` form
escaped the fixture entirely, leaving `<fixture>/etc`, `<fixture>/.podnext/etc` and
`<fixture>/.podnext/containers/etc` behind. `ASEC1-A07-001-C1` corrected the landing path for the
ABSOLUTE form.

**Where the write actually happens (this mattered).** `build.rs:276` only RECORDS the value
(`c.config.working_dir = Some(wd.clone())`). The writer is the `create_dir_all(rootfs.join(abs))` in the
same instruction arm. The guard therefore had to sit before that call, not around the record - and the
re-measurement confirmed both landing paths, reconciling the two ledger rows rather than picking one:
the RELATIVE form `../../etc` lands at `<store>/containers/etc` (the original row), while the ABSOLUTE
form `/a/../../b` lands at `<store>/containers/t1/b`, one level above rootfs inside the container
directory (the correction `-C1`). Both are now asserted in the same test file.

**Action.** `crates/podnext/src/commands/build.rs` +12 at 252-263: a `..` path component is refused
before any filesystem write, with the message `WORKDIR '<dir>' escapes the image root: '..' is not
allowed in a WORKDIR path`. The decision was REFUSE, not normalise, for two reasons: `copy_into_rootfs`
(`build.rs:369`, refusal at `:387`) already enforces exactly this rule and message shape, so a reader
learns one rule instead of two; and refusing needs no model of Docker's normalisation to be safe. New
test target `crates/podnext/tests/build_workdir_escape.rs` (245 lines, 13 assertions, 4 tests) drives the
real CLI: the escape is refused, the 1000x chained form cannot leave the store, and two positive controls
assert the landing path and the recorded `working_dir` for `/srv/app` and for the relative-chained
`/app` then `src` -> `/app/src`.

**Verification - the parent re-ran it on the worktree tree, it is not a child self-report.**
`cargo test -p podnext --bin podnext` and the two test targets were run by the parent with a PRIVATE
`CARGO_TARGET_DIR` inside the worktree (`wt-workdir/target`), because a shared target directory can serve
a sibling worktree's artefact. The child's own pasted evidence: BEFORE (pre-fix binary, the ledger's
recipe) `build exit status: 0` with `ESCAPED: <fixture>/home/.podnext/containers/etc`; AFTER `build exit
status: 1` plus `Error: WORKDIR '../../etc' escapes the image root: '..' is not allowed in a WORKDIR
path`, and the three candidate landing paths outside the rootfs all `absent`. Positive controls in the
same run: `/srv/app` -> `<rootfs>/srv/app` with `"working_dir": "/srv/app"`; `/app` then `src` ->
`<rootfs>/app/src` with `"working_dir": "/app/src"`. Negative control (guard made a no-op): two tests
FAILED on the landing-path assertion (`a WORKDIR with '..' escaped the image root; build exit=Some(0)`,
`/.../home/.podnext/containers/etc exists=true`), then restored and green - an assertion about the
landing path, not a compile error.

**Gates.** `cargo test -p podnext --bin podnext` -> 52 passed; `--test build_workdir_escape` -> 4 passed;
`--test build_copy_escape` -> 2 passed (COPY/ADD behaviour unchanged); `cargo fmt --all -- --check`
clean. All work ran through the memory guard (peak 1546 MB, exit 0).

**Isolation.** The child worked on its own branch in its own worktree (`git worktree add` +
`dept/workdir`, commit 76e6e30, never pushed); the parent verified `git status` of the main checkout is
clean and that the diff touches only the two files above. This is the delegation protocol applied
properly, after the previous wave's two children shared one tree.

**Environment note, recorded because it cost the child time.** A sibling probe in another session
exported `HOME` for a fixture and that export leaked into the SHARED terminal environment, so the
child's `~` briefly resolved to a fixture directory. The child re-pinned `HOME` and used absolute paths;
nothing was derived from the hijacked value and `~/.podnext` was never touched. The rule now in
`hermes-harness`: pass fixture variables per command, never `export` them.
## p6 slice-1: a refused archive member is now named and counted, and the extractor's defences are under test (A04-01..04, ASEC1-A07-007)

**Findings.** Four audit rows measured `extract()` in `crates/proot-distro-core/src/untar.rs` against a
hostile hand-built tar. The extractor already DEFENDED itself - nothing escaped - but the traversal arm
skipped the escaping members SILENTLY: the loop hit `is_inside() == false` and `continue`d, so
`extract()` returned `Ok(())` with no record at all. A refused write that leaves no trace is a security
event the operator cannot see, and it makes "archive contained nothing" indistinguishable from "archive
contained three members we refused". The other three rows were already correct: a planted-symlink write
was refused by name, `strip` matched the reference for absolute and doubled-slash names, and a whiteout
over a symlink removed the link while its target survived. `ASEC1-A07-007` covered malformed archives
(truncated tar, gzip-not-a-tar, truncated deflate), which fail with a named error.

**Action.** `crates/proot-distro-core/src/untar.rs:58-102` adds `Refusal { path, reason }` (its `Display`
renders `refusing entry \`<name>\`: <reason>`) and `ExtractReport { refused }` with `refused()`,
`refusal_count()` and `is_clean()`. `extract`, `extract_with_strip`, `extract_entries` and
`extract_entries_stripped` now return `Result<ExtractReport>` (lines 113, 125, 131, 136); both refusal
sites call `report.refuse(...)` before continuing (lines 167, 183) and the success path returns
`Ok(report)` (line 258). Call sites whose signature changed were updated as a consequence:
`archive.rs:127,266`, `compress.rs:171,176,181,186`, and `pn-next/src/backup_restore/mod.rs:1027`
(`.map(|_| ())`). Tests in the same file now cover traversal-by-name-and-count (814), a clean archive
reporting zero refusals as the positive control (859), strip 0 vs 1 for absolute and doubled-slash
members (888), truncated tar (925), gzip-that-is-not-a-tar (955), truncated deflate (987), declared
4 GiB (1028), the planted-symlink refusal asserted through the report (643), and the whiteout over a
symlink retained (591). Assertions in `untar.rs`: 24 -> 56.

**Verification - the parent re-ran the suite on the worktree tree.** `cargo test -p proot-distro-core
--lib` -> `112 passed; 0 failed`; `--test deep_fs_recursion` -> `3 passed; 0 failed`; `cargo check -p
pn-next` exit 0; `cargo fmt --all -- --check` clean. The child's own BEFORE/AFTER on the audit's own
fixtures: BEFORE `result=Ok` with the escaping members silently skipped; AFTER `result=Ok
refusal_count=3` with three named refusals (`refusing entry \`../escape-target\`: resolved path ...
escapes destination ...`) and `escape-target/escape2 exists=false`; the planted-symlink arm reports
`refusal_count=1` with its named reason. Negative control (the `refuse` method sabotaged to a no-op):
the traversal test FAILS on `refusals: []` with `left: 0, right: 3` - an assertion about the missing
record, not a compile error - then restored green.

**Limits, stated rather than implied.** (a) "No partial tree" is NOT guaranteed at the extractor level:
`extract` writes incrementally, so a tar cut mid-entry leaves a short file; the full guarantee lives in
the install path's staging directory, which is a different component. The new tests assert extraction
fails loudly and that the cut entry did not fully materialise, plus an empty destination for the
gzip-not-a-tar case - which is what the extractor can actually promise. (b) `archive.rs:200` is
`Vec::with_capacity(size as usize)` for a member named `index.json`, so a hostile 4 GiB-declared
`index.json` WOULD allocate; that is outside this row family (the measured case used a different member
name) and was noted rather than widened into this diff. (c) `pn-next/src/backup_restore/mod.rs:1027`
discards the report with `.map(|_| ())`; the refusals still happen (so the security property holds) but
the restore path cannot see them - a follow-up should surface `!is_clean()` there.

**Isolation.** Own worktree and branch (`dept/untar` @ 8ff1a3c), never pushed, main checkout untouched by
the child; the parent verified the branch, the commit, the four files and the test names before
integrating.
## p7 step 1: the engine translates the dynamic loader's path syscalls, so a real guest starts (2026-11-08)

**Finding.** The in-tree engine could only prove STATIC fixtures. Every real guest is dynamically linked,
and after `execve` the ELF interpreter runs inside the guest and opens the shared libraries by ABSOLUTE
path (`/system/bin/linker64`, `<prefix>/lib/libc.so`, and whatever DT_RUNPATH names) through
`openat`/`newfstatat`/`readlinkat`/`faccessat`. The engine translated only `execve`, so those opens went
to the HOST filesystem: either the guest failed to link, or - worse - it silently ran against the host's
libraries and appeared to work. The module's own doc named this gap: "argument translation for any syscall
other than `execve`".

**Action.** `crates/proot-core/src/native_run.rs` (+351/-52) adds a path-syscall dispatcher and entry-time
translation. `path_syscall` (lines 262-310) maps a syscall number to its name and path-argument index:
`openat`, `openat2`, `newfstatat`, `statx`, `readlinkat`, `faccessat`, `faccessat2`, `execve` on every
target, plus the legacy `open`, `stat`, `lstat`, `readlink`, `access` where the target declares them
(x86_64 only; arm64 has no legacy table). `SYS_NEWFSTATAT` is declared locally (245-253) because libc
omits it for android/aarch64 - found by compile error, not by grep. The parent loop's entry branch
(761-845) calls `translate_path_remote` (1190-1250: PEEKDATA the path, `resolve`, write the translated
path into a stack scratch, SETREGSET) after `write_path_and_redirect_arg` was folded in as the shared
helper (1132-1185). A path-taking syscall with no translator is a NAMED refusal
(`unhandled_path_syscall`, 313-331), and `syscall_name` consults both tables so names cannot drift from
the dispatcher (338-349). Relative paths (`dirfd`/cwd) are refused by name because neither is modelled.

**Two real defects found while doing it.** `resolve` re-translated a path that was already on the host
side (the loader `stat`s `/proc/self/exe`, which produced `<rootfs><rootfs>/...`, a path existing
nowhere), and it matched bind prefixes as raw strings, so a bind on `/sys` swallowed
`/system/lib64/libnetd_client.so` into `/sys/tem/lib64/...`. Both are fixed: an already-translated
short-circuit, and component-wise prefix matching (`Path::starts_with`).

**Verification - the parent re-ran the suites on the merged tree.** New test
`crates/proot-core/tests/native_dynamic_guest.rs` (404 lines, 18 assertions) builds a dynamic guest whose
`printf` depends on a library that exists ONLY inside the rootfs (a literal `return 7` would be vacuous on
this device, because the host's `/system/lib64/libc.so` satisfies an untranslated loader - the child said
so rather than shipping a test that cannot fail). Committed tree: `[dyn] exit=7 stops=669 entries=335
executed=Some(".../rootfs/bin/hello")`, `guest stdout="hello from guest"`, translated names
`["SYS_execve","SYS_faccessat","SYS_newfstatat","SYS_openat","SYS_readlinkat"]`, and the loader's
rootfs-only RUNPATH `/p7loader/lib` resolved through `SYS_openat`. Pre-change engine on the SAME test:
`CANNOT LINK EXECUTABLE "/bin/hello": library "libpdnflag.so" not found: needed by main executable`, with
`SYS_openat (56) [passed through, not emulated]` x45 in the refuse list. Negative control (openat
translation made a no-op): the guest's stdout is empty and the test fails on
`the guest's printf must appear on the inherited stdout; got ""`, then restored green. Existing suite:
`native_exec_translation` 4 passed, `--lib` 13 passed, and the full proot-core sweep (argv_facts 2,
identity_table 4, native_dispatch 2, native_syscall_read 1, seccomp_filter 1, selective_seccomp 3) green.
`cargo fmt --all -- --check` clean.

**Limits, stated rather than implied.** (a) PT_INTERP cannot be translated: the kernel opens the
interpreter before the tracee executes an instruction, so the guest's interpreter must exist at its
absolute host path (`/system/bin/linker64` on Android). Only the loader the interpreter then runs is
traced - which is what this change covers, and it is the honest boundary of a ptrace-based engine.
(b) `dirfd`/cwd-relative paths are refused, not translated. (c) The x86_64-only arms are cfg-gated and
compile-checked on this aarch64 device only; CI is the authority for them.

## p6 slice-2: a test target no longer resolves the operator's real store (ASEC4-HARNESS-001)

**Finding (mine, recorded after the WORKDIR verification).** `crates/podnext/tests/automation_contract.rs`
spawned the real `podnext` binary with no `PN_TEST_ROOT`/`HOME` redirection while running `remove`,
`kill`, `volume create` and `volume rm` - so it resolved `~/.podnext`, the operator's live store. Six of
its neighbouring test files already redirect the store; this one did not, and CI cannot catch it because
runners have no `~/.podnext` to damage.

**Action.** `crates/podnext/tests/automation_contract.rs` (+235/-37): a per-test `Fixture` under
`$PREFIX/tmp` with a `temp_base()` that never falls back to bare `/tmp`; `command_in()` sets `HOME`,
`PN_TEST_ROOT`, `TERMUX__HOME` and `TMPDIR` and clears the XDG variables; and a PRE-FLIGHT GUARD
(`assert_binary_honours_store`, lines 142-177) that points `HOME` at a separate DECOY store and requires a
read-only `list` to return the fixture's sentinel - so a binary that ignored `PN_TEST_ROOT` lists the decoy
and the suite refuses before any mutating arm runs. All six tests call the guard first, and
`test_yes_flag_on_volume_rm` additionally asserts the volume mountpoint appears under the fixture store
after `create` and is gone after `rm`.

**Verification.** Parent's runs on the merged tree are part of the same wave; the child's pasted evidence:
`cargo test -p podnext --test automation_contract` -> `ok. 6 passed; 0 failed`, with the guard printing
`fixture guard OK: PN_TEST_ROOT=.../pdn-auto-volrm-NY1cbc/home listed exactly ["pdn-guard-sentinel"];
decoy HOME=.../decoy was not consulted` and the volume landing in the fixture store. The real store is
byte-identical across the run: `1794090579 ... entries=8` BEFORE and AFTER. Negative control (drop
`PN_TEST_ROOT` from the child env): the guard FAILS naming what it saw -
`It listed ["pdn-decoy-9666"], i.e. it resolved HOME=.../decoy instead. A binary that ignores
PN_TEST_ROOT acts on the real ~/.podnext, so this suite refuses.` - then restored green.

**A finding CORRECTED, not patched.** The same ledger row named
`crates/proot-distro-core/tests/single_writer_races.rs` as unguarded. It was read in full and does not
touch the store: every root is an explicit `tempfile::tempdir()` (`BlobStore::at`, `scan_store`,
`staging_units`) and the session registry is redirected via `RUNTIME_DIR` (`session.rs:35-42` returns
`RUNTIME_DIR/sessions` before `resolve_home`). Adding a guard there would have been decoration, so none
was added and the file is untouched (`git diff` = 0 lines). The finding stands for
`automation_contract.rs` only; the correction is written into the ledger rather than silently dropped.

## The operator's acceptance criterion, as a runnable gate

`scripts/own_engine_acceptance.py` is the goal from workbench/WORKLIST.md section Z expressed as a check:
a container must run with `PROOT_BIN=/nonexistent/proot` and no `proot` on `PATH`. It builds a REAL
dynamic guest in the fixture rootfs (Termux has no static toolchain - `cc -static` fails with
`cannot open crtbegin_static.o`), copies the interpreter and every NEEDED library to the same absolute
path inside the rootfs, and notes bionic's file-less `libdl.so` as "implied" rather than dropping it
silently. Measured on this tree: `rc=1`, `Error: No such file or directory (os error 2)`, and the script
names proot as the cause; without `PN_TEST_ROOT` it REFUSES with rc=2. When p7 is done this must flip to
PASS, and the diff of this script is what proves it.## p7 step 6: the in-tree engine is reachable from the CLI, and the operator's criterion flips to PARTIAL (2026-11-08/09)

**Finding.** The engine could run a dynamic guest (commit f215756) but had no production caller: every
container still ran by spawning the host `proot` binary (`proot-core/src/lib.rs:195`, `:337`,
`podnext/src/native/image.rs:131`). The operator's requirement (workbench/asec/REPORT.md section 10,
BUG-PROOT-01; workbench/WORKLIST.md section Z) was therefore unreachable no matter how good the engine
became - the wiring was the missing half.

**Action.** `--engine <proot|native>` as a GLOBAL clap argument with `default_value = "proot"`
(`crates/podnext/src/cli.rs:236-244`, `EngineChoice` in `cli/common.rs:27-39`), threaded through
`main.rs:194-196` and `dispatch.rs` into `cmd_login`/`cmd_run` (`commands/container.rs:261,294,314,
371-382`). `native/session.rs:208,225-234,308-413` adds `run_container_native`, the predicate
`uses_native_engine` (a named function so the routing cannot drift) and `native_verdict`, which turns the
engine's report into an operator-facing error:
`the native engine did not translate N path syscall(s): SYS_chdir (50) [takes a path but this engine has
no translation for it; the kernel resolves it on the HOST filesystem]. ... The engine does NOT fall back
to proot; run under --engine proot for full path coverage.` A refusal is NEVER a silent fallback to
proot. `--engine native` with `--detach` is refused by name (the detached supervisor runs through
`ProotDistroCore::login`). `proot-core/src/native_run.rs:329-338` gains `is_untranslated_path_refusal` so
"path refusal" has one definition. 8 files, +276/-6.

**RunPlan fields: filled and NOT filled, named rather than invented.** Filled: `rootfs` from
`container.rootfs_path`, `binds` from `container.config.bind_mounts` (mapped to the engine's
`(guest_prefix = target, host_path = source)` form), `argv` from the command (defaulting to `/bin/sh`).
NOT filled, because `RunPlan` has no such fields and the engine hands the guest an EMPTY `envp` on
purpose: environment and cwd. Also not modelled on the native path: bind `readonly`, hostname, uid/gid,
kernel release, and the Android/synthetic bind set the proot path renders (`/dev`, `/proc`, `/sys`). All
of it is stated in the doc comment on `run_native_engine`, not left to be discovered.

**Verification - the parent's own acceptance harness, not the child's report.**
`scripts/own_engine_acceptance.py` now runs two arms with `PATH=/nonexistent-bin` and
`PROOT_BIN=/nonexistent/proot`:
  * `[default] exit=1 ran=False` with `Error: No such file or directory (os error 2)` - the default still
    needs proot, which is today's truth and the thing the operator is asking to change;
  * `[engine-native] exit=0 ran=True` printing `guest-ran-inside-the-rootfs` - a dynamically linked guest
    (6112 bytes, DT_RUNPATH `/p7loader/lib`, libraries present only inside the rootfs) RAN with no proot
    binary available to the process. This is the first measured proof that the in-tree engine can carry
    a real container.
The harness prints PARTIAL rather than PASS and names its blockers, because the default path is still
proot: (a) the default is deliberately proot until parity is demonstrated, and (b) PT_INTERP cannot be
translated - the kernel opens the interpreter before the tracee executes an instruction, so a real distro
rootfs whose interpreter is not present at its absolute HOST path will not start under the native engine.
Child's own evidence agrees: `--engine proot` and no-flag argv are identical (all six features plus
`/bin/sh`), the routing test fails on
`` `--engine native` must route to the native runner, not the proot path `` when sabotage makes native
fall through, and `cargo test -p podnext --bin podnext` is 56 passed.

**Harness defect found and fixed while measuring (recorded because it cost two runs).** The fixture first
copied Android's `/system/lib64` libraries next to a Termux-linked binary, and the guest failed to link
(`CANNOT LINK EXECUTABLE "/bin/sh"`). The working recipe - the same one the engine's own test uses - is
to link the guest with a DT_RUNPATH inside the rootfs and place every NEEDED library in THAT directory,
so the guest's loader resolves them through the engine's path translation. The interpreter keeps its
absolute host path. A `linkerconfig` warning remains (`failed to find generated linker configuration from
"/linkerconfig/ld.config.txt"`) and is harmless here, but it is a real gap: the loader's config file is
not part of the fixture rootfs.

**Stale-build trap, third occurrence of its class.** The first acceptance run reported
`unexpected argument '--engine' found` while the binary was in fact built from before the cherry-pick:
cargo answered `Finished in 3.51s` and did NOT relink, because the shared target directory's fingerprint
was stale. Touching the changed sources and rebuilding fixed it (binary mtime moved to 01:23:35 and
`--engine <ENGINE>` appeared in `--help`). Any claim about a CLI surface must be made against a binary
whose mtime is newer than the change - or built into a private target directory.

## p6 slice-3: the warning ratchet no longer passes on a partial measurement (ASEC2-SUPPLY-15)

**Finding.** `scripts/check_warnings.py` counted cargo's per-target warning summaries and compared them
with `scripts/warning_baseline.json`, keyed on host triple plus tool versions. Its live defect: a
PARTIAL measurement passed. A warm run that re-checked one crate of seven reported `208 -> 0` and exit 0
("lower than the baseline"), because a target missing from the measurement read as zero. The docstring,
the audit finding and a CI comment all justified the ratchet with a premise that does NOT reproduce here:
that a warm `cargo check` re-emits nothing. Cargo replays cached diagnostics - the scoped probe
re-checked zero crates yet still emitted four summaries, and two consecutive warm runs both measured 261.
That premise is withdrawn in the docstring with the measurement that refutes it.

**Action.** `scripts/check_warnings.py` (+72/-14): a pure `unseen_targets(measured, base, complete)` and a
`compare(measured, base, complete, source)` that REFUSES an empty or partial measurement with exit 2
("cannot measure") instead of reading it as zero, unchanged growth/equality logic, a `record` guard so a
warm run cannot freeze a too-low ceiling, `mode` optional so a bare invocation defaults to `compare` (it
used to die in argparse with rc 2), and a build-free `self-check` that proves red-on-growth,
quiet-on-equality, red-on-empty, red-on-partial. ANSI handling and the toolchain key were already
correct and are untouched.

**Verification (child's pasted runs; parent integrates and re-runs the gate).** Pre-fix partial 1-of-7
without `--cold` returned rc 0; now rc 2 naming the six unseen targets. Empty log -> rc 2
`.../empty.log holds no cargo summary lines`. Growth 208 -> 209 -> rc 1
`WARNINGS GREW: pn-next (lib): 208 -> 209`. Full+equal -> rc 0. Bare invocation -> rc 0 printing
`measured: 261 warnings (ceiling 266 on this toolchain, cold=False)` plus the per-target deltas.
`record` on a partial measurement -> rc 2 and the baseline file untouched.

**Not demonstrated, stated as such.** The real `--cold` compare path and the `record --cold` write path
were not run (budget); the exact CI sequencing (build-then-warm-check) is not reachable on-device
cheaply. The stale comment in `.github/workflows/issue-ledger.yml:122-125` still asserts the withdrawn
warm-zero premise and was left untouched, flagged here instead.## p7 step 6 follow-up: the help fixture, a global option, and a predicate pinned to commentary (2026-11-09)

**Finding 1 - a global option reflows every screen.** Adding `--engine` with `global = true` and a
three-line doc comment (which clap turns into `long_help`) made clap switch to next-line layout on
**every** screen the option reaches: `research/cli-help-baseline.txt` went 4733 -> 10168 lines for an
eight-line option. The gate caught it (`dump differs from research/cli-help-baseline.txt`). The fix is
not to re-pin a 5,435-line diff nobody can review: the detail moved into a plain `//` comment and the
doc line became single-line, so the option renders compactly again. Re-pinned with the gate's own
generator, after rebuilding: +223 lines, 123 hunks, and filtering the `--engine` lines out of the diff
leaves **zero** changes - the whole diff is the one flag appearing on every screen, which is what
`global` means.

**Finding 2 - a ledger predicate pinned the self-check's wording, not its behaviour.** `#69`'s predicate
ran `check_warnings.py self-check` and looked for `fires on growth`. The self-check passed (rc 0) but now
says `red on growth` and additionally refuses a partial measurement, so the predicate failed on prose
while the property it stands for had got *stronger*. The needle moved with the sentence and a second
predicate asserts the partial refusal ("refuses BOTH an empty and a partial"). A predicate pinned to
commentary is a record of a past decision, not a law.

**Measured state after both fixes.** Gate GREEN, `136 PASS, 0 FAIL, 0 SKIPPED (of 136)`, exit 0, 1073 s,
run in a clean shell. CI on `656f8ea`: 15 runs, 15 success, 0 failures. Commits `0508656` (acceptance
harness: `[engine-native] exit=0 ran=True` with no proot on PATH and `PROOT_BIN` missing, default arm
still failing by design) and `656f8ea` (this follow-up).

**Fixture leak, recorded rather than fixed here.** `crates/proot-core/tests/native_dynamic_guest.rs`
cleans its root at line 177 *before* the test and never after, so each run of `cargo test -p proot-core`
leaves `$PREFIX/tmp/p7-loader-<pid>-dyn/` behind (six were present when this was measured, each with
`rootfs/p7loader/lib/{libc.so,libdl.so,libpdnflag.so}` and `rootfs/bin/hello`). Harmless but unbounded;
the teardown belongs with the test that owns it.## p7 step 7: the two engines compared on the same load (2026-11-09 02:34 WIB)

**Finding.** After step 6 the CLI could reach both engines, but nobody had measured whether they AGREE.
Flipping the default to `native` on the strength of one passing command would have been a claim about
one guest on one rootfs with three of its observables never looked at.

**Action.** `scripts/engine_differential.py` (new, ~250 lines): one fixture rootfs, one guest, the SAME
argv and the SAME bind for both arms, three runs each. The guest reports what the two engines are most
likely to disagree about - `cwd`, a probe environment variable, `/proc/self/exe`, `uid`/`euid` - writes
a file inside the rootfs, prints its argv, and exits 7. Four observables are compared (exit, stdout,
the file effect, per-field values) and wall time is recorded. It uses the shared fixture guard
(`scripts/pdn_fixture_guard.py`), so it refuses without `PN_TEST_ROOT` instead of touching the operator's
store (measured: `~/.podnext` unchanged at 10 containers / 0 sessions).

**Two fixture defects found while measuring, both real:**

1. The proot arm failed with `proot error: '/bin/sh' not found (root = <fixture>)` - the proot path
   resolves its command through a shell inside the rootfs, and the seeded rootfs is empty by design.
   Fixed by binding the host `$PREFIX/bin` at `/bin` for BOTH arms (symmetric), with the guest at
   `/opt/hello` so neither arm can accidentally run a host binary.
2. The first guest build was refused (`REFUSING ... rc=2`) for a missing `<stdlib.h>` and for `%%s` left
   over from the Python raw string. Fail-closed rather than a silent skip, which is the behaviour the
   guard is supposed to have.

**Measured result (exit 1 - NOT EQUAL, and that is the finding):**

| observable | proot (default) | native | verdict |
|---|---|---|---|
| exit status | 7 | 7 | same |
| stdout | `differential-ok`, `argv[0]=/opt/hello`, ... | identical except the fields below | differs only in those fields |
| file written in rootfs | `written-by-guest` | `written-by-guest` | **same** |
| `cwd` | `/` | `/data/data/com.termux/files/home/workspace/PDN` | **DIFFERS** |
| probe env var | `<unset>` | `<unset>` | same (neither path passes it) |
| `/proc/self/exe` | `readable` | `absent` | **DIFFERS** |
| `uid` / `euid` | `0` / `0` | `10396` / `10396` | **DIFFERS** |
| wall time (median of 3) | 0.186 s | 0.087-0.124 s | native faster by ~1.5-2.1x |

**Three named gaps, which is what step 8 is blocked on:** the working directory is not set on the
native path (`RunPlan` has no cwd field, so the guest inherits the HOST cwd), the guest sees the host
uid because no `--change-id=0:0:0` mapping exists on that path, and `/proc` is not mounted so anything
reading `/proc/self` sees nothing. Each is fixable in the engine or the plan, and each must be fixed or
explicitly modelled before the default moves.

**Not measured, and said so in the script's own output** rather than in a summary: the remaining
synthesised mounts (`/dev`, `/sys`, the Android bind set - only `/proc` was probed), hostname and
kernel-release overrides, bind readonly semantics, namespaces and clone flags, and signal/ptrace
behaviour inside the guest. The list was corrected during the work: it first still named cwd, envp and
uid as uncompared AFTER the guest started reporting them, which understates evidence the harness itself
produces.## p7 fixture leak: corrected mechanism, then fixed with a Drop guard (2026-11-09 03:05 WIB)

**Correction of the record, not a quiet rewrite.** The section "p7 step 6 follow-up" above says
`crates/proot-core/tests/native_dynamic_guest.rs` "cleans its root at line 177 *before* the test and never
after". The first half is right and the second half is WRONG: a `remove_dir_all` also sat at the END of
the test (line 403, now removed). Six fixture trees under `$PREFIX/tmp/p7-loader-*-dyn` were genuinely
present, so the observation stood - the explanation attached to it did not.

**The real defect, and it was reproduced on purpose.** The trailing call runs only when the test
PASSES. A panicking assertion unwinds straight past it, which is exactly the situation in which a
fixture tree should be cleaned up and is not. Three runs of the same test target, with the tree counted
from disk after each (`ls -d $PREFIX/tmp/p7-loader-* | wc -l`):

| state | result | surviving trees |
|---|---|---|
| planted `panic!` after the fixture is built, **no** guard | `FAILED. 0 passed; 1 failed`, exit 101 | **1** (`p7-loader-17518-dyn`) |
| the same planted panic, **with** the guard | `FAILED. 0 passed; 1 failed`, exit 101 | **0** |
| panic removed, guard kept | `ok. 1 passed; 0 failed; 0 ignored`, exit 0 | **0** |

The middle row is the point: the guard does not suppress the failure, it only makes teardown
unconditional. Fix: a `FixtureGuard(PathBuf)` whose `Drop` removes the root, constructed immediately
after `fixture_root("dyn")`; the pre-clean at the top of the test is kept, because it is what makes a
re-run start from a clean tree. The manual call at the end is deleted with a comment saying who owns
teardown now - a manual cleanup beside a guard reads as if both are needed.

**Why the leak mattered at all:** dashboard tests and every `cargo test -p proot-core` run used the same
`$PREFIX/tmp` root, so an unbounded leak is a slow disk-eating defect on a device with 11 GB of RAM and a
shared filesystem, and a stale `p7-loader-*` tree is indistinguishable from a live fixture to anyone
debugging by hand.## p7 step 7 addendum: the gap inventory completed, and a correction to the cwd claim (2026-11-09 03:20 WIB)

The differential guest now also reports `/dev/null` writability and `/sys` readability, so the three
per-syscall gaps are no longer the whole inventory. Same harness, same invocation, before any fix:

| field | proot (default) | native | verdict |
|---|---|---|---|
| exit, stdout-head, marker file | 7 / `differential-ok` / `written-by-guest` | identical | same |
| `cwd` | `/` | the fixture ROOTFS path this run | differs |
| `envp` probe | `<unset>` | `<unset>` | same |
| `devnull` | `writable` | `absent` | **differs** |
| `proc` (`/proc/self/exe`) | `readable` | `absent` | differs |
| `sysfs` (`/sys`) | `readable` | `absent` | **differs** |
| `uid` / `euid` | `0` / `0` | `10396` / `10396` | differs |

Verdict line: `DIFFERENTIAL: NOT EQUAL - stdout, cwd, devnull, euid, proc, sysfs, uid differ.`

**Correction.** The step-7 entry above says the native guest "inherits the host working directory
(`/data/data/com.termux/files/home/workspace/PDN`)". That value was measured, but the generalisation is
wrong: on this run the same field read `/data/.../engdiff7-32520/.podnext/containers/engine-differential/
rootfs`. The native guest therefore inherits **whatever cwd the podnext process happens to have at fork
time**, which is neither `/` nor stable - a stronger and more useful statement than naming the directory I
happened to observe. The gap is the same; the sentence describing it was too narrow.

**Where the proot path's `/proc`, `/dev` and `/sys` come from - read, not assumed.** Not from podnext:
`sysdata::bindings` (`crates/proot-distro-core/src/sysdata.rs:527-604`) only MASKs parts of `/proc`
(`/proc/net`, `/proc/mounts`, `/proc/1`, every numeric `/proc/<pid>`) and returns an empty vector when its
staging directory is absent, and `guestdirs::bindings`
(`crates/proot-distro-core/src/guestdirs.rs:57-70`) binds only `/dev/shm` and `/tmp`. Those binds are added
to the proot engine's argv in `runtime.rs:141-149`, and the native path never runs that code: it builds
`RunPlan` from `container.config.bind_mounts` alone. So the guest-visible `/proc`, `/dev` and `/sys` on the
proot arm come from **proot's own default setup**, which means native parity has to be produced
deliberately (bind them, or emulate them) rather than inherited from the container record.

This is now five causes behind six differing fields: cwd (no chdir), devnull, proc, sysfs (no synthetic
mounts), and uid/euid (no id mapping). Env is the one field that already agrees.## p7 step 8, gap 1 of 3: the starting directory, and the over-refusal it exposed (2026-11-09 03:30 WIB)

**Delegated to `deleg_901d9bfc`** (worktree `wt-p7cwd`, branch `dept/p7-cwd`, base `36ff688`), verified by
the parent from the tree rather than from the report: `b43afbe`, 4 files, +287/-13, `pub cwd: Option<String>`
at `native_run.rs:185`, `resolve_cwd` at `:1417`, `session.rs:364` sets
`cwd: Some(working_dir.unwrap_or("/"))`, test `the_engine_chdirs_the_guest_into_the_plans_cwd` at
`native_exec_translation.rs:684`. Cherry-picked as `58ea7e5` (auto-merged the one forced line in
`native_dynamic_guest.rs`, which the parent's `FixtureGuard` commit had meanwhile touched).

Design points the parent checked in the code, because they are the risk: `cwd_host` is resolved ONCE in
`run_native` before the fork, so a refusal is an `Err` from `run_native` rather than a child exit code; the
child issues a RAW `SYS_chdir` (a preloaded library can interpose `libc::chdir`, the same trap `execve`
hit) and `_exit(126)` if it fails; the parent loop exempts the engine's own pre-exec `chdir` only for
`pid == child && !slots[idx].execve_handled`, so a guest `chdir` after the exec is still the unhandled
path syscall it always was. Non-absolute or `..`-containing cwd is a NAMED refusal.

**Measured end-to-end by the parent** (the child declared this UNVERIFIED, correctly - it cannot run the
CLI harness without the operator's store). Rebuild `target/debug/podnext` in 137 s, then
`scripts/engine_differential.py`:

| field | proot | native | before this commit | verdict |
|---|---|---|---|---|
| `cwd` (guest string) | `/` | `<fixture>/containers/engine-differential/rootfs` | the podnext process's own cwd, and it MOVED between runs (repo path once, the fixture rootfs another time) | informational, see below |
| `relmarker` (relative write) | `relative-write-ok` | `relative-write-ok` | n/a | **same** |
| `marker` (absolute write) | `written-by-guest` | `written-by-guest` | same | same |
| `envp` probe | `<unset>` | `<unset>` | same | same |
| `devnull` / `proc` / `sysfs` | `writable` / `readable` / `readable` | `absent` / `absent` / `absent` | same | differs |
| `uid`/`euid` | `0`/`0` | `10396`/`10396` | same | differs |
| wall time (median of 3) | 0.156 s | 0.107 s | - | native faster |

`cwd` became an INFORMATIONAL field, with the reason in the harness: proot virtualises `getcwd()` so its
guest sees `/`, while the native engine translates guest paths to host paths and does not virtualise the
answer. The comparable property is the EFFECT of the starting directory, which `relmarker` measures by
writing a RELATIVE path and checking where it lands - and it lands in the container on both engines. That
is the parity claim for this increment, stated as the effect rather than as a string.

**NEW FINDING, produced by that very measurement: the engine now refuses runs that were correct.** With the
cwd translated, a relative guest path is resolved by the kernel against a directory that is already inside
the rootfs, so it cannot escape. The engine nevertheless records it as an untranslated path syscall and
fails the whole run:

```
Error: the native engine did not translate 1 path syscall(s): SYS_openat (56) [its path could not be
translated (unreadable, empty or relative); the HOST path is not used as a fallback]. ... The engine does
NOT fall back to proot; run under `--engine proot` for full path coverage.
```
while `rootfs/relative-marker.txt` (18 bytes) is present - the write SUCCEEDED. All three manual runs
exited 1, so it is deterministic, not a flake. Before this commit the same refusal was CORRECT: with the
cwd left at the host's directory, a relative path really did resolve on the host. The rule "refuse rather
than silently fall back" is right; its scope is now one step too wide, and the fix is either to translate
`AT_FDCWD`-relative paths against the known translated cwd (the engine knows it now) or to name them as a
distinct, safe class instead of as an untranslated fallback. That is the next increment, and it was found
only because the differential compares EFFECTS rather than strings.

**Stale prose, clarified rather than left alone:** `native_dynamic_guest.rs:411-413` and
`podnext/src/native.rs:311-314` describe `SYS_chdir` as "a path-taking syscall with no translator". Still
true of a GUEST `chdir`; the clause added says the engine's own pre-exec `chdir` is exempt because it is
handed an already-translated host path.## p7 relative paths: the over-refusal fixed, and two findings it exposed (2026-11-09 04:20 WIB)

**Fix: a relative guest path is now translated when the engine knows the guest's starting directory.**
`native_run.rs`: `path_syscall` returns a `PathSyscall { name, arg_index, dirfd_arg }` instead of a
2-tuple, so the table that already names each syscall now also says where its dirfd lives - one source of
truth, not a second list that can drift. `translate_path_remote` joins a relative name onto
`RunPlan::cwd` and goes through the same `resolve`, but ONLY when (a) the syscall resolves the name
against the cwd - `arg0 == AT_FDCWD` for the `*at` family, or no dirfd at all for legacy
`open`/`stat`/`access` - and (b) the name carries no `.`/`..` component. Both conditions earn their place:
for any other fd the kernel resolves the name against THAT directory, so joining it onto the cwd would
silently open a different file, and collapsing `.`/`..` is the lexical work the cwd guard already does.

**Measured on the harness that found the defect** (`scripts/engine_differential.py`, same fixture, same
argv, 3 runs per arm): before, the native arm exited **1** with
`the native engine did not translate 1 path syscall(s): SYS_openat (56)` while `rootfs/relative-marker.txt`
was present; after, `exit: same (7 = 7)`, `relmarker: same`, `marker: same`, and **zero** occurrences of
"did not translate". Remaining differences are the two untouched gaps: `devnull`/`proc`/`sysfs` and
`uid`/`euid`.

**Regression test**, `a_relative_path_is_translated_against_the_plans_cwd` (in
`crates/proot-core/tests/native_exec_translation.rs`), three arms on one guest: relative name with
`cwd: Some("/probe")` -> lands in the bound HOST directory and the report names the cwd-joined guest path
`/probe/relative-report` with no `SYS_openat` refusal; `cwd: None` -> the same open is still refused
(negative control, without which "stopped refusing everywhere" would pass); a `..` name -> refused. Proven
able to fail: with the fix stashed the test fails on
`the report must name the cwd-JOINED guest path, not the bare relative one`. Whole target: **6 passed**.

**Finding A - `strict` mode cannot reach a user syscall at all.** The first attempt at the `..` arm used
`strict = true` and the run died with `exit = -1` and a report holding only `SYS_read (63)`: the engine's
own bootstrap read on the release pipe is itself a pass-through refusal, so strict stops on the engine.
That matches the documented limitation ("the only mode a dynamically linked guest survives"), but it means
strict is unusable as a test arm for anything the engine bootstraps through.

**Finding B - the engine passes ONLY `argv[0]` to the guest, silently.** `native_run.rs:604-609` publishes
`argv[0]` into the page, sets `argv_slots[1] = null`, and the comment beside it talks only about argv[0]
being the program's own name; nothing says the rest are dropped. Measured while testing: a guest invoked
with `argv[1] = "../escaped-report"` observed `argc == 1` and silently fell back to its compiled-in default
(it printed `[guest] name=relative-report cwd=...`), so the `..` name never reached the engine at all.
The consequence for the operator goal is direct: `podnext --engine native run c -- /bin/sh -c 'cmd'` runs
`/bin/sh` with NO arguments and no error. Recorded as the next increment (`p7-argv`); the test above works
around it by compiling the name in, and says so in a comment rather than hiding it.## p7 argv: the engine published only argv[0], and the guest never said so (2026-11-09 05:05 WIB)

**Finding.** `native_run.rs` published `argv[0]` into the shared page and then terminated the pointer array
(`*argv_slots.add(1) = std::ptr::null()`), with a comment that spoke only about argv[0] being the program's
own name. Nothing documented that every LATER argument was dropped, and nothing reported it: the guest saw
`argc == 1`, used its own defaults, and exited 0. Reproduced while writing a test for the `..` guard: a probe
invoked with `argv[1] = "../escaped-report"` printed `[guest] name=relative-report` - its compiled-in default
- so the `..` never reached the engine at all. For the operator's goal the consequence is direct:
`podnext --engine native run c -- /bin/sh -c 'cmd'` runs `/bin/sh` with no arguments and no error.

**Action.** The page already had 2,496 unused bytes after the header (`PAGE_BYTES = 4096`, last used region
`[1536 .. 1600)`), so no layout change was needed, only a fifth documented region:
`[1600 .. 4096)` holds the strings for `argv[1..]`, NUL-separated. New constants derive their values from
the page rather than restating them (`ARGV_EXTRA_OFFSET = 1600`, `ARGV_EXTRA_CAPACITY = PAGE_BYTES -
ARGV_EXTRA_OFFSET - 1`, `ARGV_SLOTS = (HEADER_OFFSET - ARGV_OFFSET) / size_of::<*const c_char>()` = 64).
Both limits are checked in `run_native` BEFORE the fork, so exceeding them is an `Err` naming the limit
("argv of 81 entries needs 82 slots ...", "argv strings after argv[0] total 3001 bytes and do not fit ...")
instead of a child that exits 126 for a reason the caller cannot read. The child then copies each entry and
writes the terminating null into the slot after the last.

**Measured, three ways.**

1. Regression test `the_guest_receives_every_argument` (7th test in `native_exec_translation.rs`): the guest
   reports `argc` and every entry into a bound HOST directory, invoked as
   `["/bin/argv", "one", "two words", "--flag=value"]`, and the report must equal
   `argc=4|argv[0]=/bin/argv|argv[1]=one|argv[2]=two words|argv[3]=--flag=value` - a space and an `=` are in
   the fixture because a naive re-split would mangle them. Two refusal arms cover the limits. Proven able to
   fail: with the fix stashed the report is `argc=1|argv[0]=/bin/argv` and the assertion fires. Target:
   7 passed.
2. `scripts/engine_differential.py` now passes `alpha` and `beta gamma` to BOTH engines and the guest prints
   `argc`, so this class is compared on every future run. Before rebuilding the CLI binary it read
   `argc: DIFFERS proot='3' native='1'`; after `cargo build -p podnext --bin podnext` (mtime 05:02:26) it
   reads `argc: same proot='3' native='3'` with identical `argv[0..2]` lines in both stdouts, including
   `argv[2]=beta gamma`.
3. The first differential run in (2) is itself evidence of the stale-binary trap: the source was fixed, the
   binary was not, and the harness measured base behaviour. Rebuild before believing a CLI-level difference.

**Still different, and measured every run:** `devnull`, `proc`, `sysfs` (gap 2: the proot arm gets them from
proot's own defaults, not from podnext) and `uid`/`euid` (gap 3), plus `cwd` as an informational field.## p7 gap 2: /dev and /proc now match; /sys is measured but NOT explained (2026-11-09 05:55 WIB)

**Correction of my own record, first.** The step-7 addendum above says the guest's `/proc`, `/dev` and `/sys` under the
proot arm "come from proot's own default setup" and that "podnext never runs that code". The first half is
WRONG: podnext hands proot `--bind=/dev`, `--bind=/proc` and `--bind=/sys` itself
(`crates/proot-core/src/lib.rs:244-246`, and again at `:386-388`), and only then appends the sysdata masks and
the guest runtime directories (`runtime.rs:141-149`). The native path builds its plan from the container
record alone, which is why it was missing all of it - the conclusion stood, the attribution did not.

**Action.** `run_native_engine` (`crates/podnext/src/native/session.rs`) now reproduces that set: identity binds
for `/dev`, `/proc`, `/sys`; then `sysdata::bindings(container_dir)` and `guestdirs::bindings(container_dir)`
parsed from `host:guest` (a trailing `!` is a proot-only "do not dereference" flag, stripped and the bind
kept), with `container_dir = container.rootfs_path.parent()` - the same expression the proot path uses.
Ordering is safe by construction: `resolve` picks the LONGEST matching prefix and, on a tie, the first entry,
so a sysdata mask like `{stub}:/proc/net` always beats the identity `/proc` bind regardless of position.

The widening is stated in the code rather than inherited: binding the host `/proc` lets a native guest read
the process table of its own uid, exactly as the proot path already does.

**Measured (`scripts/engine_differential.py`, same fixture, 3 runs per arm, new CLI binary built 05:48:44):**

| field | before | after |
|---|---|---|
| `devnull` | `writable` vs `absent` | **same** (`writable`) |
| `proc` | `readable` vs `absent` | **same** (`readable`) |
| `sysfs` | `readable` vs `absent` | still differs |
| `argc` | - | same (3) |
| `uid`/`euid` | `0/0` vs `10396/10396` | still differs |

**`/sys` is open, and NOT explained.** Two facts measured, one hypothesis unwritten: (a) the host `/sys` is not
readable by this uid (`test -r /sys` fails, `ls /sys` exits 2), so `absent` on the native side is consistent
with the host; (b) inside the PROOT arm the same probe returns `readable-rc=0`, i.e. `access("/sys", R_OK)`
succeeds there. Mode bits on `/sys` are 0555, so `access` answering 0 is plausible while a real `readdir` is
denied - which is exactly why `sysdata` exists (it masks `/sys` with an empty directory when the host one
cannot be read, and its staging directory does not exist in this fixture). What is NOT established is why the
native arm's `access("/sys")` answers differently from proot's. The next experiment is bounded and written
down: a compiled guest that calls `stat("/sys")` and `open("/sys", O_RDONLY|O_DIRECTORY)` and reports both
errno values, run under each engine - the sh-based probe used here is invalid for the native arm because
`/bin` is a bind to the host and the guest shell cannot resolve `libc.so`.

Claim discipline for this increment: two of the three fields flipped and are quoted; the third is reported as
open with the facts that are known, not folded into a parity claim.