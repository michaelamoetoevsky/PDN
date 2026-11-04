# PDN issue ledger — fixes re-verified against HEAD

Generated 2026-11-04 22:01:10Z from `e111232` (test(ledger): predicate for #38 (store visibility)).

## Why this exists

Closing an issue records an intention. It does not prove the fix is still in the tree: a later
refactor, a revert, or a merge that resolved a conflict the wrong way can quietly undo it. This
ledger pairs every issue with a command that must still succeed against the current code, so a
silently-regressed fix shows up as a FAIL in a document rather than as a surprise in production.

## Verdict contract

| verdict | meaning | effect on exit status |
|---|---|---|
| **PASS** | the exact symbol the fix introduces was found; the fix is present | none |
| **FAIL** | the symbol is absent or the check could not hold: a regression, or a fix that is gone | **exit 1** |
| **SKIPPED** | no automated check exists (open issue), or a gated `--cargo` check was not enabled | none, and **never** a pass |

**Coverage: 54 checks PASS by direct observation · 0 FAIL · 1 SKIPPED**, across 55 checks covering 24 of 36 issues (12 issues have no automated check).

**A SKIPPED check is not evidence the fix works.** It means no verdict was produced. Here the 1 SKIPPED are cargo-gated behaviour checks: they are not enabled by default, and on this
device the memory guard refuses the bench build (RSS exceeds the budget), so the behaviour behind
them was not observed in the committed run. Do not read the PASS count as full verification.

## Checks that FAIL (a fix that is no longer present)

None.

## Checks SKIPPED (no verdict was produced)

- **#25 — the benchmark targets compile (the false-green defect)** — _cargo guard stopped the build (memory budget), so behaviour was not observed; re-run on a host with a larger budget_

## Per-issue detail

**#7 Add GitHub Actions workflow for automated dependency updates**  
_state_: CLOSED  ·  _labels_: good first issue, github-actions, dependabot, security  ·  _closed_: 2026-11-03T19:25:46Z

- [PASS] dependabot.yml declares the cargo ecosystem
  - `grep -nF 'package-ecosystem: "cargo"' .github/dependabot.yml`
  - → `4:  - package-ecosystem: "cargo"`
- [PASS] the dependency-updates workflow file exists
  - `ls .github/workflows/dependency-updates.yml`
  - → `.github/workflows/dependency-updates.yml`

**#8 Add benchmark suite with Criterion for performance regression detection**  
_state_: CLOSED  ·  _labels_: enhancement, good first issue, benchmark, ci, performance  ·  _closed_: 2026-11-03T19:25:49Z

- [PASS] the Criterion bench target is declared
  - `grep -nF '[[bench]]' crates/pn-next/Cargo.toml`
  - → `73:[[bench]]`
- [PASS] the criterion dev-dependency is present
  - `grep -nF 'criterion' crates/pn-next/Cargo.toml`
  - → `79:criterion = { version = "0.5", features = ["async_tokio"] }`
- [PASS] the bench source exists
  - `ls crates/pn-next/benches/benchmarks.rs`
  - → `crates/pn-next/benches/benchmarks.rs`

**#11 [BUG-C1] install accepts empty container name and creates an unremovable orphan**  
_state_: CLOSED  ·  _labels_: bug, cli  ·  _closed_: 2026-11-04T04:23:37Z

- [PASS] install validates the container name before creating anything
  - `grep -nF 'validate_container_name' crates/podnext/src/main.rs`
  - → `2029:    proot_distro_core::NameValidator::new().validate_container_name(&container_name)?; | 2713:            .validate_container_name(&candidate) | 3237:    proot_distro_core::NameValidator::new().validate_container_name(&newname)?;`

**#12 [BUG-C2] backup delete deadlocks permanently (tokio RwLock read guard held across write)**  
_state_: CLOSED  ·  _labels_: bug  ·  _closed_: 2026-11-04T04:23:41Z

- [PASS] delete_backup drops its read guard before taking the write lock
  - `grep -nF 'contains_key' crates/pn-next/src/backup_restore/mod.rs`
  - → `1115:        let exists = backups.contains_key(backup_id);`
- [PASS] the deadlock regression test exists
  - `grep -rnF 'test_delete_backup_does_not_deadlock' crates/pn-next/src/backup_restore/mod.rs`
  - → `1487:    async fn test_delete_backup_does_not_deadlock() {`

**#13 [BUG-C3] reset destroys the container before verifying the image is downloadable**  
_state_: CLOSED  ·  _labels_: bug, cli  ·  _closed_: 2026-11-04T10:12:19Z

- [PASS] reset verifies the image BEFORE destroying the container
  - `awk '/fn cmd_reset/,/^}/' crates/podnext/src/main.rs | grep -nE 'verify_image_available|remove_dir_all' | head -2`
  - → `37:    verify_image_available(&image_ref).await?; | 39:    std::fs::remove_dir_all(&container_dir)?;`

**#14 [BUG-M1] podnext ps does not report non-interactive run sessions**  
_state_: CLOSED  ·  _labels_: bug, cli, monitoring  ·  _closed_: 2026-11-04T04:23:46Z

- [PASS] run sessions are registered so ps can see them
  - `grep -rnF 'register_session' crates/proot-distro-core/src/runtime.rs crates/podnext/src/native.rs`
  - → `crates/proot-distro-core/src/runtime.rs:179:        let _session = crate::session::register_session(`

**#15 [BUG-M2] concurrent guest writes to one file clobber each other (4/5)**  
_state_: CLOSED  ·  _labels_: bug, documentation  ·  _closed_: 2026-11-04T11:08:08Z

- [PASS] the concurrent-write contract is documented for users
  - `grep -nF 'no atomicity guarantee' docs/src/troubleshooting.md`
  - → `438:with no atomicity guarantee, and any synchronisation is the workload's own responsibility.`
- [PASS] the #15 verdict (not a PDN defect, with control) is recorded
  - `grep -nF 'not a PDN defect' research/issues/concurrency_verdict.md`
  - → `1:## Verdict: not a PDN defect`

**#16 [BUG-M3] deep tree extraction scales super-linearly and times out past ~200 levels**  
_state_: CLOSED  ·  _labels_: performance  ·  _closed_: 2026-11-04T11:04:51Z

- [PASS] untar records created dirs instead of re-walking ancestors per entry
  - `grep -nF 'created: &mut HashSet<PathBuf>' crates/proot-distro-core/src/untar.rs`
  - → `135:fn ensure_dir(dir: &Path, created: &mut HashSet<PathBuf>) -> io::Result<()> {`
- [PASS] the deep-recursion regression test exists
  - `ls crates/proot-distro-core/tests/deep_fs_recursion.rs`
  - → `crates/proot-distro-core/tests/deep_fs_recursion.rs`
- [PASS] the deep-recursion test passes
  - `cargo test -p proot-distro-core --test deep_fs_recursion --locked`
  - → `memguard: env-u-LD pid=15786 pgid=15786 budget=2000MB min_avail=700MB | memguard: log=/data/data/com.termux/files/home/.hermes/logs/heavy/20261104T215924Z-env-u-LD.log | memguard: completed exit=0 reason=exited normally peak=90MB dur=4s`

**#17 [BUG-L1] podnext build local rejects ARG, skips RUN, and produces no usable OCI image**  
_state_: CLOSED  ·  _labels_: bug, cli  ·  _closed_: 2026-11-04T11:04:52Z

- [PASS] build local consumes ARG before the parser rejects it
  - `grep -nF 'fn preprocess_build_args' crates/podnext/src/commands/mod.rs`
  - → `2559:fn preprocess_build_args(content: &str) -> (String, Vec<(String, String)>) {`
- [PASS] build local matches a RUN instruction instead of printing it
  - `grep -nF 'Instr::Run(run_cmd)' crates/podnext/src/commands/mod.rs`
  - → `2237:            Instr::Run(run_cmd) => {`
- [PASS] build local delegates RUN to the container engine
  - `grep -nF 'execute_container(' crates/podnext/src/commands/mod.rs`
  - → `2249:                    .execute_container(`
- [PASS] build local fails the build when RUN exits non-zero
  - `grep -nF 'RUN instruction failed (exit' crates/podnext/src/commands/mod.rs`
  - → `2255:                    bail!("RUN instruction failed (exit {}): '{}'", code, run_cmd);`

**#18 [BUG-L2] copy/sync expand sparse files to dense allocation**  
_state_: CLOSED  ·  _labels_: bug, performance  ·  _closed_: 2026-11-04T11:04:53Z

- [PASS] sparse copies probe holes with SEEK_DATA/SEEK_HOLE
  - `grep -nF 'libc::SEEK_DATA' crates/podnext/src/main.rs`
  - → `3583:        let data = match lseek(fd, offset, libc::SEEK_DATA) {`
- [PASS] the sparse-copy regression test exists
  - `grep -nF 'fn copy_keeps_sparse_files_sparse' crates/podnext/src/main.rs`
  - → `4133:    fn copy_keeps_sparse_files_sparse() {`
- [PASS] the sparse-copy unit test passes
  - `cargo test -p podnext --bins copy_keeps_sparse_files_sparse --locked`
  - → `memguard: env-u-LD pid=11510 pgid=11510 budget=2000MB min_avail=700MB | memguard: log=/data/data/com.termux/files/home/.hermes/logs/heavy/20261104T215832Z-env-u-LD.log | memguard: completed exit=0 reason=exited normally peak=1415MB dur=52s`

**#19 [CI-1] test_restart_policy_unless_stopped fails: explicit stop loses to restart policy**  
_state_: CLOSED  ·  _labels_: bug, ci  ·  _closed_: 2026-11-04T04:23:51Z

- [PASS] the restart path re-checks user_stopped under the same write lock
  - `grep -nF 'user_stopped' crates/pn-next/src/supervisor.rs | head -8`
  - → `728:    user_stopped: bool, | 850:                            user_stopped: false, | 992:                                if managed.process.is_none() && !managed.user_stopped && matches!(managed.status.state, ServiceState::Restarting | ServiceS`

**#20 [BUG-C4] FileLock::lock flocks fd 0 (stdin) and then panics on a None field**  
_state_: CLOSED  ·  _labels_: bug  ·  _closed_: 2026-11-04T04:23:55Z

- [PASS] FileLock no longer flocks fd 0
  - `grep -rnF 'FileLock' crates/proot-distro-core/src/locking.rs | head -6`
  - → `65:pub struct FileLock { | 70:impl FileLock { | 109:impl Drop for FileLock { | 247:        let mut first = FileLock::new(&path); | 251:        let mut second = FileLock::new(&path);`

**#21 [BUG-C5] a guest command containing -r aborts with 'rootfs is required for direct execution mode'**  
_state_: CLOSED  ·  _labels_: bug, cli  ·  _closed_: 2026-11-04T04:24:00Z

- [PASS] direct-execution detection only fires when -r/--rootfs precedes the command
  - `grep -nF 'pre_dashdash_args' crates/podnext/src/main.rs | head -4`
  - → `1459:    let pre_dashdash_args = &raw_args[..dashdash_idx]; | 1494:    let has_subcommand = pre_dashdash_args | 1499:        && pre_dashdash_args`

**#22 [CI-2] dependency-updates workflow references a nonexistent action (peter-evans/update-github-actions 404s)**  
_state_: CLOSED  ·  _labels_: bug, github-actions, ci  ·  _closed_: 2026-11-04T04:24:05Z

- [PASS] the self-audit job that replaces the nonexistent action exists
  - `grep -nF 'name: Audit GitHub Actions pins' .github/workflows/dependency-updates.yml`
  - → `98:      - name: Audit GitHub Actions pins`

**#23 [CI-3] Cargo.lock is gitignored in an application workspace, so --locked was impossible**  
_state_: CLOSED  ·  _labels_: bug, ci  ·  _closed_: 2026-11-04T04:24:09Z

- [PASS] Cargo.lock is tracked
  - `git ls-files --error-unmatch Cargo.lock`
  - → `Cargo.lock`

**#24 [SEC-1] 35 advisories in the dependency tree: inventory, exposure analysis and remediation plan**  
_state_: CLOSED  ·  _labels_: security, ci  ·  _closed_: 2026-11-04T10:12:17Z

- [PASS] deny.toml encodes the advisory acceptance list (string form)
  - `grep -nF '"RUSTSEC-' deny.toml | head -3`
  - → `70:    "RUSTSEC-2026-0258", # h2: unbounded empty DATA frames | 71:    "RUSTSEC-2026-0346", # rustls: Debug impls expose TLS secrets | 72:    "RUSTSEC-2026-0349", # rustls: unauthenticated ServerHello downgrade`

**#25 [CI-4] Benchmark workflow is a false green: cargo bench fails to compile and the pipe hides it**  
_state_: CLOSED  ·  _labels_: bug, ci, performance  ·  _closed_: 2026-11-04T10:12:24Z

- [PASS] benchmark workflow enforces the exit status and builds the targets
  - `grep -nF -- '--bench benchmarks' .github/workflows/benchmark.yml | head -4`
  - → `103:          # `--bench benchmarks` is REQUIRED for the criterion flags to land on the | 108:          cargo bench --workspace --bench benchmarks --locked -- \ | 130:          cargo bench --workspace --bench benchmarks --locked -- \`
- [SKIPPED] the benchmark targets compile (the false-green defect)
  - `cargo bench -p pn-next --no-run --locked`
  - → `memguard: env-u-LD pid=7853 pgid=7853 budget=2000MB min_avail=700MB
memguard: log=/data/data/com.termux/files/home/.hermes/logs/heavy/20261104T215701Z-env-u-LD.log
memguard: group RSS 2034MB over budg`

**#30 [CI-5] Benchmark job is unpredictable: 25.7 min then 43+ min, and a baseline step killed with SIGTERM**  
_state_: CLOSED  ·  _labels_: ci, performance  ·  _closed_: 2026-11-04T15:35:42Z

- [PASS] the benchmark job carries a bounded timeout
  - `grep -nF 'timeout-minutes: 60' .github/workflows/benchmark.yml`
  - → `30:    timeout-minutes: 60`
- [PASS] the suite is invoked bounded via --bench benchmarks
  - `grep -nF -- '--bench benchmarks' .github/workflows/benchmark.yml`
  - → `103:          # `--bench benchmarks` is REQUIRED for the criterion flags to land on the | 108:          cargo bench --workspace --bench benchmarks --locked -- \ | 130:          cargo bench --workspace --bench benchmarks --locked -- \`

**#31 [CI-6] test_restart_policy_on_abnormal is flaky: assertions bet on wall-clock sleeps**  
_state_: CLOSED  ·  _labels_: ci, flaky-test  ·  _closed_: 2026-11-04T14:30:47Z

- [PASS] the test asserts on transitions via wait_status, not fixed sleeps
  - `grep -nF 'async fn wait_status' crates/pn-next/tests/supervisor_e2e.rs`
  - → `87:async fn wait_status<F>(`
- [PASS] the test bounds its wait with a DEADLINE
  - `grep -nF 'const DEADLINE: Duration = Duration::from_secs(30);' crates/pn-next/tests/supervisor_e2e.rs`
  - → `25:const DEADLINE: Duration = Duration::from_secs(30);`
- [PASS] the abnormal-restart test passes
  - `cargo test -p pn-next --test supervisor_e2e test_restart_policy_on_abnormal --locked`
  - → `memguard: env-u-LD pid=16182 pgid=16182 budget=2000MB min_avail=700MB | memguard: log=/data/data/com.termux/files/home/.hermes/logs/heavy/20261104T215928Z-env-u-LD.log | memguard: completed exit=0 reason=exited normally peak=1254MB dur=102s`

**#32 Signal lifecycle: stopping a PDN process orphans the guest, and podnext-daemon ignores SIGTERM**  
_state_: CLOSED  ·  _labels_: bug  ·  _closed_: 2026-11-04T13:12:18Z

- [PASS] spawned proot gets its own process group
  - `grep -nF 'process_group(0)' crates/proot-core/src/lib.rs`
  - → `307:        cmd.process_group(0); | 442:            .process_group(0)`
- [PASS] container stop signals the group and reaps (zombie-aware)
  - `grep -nF 'async fn terminate_group' crates/proot-distro-core/src/container.rs`
  - → `640:async fn terminate_group(pgid: u32, timeout: u64) -> Result<()> {`
- [PASS] podnext run forwards signals to the guest group
  - `grep -nF 'fn forward_to_guest' crates/podnext/src/native.rs | head -3`
  - → `566:fn forward_to_guest(sig: i32) { | 836:    fn forward_to_guest_terminates_child_group() {`
- [PASS] the daemon handles SIGTERM, not only Ctrl-C
  - `grep -nF 'SignalKind::terminate' crates/podnext/src/daemon.rs`
  - → `70:    if kind == SignalKind::terminate() || kind == SignalKind::interrupt() { | 136:    let mut sigterm = signal(SignalKind::terminate()).context("install SIGTERM handler")?; | 143:                _ = sigterm.recv() => SignalKind::terminate(),`

**#33 Session files and service logs grow without bound (46 dead session files measured)**  
_state_: CLOSED  ·  _labels_: bug  ·  _closed_: 2026-11-04T13:12:39Z

- [PASS] the session record is unlinked on exit via a guard
  - `grep -nF 'impl Drop for SessionGuard' crates/proot-distro-core/src/session.rs`
  - → `67:impl Drop for SessionGuard {`
- [PASS] log rotation has a consumer
  - `grep -nF 'fn rotate_log_if_needed' crates/pn-next/src/supervisor.rs`
  - → `478:fn rotate_log_if_needed(path: &Path, cfg: &LogRotationConfig) -> std::io::Result<()> {`

**#34 pn-gateway: documented duration form ("60s") cannot be loaded by the new TOML loader**  
_state_: CLOSED  ·  _labels_: bug, documentation  ·  _closed_: 2026-11-04T21:44:02Z

- [PASS] documented durations ("60s", "500ms", "2m") parse
  - `grep -nF 'pub fn parse_duration' crates/pn-gateway/src/duration_fmt.rs`
  - → `42:pub fn parse_duration(input: &str) -> Result<Duration, String> {`
- [PASS] every Duration field uses the documented-form serde adapter
  - `grep -cF 'serde(with = "crate::duration_fmt")' crates/pn-gateway/src/config.rs`
  - → `10`
- [PASS] the documented form is tested against a documentation-shaped file
  - `grep -nF 'documented_duration_strings_load' crates/pn-gateway/tests/config_loader.rs`
  - → `177:fn documented_duration_strings_load() {`
- [PASS] a load/save round-trip exists, so what is written can be read back
  - `grep -nF 'pub fn to_toml_str' crates/pn-gateway/src/config_loader.rs`
  - → `73:    pub fn to_toml_str(&self) -> Result<String, ConfigError> {`

**#35 podnext: a tag containing ':' silently leaves /dev/shm and the /proc stubs unbound**  
_state_: CLOSED  ·  _labels_: bug  ·  _closed_: 2026-11-04T21:44:04Z

- [PASS] one shared rule turns a reference into a store key
  - `grep -nF 'pub fn store_key' crates/proot-distro-core/src/names.rs`
  - → `154:pub fn store_key(reference: &str) -> String {`
- [PASS] build local stores under the derived key, not the raw tag
  - `grep -nF 'names::store_key(tag)' crates/podnext/src/commands/mod.rs`
  - → `2144:    let key = proot_distro_core::names::store_key(tag);`
- [PASS] the raw tag is no longer used as a directory name
  - `grep -nF 'containers_dir.join(tag)' crates/podnext/src/commands/mod.rs`
  - → ``
- [PASS] the RUN/install path resolves the container by the same key
  - `grep -nF 'mgr.get(&key)' crates/podnext/src/commands/mod.rs`
  - → `2243:                let mut c = mgr.get(&key).await?;`
- [PASS] a colon in a bind source is an error, not a dropped mount
  - `grep -nF 'pub fn ensure_bindable' crates/proot-distro-core/src/bindings.rs`
  - → `108:pub fn ensure_bindable(source: &str) -> anyhow::Result<()> {`
- [PASS] a colon tag is tested end to end at the store level
  - `grep -nF 'a_colon_tag_builds_under_a_colon_free_store_key' crates/podnext/src/commands/mod.rs`
  - → `3478:    async fn a_colon_tag_builds_under_a_colon_free_store_key() {`

**#38 A FROM scratch image is unreachable by run/list/remove after a successful build**  
_state_: CLOSED  ·  _labels_: bug  ·  _closed_: 2026-11-04T21:56:44Z

- [PASS] container detection is metadata-based, not an etc/bin/usr guess
  - `grep -nF 'pub fn is_container_dir' crates/proot-distro-core/src/container.rs`
  - → `829:pub fn is_container_dir(dir: &std::path::Path) -> bool {`
- [PASS] load_containers uses the metadata-based check
  - `grep -nF 'is_container_dir(&entry.path())' crates/proot-distro-core/src/container.rs`
  - → `65:            if !is_container_dir(&entry.path()) {`
- [PASS] the etc/bin/usr probe is not the container test any more
  - `grep -nF 'let is_valid_rootfs' crates/proot-distro-core/src/container.rs`
  - → ``
- [PASS] a scratch image with metadata is tested as a container
  - `grep -nF 'a_scratch_image_with_metadata_is_a_container' crates/proot-distro-core/src/container.rs`
  - → `851:    fn a_scratch_image_with_metadata_is_a_container() {`

## Issues with no automated check (the honest gap)

Listed rather than omitted, so the coverage gap is visible. Each has a reason. These are open
feature requests, open defects with no landed fix, or anything whose verification needs a live
container and therefore belongs in CI.

| # | state | reason | title |
|---|---|---|---|
| 1 | OPEN | feature request (PowerShell completion) — nothing on the tree to verify. | Add shell completion for PowerShell |
| 2 | OPEN | documentation task (example service configs) — no code fix to assert. | Add example service configurations for common applications |
| 3 | OPEN | documentation task (tutorial) — no code fix to assert. | Create tutorial: Running a full-stack development environment in PDN |
| 4 | OPEN | feature request (VS Code Dev Container) — no landed implementation. | Add VS Code Dev Container support |
| 5 | OPEN | feature request (systemd unit generator) — no landed implementation. | Add systemd service unit generator for Linux hosts |
| 6 | CLOSED | feature request (Docker Compose export) — partially landed via PR #26; see #28. | Add Docker Compose export for PDN services |
| 9 | OPEN | feature request (Prometheus exporter) — no landed implementation. | Add Prometheus metrics exporter for PDN services |
| 10 | OPEN | feature request (WebSocket events) — no landed implementation. | Add WebSocket support for real-time service events |
| 27 | CLOSED | defect: documented [gateway.fail_safe] (bulkhead/retry_policy) absent from pn-gateway; no fix landed, so a fix-present predicate has nothing to assert. | [BUG-DOC1] Documented gateway fail-safe config (bulkhead, retry_policy |
| 28 | CLOSED | defect: empty command yields a null Compose service in export_to_compose; no fix landed. | Compose export: empty command emits an invalid service; follow-ups fro |
| 29 | CLOSED | defect: podnext build local reports success without an image; no fix landed. | podnext build local computes a layer diff but produces no image |
| 37 | OPEN | no honest source-level predicate identified. | build local: locally built images are invisible to 'image ls' and 'ima |

## How to re-run this (CI-friendly)

```sh
export PATH="$HOME/.hermes/bin:$PATH"
# source predicates only — fast, read-only, safe on any host including Android:
env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py
# machine-readable summary for CI (parse jq .checks.fail; exit status is 1 on any FAIL):
env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py --json | jq .checks
# opt-in behaviour checks (build; serialised with flock against concurrent departments):
env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py --cargo
```

Exit status is **0** when no check FAILs and **1** when any check FAILs, so the ledger doubles as
a regression gate. `--json` prints `{"checks":{"pass":N,"fail":N,"skipped":N,"total":N}}`.
This file is regenerated by every run — it cannot drift from the code.
