# v0.1.9 Stabilization & Hardening Synthesis Report

**Status**: All eight remediation batches integrated on `master`; `v0.1.9` released and verified
**Date**: November 5, 2026 (WIB)
**Integration head**: `c3139bb`
**Baseline**: 57 empirically observed audit findings (2 critical, 16 high, 22 medium, 12 low, 5 nit)

---

## 1. Stream Status Overview

| Batch | Target Scope | Findings Covered | Worktree (branch) | State on `master` | Evidence |
| :---: | :--- | :--- | :--- | :--- | :--- |
| **Manual** | Critical C1 (argv non-UTF8 `#41`) & C2 (image pull honesty `01-1`) | 2 critical | host | merged & pushed (`e08ff48`) | CI runs 41951506268, 41951506196 — success |
| **A** | Storage layer validation & path traversal | 5 (`05-2`..`05-6`) | `wt-fix-a` (`fix/storage-validation`) | merged (`356e5f8`, `703f151`, `2ca711b`, `a59f0f5`, `5afaa8d`) | `names::store_key_tests`, `storage::tests` pass |
| **B** | Automation contract: JSON purity, prompts, exit codes | 6 (`04-1`..`04-6`) | `wt-fix-b` (`fix/automation`) | merged (`f6329c5`) | `crates/podnext/tests/automation_contract.rs` |
| **C** | Guest environment: isolation, truthful mounts, live DNS | 5 (`11-1`..`11-5`) | `wt-fix-c` (`fix/guest-env`) | merged (`04692dd`) | `network::tests`, `sysdata` bindings |
| **D** | Documentation honesty & phantom CLI deletion | 6 (`13-1`..`13-3`, `13-7`..`13-9`) | `wt-fix-d` (`fix/docs`) | merged (`4fe2455`) | CLI reference vs Clap tree |
| **E** | Install & image mechanics | 6 (`01-2`..`01-7`) | `wt-fix-e` (`fix/install`) | merged (`839b232`) | staging-directory install path |
| **F** | CLI surface & flag reconciliation | 7 (`13-4`..`13-6`, `13-10`..`13-13`) | `wt-fix-f` (`fix/cli`) | merged (`66e733b`) | Clap definitions + docs |
| **G** | Sysdata virtualization (`/proc`, `/sys`, `/dev`) | 6 (`11-6`..`11-11`) | `wt-fix-g` (`fix/proc-stubs`) | merged (`9dbce8e`) | `sysdata::tests` (5 checks) |
| **H** | Polish & defect mop-up | 11 (`04-7`..`04-12`, `01-8`..`01-10`) | `wt-fix-h` (`fix/polish`) | merged (`ebffba0`..`8bc2ef4`, 8 commits) | per-finding unit tests |

Twenty commits landed between `7cf3fd4` and the integration head: 18 originating in the
batches, 2 written during integration (see §3).

---

## 2. Verification Evidence (measured on the Android/Termux target)

| Command | Result |
| :--- | :--- |
| `cargo fmt --all -- --check` | exit 0, no diff |
| `cargo check -p proot-distro-core -j 1` | exit 0 (peak 455 MB, 8 s) |
| `cargo check -p podnext -j 1` | exit 0 (peak 1190 MB, 48 s) |
| `cargo test -p proot-distro-core` | **56 passed / 0 failed** (lib, 1.04 s) + 3 + 1 integration tests passed |
| `cargo test --workspace -j 1` | all binaries green: 4, 1 (258 s), 9, 9, 2, 58, 19, 32 tests passed, 0 failed — one stale assertion was fixed in `c3139bb` (see §3.4) |
| `build_issue_ledger.py --offline --json` (source predicates) | 55 PASS / 0 FAIL / 4 SKIPPED of 59 checks |
| `build_issue_ledger.py --cargo` (full verdicts) | **59 PASS / 0 FAIL / 0 SKIPPED** of 59 checks |

The four cargo-gated behaviour checks (`#16`, `#18`, `#25`, `#31`) carry verdicts in the `--cargo`
run recorded above; the source-only run leaves them `SKIPPED`, and a `SKIPPED` check is never a pass.
The `Issue ledger` CI workflow (the required status check on this branch) runs the same gate with
`--cargo`.

