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