`cargo test --workspace` is run on the device with `-j 1` under the memory guard; release
builds and the full matrix remain the CI's job, because a release build on the device
exceeds the 2400 MB budget and the guard kills it (`BUILD_RC=137`).

---

## 3. Integration-Only Defects (invisible to per-batch CI)

Per-batch verification cannot see these: they appear only when the branches meet, or only
on the target device. All four were found by running the merged tree, and all four are
fixed in `a52029f`, `2925626` and `c3139bb`.

1. **Duplicate symbols from a union-of-hunks conflict resolution (`E0428`).**
   Batches C and G both edited `crates/proot-distro-core/src/sysdata.rs`. The conflict was
   resolved by keeping both sides of every region, which left two definitions of
   `FAKE_MOUNTS` and two of `fake_vmstat`. The crate did not compile at all.
   Fix: keep the richer mount table (`/dev`, `/dev/shm`, `/tmp` included) and the helper
   that reads `host_mem_free_kb()`, drop the older duplicates.

2. **Borrow conflict in the proot spawn path (`E0499`).**
   `crates/proot-core/src/lib.rs` acquired the command's mutable reference before
   `.spawn()`. Only visible once the umask/pgid work met the `pre_exec` hook.
   Fix: call `pre_exec` on `cmd` first, then chain `stdout`/`stderr`/`stdin`/`process_group(0)`
   into a single `.spawn()`.

3. **A read probe on `/dev/ptmx` blocks forever — the test hung instead of failing.**
   `sysdata::bindings()` probes host paths with `readable()`, which opens the path and reads
   one byte. On a device where `/sys` and `/dev` cannot be listed, the `/dev` mask branch
   runs and probes the device nodes. A 1-byte read from a PTY master waits for data the
   slave never sends, so `bindings()` never returned:
   `sysdata::tests::ensure_writes_every_entry_then_bindings_cover_the_blocked_ones` stalled
   past 180 s (memguard peak 86 MB, `--test-threads=1`), where CI's Ubuntu runner lists
   `/dev` normally and skips the branch entirely.
   Fix: `readable()` returns true on metadata alone for a character device, so the guest
   still gets the real `/dev/null`, `/dev/zero`, `/dev/urandom` and `/dev/ptmx` bound
   without ever reading from one.
   The same test then failed fast on a second, independent defect: it asserted that
   `/sys/fs/selinux` is always bound, but the two mask branches are mutually exclusive. On
   Android the whole `/sys` tree is masked, so the assertion now accepts either mask.

4. **A cross-crate assertion left stale by a new bind.**
   Batch G added a second bind to `proot_distro_core::guestdirs::bindings()`
   (`<container>/tmp:/tmp`, so the guest gets a writable `/tmp` from its own directory
   rather than the host's) and updated the tests inside `guestdirs`. The colon-free
   store-key test in `crates/podnext/src/commands/mod.rs` — a different crate — asserted
   that every bind ends with `:/dev/shm`, so the merged tree failed with
   `unexpected bind: <tmpdir>/tmp:/tmp`. The code is right; the assertion now accepts
   either target while keeping its real intent (exactly one colon, and only directories
   PDN owns). Re-run: `cargo test -p podnext --bin podnext` → 32 passed / 0 failed.

Lesson: "does it compile" and "do the unit tests pass" are per-branch questions. The
integration tree needs its own compile, its own test run, and its own ledger pass before
release.

---

## 4. Integrated Behaviour Highlights

### 4.1 Critical hardening — zero-lie engine
- **argv non-UTF8 sanitization (`#41`)**: `std::env::args_os()` + `to_string_lossy()`
  replaces `std::env::args()`; regression test `crates/podnext/tests/argv_nonutf8.rs`.
- **Deceptive `image pull` neutralized (`01-1`)**: bails with exit 1 and points the user at
  `podnext install`, instead of fabricating a local container.

### 4.2 Storage and name hygiene (Batch A)
- `validate_container_name` is the single validator behind container, volume and network
  names: `../`, control bytes, empty names and invalid delimiters are rejected.
- `volume rm` deletes the mountpoint directory and returns non-zero when a removal fails.

### 4.3 Guest environment (Batches B, C, G)
- Truthful `/proc/mounts` and `/etc/mtab`; host PIDs masked out of the guest's `/proc`.
- `/etc/resolv.conf` is written when a docker image ships none, so `apk update` resolves.
- `/sys` masked, `/proc/net`, `/proc/filesystems`, `/proc/cmdline` stubbed, `umask 022`.
- Interactive prompts and progress messages go to stderr, so `--format json` output stays
  parseable; a non-tty stdin without `--force`/`--yes` is an error, not a silent no-op.

### 4.4 Documentation honesty (Batches D, F, H)
- Fictitious `podnext storage` group purged; fictitious exit codes 3–12 replaced by the real
  codes (0, 1, 2, child pass-through); phantom Clap options removed from the reference.
- Top-level `--help` examples standardized to English; unimplemented build stubs annotated
  and warned about on stderr.

---

## 5. Release Outcome and the Remaining Gap

Completed for v0.1.9 (all items below carry an artifact, not an intention):

1. Workspace suite green on the device (`cargo test --workspace -j 1`) and the ledger re-run with
   `--cargo`: **59 PASS / 0 FAIL / 0 SKIPPED** of 59 checks — the four gated behaviour checks
   (`#16`, `#18`, `#25`, `#31`) now carry verdicts instead of `SKIPPED`.
2. CI green on the release commit `be4919d`: 14 of 14 check runs `success` (`Rust Host Build`,
   `Fuzzing`, `Security Audit`, `Build Documentation`, `PRoot Distro Integration Test`,
   `Rust Android Cross-Compile (aarch64)`, four NDK cross-compile matrix jobs, `Run Benchmarks`,
   `Issue ledger`, `Aggregate Artifacts & Checksums`, `Report benchmark results`).
3. `research/ISSUE_LEDGER.md` regenerated by the script and committed (`64cfc8f`); its summary
   paragraph is now conditional on the skipped count, so it no longer explains away skips that did
   not happen.
4. `v0.1.9` tagged and published. Verified from the published bytes, not from the workflow's green
   check: `podnext-aarch64-linux-android`, 19 624 960 B, sha256
   `4b7f0297ad265ad6a193c325a6975db5fc3eb7d35ee3f59d825e71d0ad6cb35e` — the digest reported by the
   API and the digest of the downloaded file agree; `file` reports ELF aarch64 with interpreter
   `/system/bin/linker64` (NDK r27c); the binary prints `podnext 0.1.9` and lists real containers.

### Known gap: the tag-gated `Release` job in `ci.yml` never built

`ci.yml` carries a second publisher for the same tag (`release:`, gated on
`github.event_name == 'push' && refs/tags/v*`). It does not install the Android Rust target, so its
"Build release binaries (Android aarch64)" step died with
`error[E0463]: can't find crate for core` / `the aarch64-linux-android target may not be installed`,
and the job failed (exit 101). The `Release Automation` workflow is what produced the published
assets, so the release itself is unaffected.

Fixed by adding `targets: aarch64-linux-android` to that job's toolchain step. **The fix is not yet
observed on the event it guards**: the job only runs for a tag push, and a push to `master` does not
exercise it — the next tag is the evidence. Note also that two workflows now attach assets to the
same GitHub release (tarball set from `ci.yml`, binary set from `release.yml`); consolidating them
into one publisher is a process decision, not a code defect, and is left to the maintainer.

Next: v0.2.0 — Fast-Path Ptrace Shim (`libpdn_fastpath.so`), SQLite inode Virtual Overlay Engine,
OCI whiteout layer stacking (`research/OCI_WHITEOUT_AND_LAYER_ENGINE_SPEC.md`), and the 18-flag
login parity recorded in `research/PROOT_DISTRO_DEEP_ANATOMY_AND_PDN_PARITY.md`.
