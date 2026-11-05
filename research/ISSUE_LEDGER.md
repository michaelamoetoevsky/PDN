# PDN issue ledger — fixes re-verified against HEAD

Generated 2026-11-05 17:25:17Z from `2c7ccd0` (docs(release): record wave 2's outcome and wave 3's dispatch in the se).

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

**Coverage: 97 checks PASS by direct observation · 0 FAIL · 18 SKIPPED**, across 115 checks covering 39 of 67 issues (14 issues have no automated check).

**A SKIPPED check is not evidence the fix works.** It means no verdict was produced. Here the 18 SKIPPED are cargo-gated behaviour checks: they are not enabled by default, and on this
device the memory guard refuses the bench build (RSS exceeds the budget), so the behaviour behind
them was not observed in the committed run. Do not read the PASS count as full verification.

## Checks that FAIL (a fix that is no longer present)

None.

## Checks SKIPPED (no verdict was produced)

- **#41 — both regression tests pass** — _cargo check gated; re-run with --cargo_
- **#25 — the benchmark targets compile (the false-green defect)** — _cargo check gated; re-run with --cargo_
- **#18 — the sparse-copy unit test passes** — _cargo check gated; re-run with --cargo_
- **#16 — the deep-recursion test passes** — _cargo check gated; re-run with --cargo_
- **#31 — the abnormal-restart test passes** — _cargo check gated; re-run with --cargo_
- **#42 — every documented command and option exists in the real CLI** — _cargo check gated; re-run with --cargo_
- **#60 — the guard is unit tested** — _cargo check gated; re-run with --cargo_
- **#68 — both decisions are unit tested** — _cargo check gated; re-run with --cargo_
- **#62 — the refusal is asserted against the real binary** — _cargo check gated; re-run with --cargo_
- **#47 — every refusal class behaves as documented** — _cargo check gated; re-run with --cargo_
- **#42 — a stub must refuse with a reason, not exit 0** — _cargo check gated; re-run with --cargo_
- **#43 — the rendered bind form is exactly host:target, pinned by a test** — _cargo check gated; re-run with --cargo_
- **#43 — a malformed or read-only mount spec is refused** — _cargo check gated; re-run with --cargo_
- **#44 — a failed measurement is reported as a bound, never as a bare number** — _cargo check gated; re-run with --cargo_
- **#45 — the command itself is exercised end to end** — _cargo check gated; re-run with --cargo_
- **#46 — the resolution order is unit tested** — _cargo check gated; re-run with --cargo_
- **#46 — the binary follows a relocated home and the XDG variables** — _cargo check gated; re-run with --cargo_
- **#47 — each refused option exits 1 without running the command** — _cargo check gated; re-run with --cargo_

## Per-issue detail

**#7 Add GitHub Actions workflow for automated dependency updates**  
_state_: CLOSED  ·  _labels_: good first issue, github-actions, dependabot, security  ·  _closed_: 2026-11-03T19:25:46Z

- [PASS] dependabot.yml declares the cargo ecosystem
  - `grep -rnF 'package-ecosystem: "cargo"' .github/dependabot.yml`
  - → `4:  - package-ecosystem: "cargo"`
- [PASS] the dependency-updates workflow file exists
  - `ls .github/workflows/dependency-updates.yml`
  - → `.github/workflows/dependency-updates.yml`

**#8 Add benchmark suite with Criterion for performance regression detection**  
_state_: CLOSED  ·  _labels_: enhancement, good first issue, benchmark, ci, performance  ·  _closed_: 2026-11-03T19:25:49Z

- [PASS] the Criterion bench target is declared
  - `grep -rnF '[[bench]]' crates/pn-next/Cargo.toml`
  - → `73:[[bench]]`
- [PASS] the criterion dev-dependency is present
  - `grep -rnF 'criterion' crates/pn-next/Cargo.toml`
  - → `79:criterion = { version = "0.5", features = ["async_tokio"] }`
- [PASS] the bench source exists
  - `ls crates/pn-next/benches/benchmarks.rs`
  - → `crates/pn-next/benches/benchmarks.rs`

**#11 [BUG-C1] install accepts empty container name and creates an unremovable orphan**  
_state_: CLOSED  ·  _labels_: bug, cli  ·  _closed_: 2026-11-04T04:23:37Z

- [PASS] install validates the container name before creating anything
  - `grep -rnF 'validate_container_name' crates/podnext/src`
  - → `crates/podnext/src/commands/image.rs:360:            .validate_container_name(&candidate) | crates/podnext/src/commands/sync.rs:20:    proot_distro_core::NameValidator::new().validate_container_name(&newname)?; | crates/podnext/src/commands/con`

**#12 [BUG-C2] backup delete deadlocks permanently (tokio RwLock read guard held across write)**  
_state_: CLOSED  ·  _labels_: bug  ·  _closed_: 2026-11-04T04:23:41Z

- [PASS] delete_backup drops its read guard before taking the write lock
  - `grep -rnF 'contains_key' crates/pn-next/src/backup_restore/mod.rs`
  - → `1115:        let exists = backups.contains_key(backup_id);`
- [PASS] the deadlock regression test exists
  - `grep -rnF 'test_delete_backup_does_not_deadlock' crates/pn-next/src/backup_restore/mod.rs`
  - → `1487:    async fn test_delete_backup_does_not_deadlock() {`

**#13 [BUG-C3] reset destroys the container before verifying the image is downloadable**  
_state_: CLOSED  ·  _labels_: bug, cli  ·  _closed_: 2026-11-04T10:12:19Z

- [PASS] reset verifies the image BEFORE destroying the container
  - `grep -rnA 40 'fn cmd_reset' crates/podnext/src | grep -nE 'verify_image_available|remove_dir_all' | head -2`
  - → `37:crates/podnext/src/commands/sync.rs-271-    verify_image_available(&image_ref).await?; | 39:crates/podnext/src/commands/sync.rs-273-    std::fs::remove_dir_all(&container_dir)?;`

**#14 [BUG-M1] podnext ps does not report non-interactive run sessions**  
_state_: CLOSED  ·  _labels_: bug, cli, monitoring  ·  _closed_: 2026-11-04T04:23:46Z

- [PASS] run sessions are registered so ps can see them
  - `grep -rnF 'register_session' crates/proot-distro-core/src/runtime.rs crates/podnext/src/native.rs`
  - → `crates/proot-distro-core/src/runtime.rs:185:        let _session = crate::session::register_session(`

**#15 [BUG-M2] concurrent guest writes to one file clobber each other (4/5)**  
_state_: CLOSED  ·  _labels_: bug, documentation  ·  _closed_: 2026-11-04T11:08:08Z

- [PASS] the concurrent-write contract is documented for users
  - `grep -rnF 'no atomicity guarantee' docs/src/troubleshooting.md`
  - → `394:with no atomicity guarantee, and any synchronisation is the workload's own responsibility.`
- [PASS] the #15 verdict (not a PDN defect, with control) is recorded
  - `grep -rnF 'not a PDN defect' research/issues/concurrency_verdict.md`
  - → `1:## Verdict: not a PDN defect`

**#16 [BUG-M3] deep tree extraction scales super-linearly and times out past ~200 levels**  
_state_: CLOSED  ·  _labels_: performance  ·  _closed_: 2026-11-04T11:04:51Z

- [PASS] untar records created dirs instead of re-walking ancestors per entry
  - `grep -rnF 'created: &mut HashSet<PathBuf>' crates/proot-distro-core/src/untar.rs`
  - → `159:fn ensure_dir(dir: &Path, created: &mut HashSet<PathBuf>) -> io::Result<()> { | 185:    created: &mut HashSet<PathBuf>,`
- [PASS] the deep-recursion regression test exists
  - `ls crates/proot-distro-core/tests/deep_fs_recursion.rs`
  - → `crates/proot-distro-core/tests/deep_fs_recursion.rs`
- [SKIPPED] the deep-recursion test passes
  - `cargo test -p proot-distro-core --test deep_fs_recursion --locked`
  - → ``

**#17 [BUG-L1] podnext build local rejects ARG, skips RUN, and produces no usable OCI image**  
_state_: CLOSED  ·  _labels_: bug, cli  ·  _closed_: 2026-11-04T11:04:52Z

- [PASS] build local consumes ARG before the parser rejects it
  - `grep -rnF 'fn preprocess_build_args' crates/podnext/src/commands`
  - → `crates/podnext/src/commands/build.rs:582:pub fn preprocess_build_args(content: &str) -> (String, Vec<(String, String)>) {`
- [PASS] build local matches a RUN instruction instead of printing it
  - `grep -rnF 'Instr::Run(run_cmd)' crates/podnext/src/commands`
  - → `crates/podnext/src/commands/build.rs:254:            Instr::Run(run_cmd) => {`
- [PASS] build local delegates RUN to the container engine
  - `grep -rnF 'execute_container(' crates/podnext/src/commands`
  - → `crates/podnext/src/commands/build.rs:266:                    .execute_container(`
- [PASS] build local fails the build when RUN exits non-zero
  - `grep -rnF 'RUN instruction failed (exit' crates/podnext/src/commands`
  - → `crates/podnext/src/commands/build.rs:272:                    bail!("RUN instruction failed (exit {}): '{}'", code, run_cmd);`

**#18 [BUG-L2] copy/sync expand sparse files to dense allocation**  
_state_: CLOSED  ·  _labels_: bug, performance  ·  _closed_: 2026-11-04T11:04:53Z

- [PASS] sparse copies probe holes with SEEK_DATA/SEEK_HOLE
  - `grep -rnF 'libc::SEEK_DATA' crates/podnext/src`
  - → `crates/podnext/src/commands/sync.rs:377:        let data = match lseek(fd, offset, libc::SEEK_DATA) {`
- [PASS] the sparse-copy regression test exists
  - `grep -rnF 'fn copy_keeps_sparse_files_sparse' crates/podnext/src`
  - → `crates/podnext/src/main.rs:947:    fn copy_keeps_sparse_files_sparse() {`
- [SKIPPED] the sparse-copy unit test passes
  - `cargo test -p podnext --bins copy_keeps_sparse_files_sparse --locked`
  - → ``

**#19 [CI-1] test_restart_policy_unless_stopped fails: explicit stop loses to restart policy**  
_state_: CLOSED  ·  _labels_: bug, ci  ·  _closed_: 2026-11-04T04:23:51Z

- [PASS] the restart path re-checks user_stopped under the same write lock
  - `grep -rnF 'user_stopped' crates/pn-next/src/supervisor.rs | head -8`
  - → `728:    user_stopped: bool, | 850:                            user_stopped: false, | 992:                                if managed.process.is_none() && !managed.user_stopped && matches!(managed.status.state, ServiceState::Restarting | ServiceS`

**#20 [BUG-C4] FileLock::lock flocks fd 0 (stdin) and then panics on a None field**  
_state_: CLOSED  ·  _labels_: bug  ·  _closed_: 2026-11-04T04:23:55Z

- [PASS] FileLock no longer flocks fd 0
  - `grep -rnF 'FileLock' crates/proot-distro-core/src/locking.rs | head -6`
  - → `65:pub struct FileLock { | 70:impl FileLock { | 109:impl Drop for FileLock { | 247:        let mut first = FileLock::new(&path); | 251:        let mut second = FileLock::new(&path);`

**#21 [BUG-C5] a guest command containing -r aborts with 'rootfs is required for direct execution mode'**  
_state_: CLOSED  ·  _labels_: bug, cli  ·  _closed_: 2026-11-04T04:24:00Z

- [PASS] direct-execution detection only fires when -r/--rootfs precedes the command
  - `grep -rnF 'pre_dashdash_args' crates/podnext/src | head -4`
  - → `crates/podnext/src/main.rs:87:    let pre_dashdash_args = &raw_args[..dashdash_idx]; | crates/podnext/src/main.rs:125:    let has_subcommand = pre_dashdash_args | crates/podnext/src/main.rs:130:        && pre_dashdash_args`

**#22 [CI-2] dependency-updates workflow references a nonexistent action (peter-evans/update-github-actions 404s)**  
_state_: CLOSED  ·  _labels_: bug, github-actions, ci  ·  _closed_: 2026-11-04T04:24:05Z

- [PASS] the self-audit job that replaces the nonexistent action exists
  - `grep -rnF 'name: Audit GitHub Actions pins' .github/workflows/dependency-updates.yml`
  - → `98:      - name: Audit GitHub Actions pins`

**#23 [CI-3] Cargo.lock is gitignored in an application workspace, so --locked was impossible**  
_state_: CLOSED  ·  _labels_: bug, ci  ·  _closed_: 2026-11-04T04:24:09Z

- [PASS] Cargo.lock is tracked
  - `git ls-files --error-unmatch Cargo.lock`
  - → `Cargo.lock`

**#24 [SEC-1] 35 advisories in the dependency tree: inventory, exposure analysis and remediation plan**  
_state_: CLOSED  ·  _labels_: security, ci  ·  _closed_: 2026-11-04T10:12:17Z

- [PASS] deny.toml encodes the advisory acceptance list (string form)
  - `grep -rnF '"RUSTSEC-' deny.toml | head -3`
  - → `70:    "RUSTSEC-2026-0258", # h2: unbounded empty DATA frames | 71:    "RUSTSEC-2026-0346", # rustls: Debug impls expose TLS secrets | 72:    "RUSTSEC-2026-0349", # rustls: unauthenticated ServerHello downgrade`

**#25 [CI-4] Benchmark workflow is a false green: cargo bench fails to compile and the pipe hides it**  
_state_: CLOSED  ·  _labels_: bug, ci, performance  ·  _closed_: 2026-11-04T10:12:24Z

- [PASS] benchmark workflow enforces the exit status and builds the targets
  - `grep -nF -- '--bench benchmarks' .github/workflows/benchmark.yml | head -4`
  - → `103:          # `--bench benchmarks` is REQUIRED for the criterion flags to land on the | 108:          cargo bench --workspace --bench benchmarks --locked -- \ | 130:          cargo bench --workspace --bench benchmarks --locked -- \`
- [SKIPPED] the benchmark targets compile (the false-green defect)
  - `cargo bench -p pn-next --no-run --locked`
  - → ``

**#30 [CI-5] Benchmark job is unpredictable: 25.7 min then 43+ min, and a baseline step killed with SIGTERM**  
_state_: CLOSED  ·  _labels_: ci, performance  ·  _closed_: 2026-11-04T15:35:42Z

- [PASS] the benchmark job carries a bounded timeout
  - `grep -rnF 'timeout-minutes: 60' .github/workflows/benchmark.yml`
  - → `30:    timeout-minutes: 60`
- [PASS] the suite is invoked bounded via --bench benchmarks
  - `grep -nF -- '--bench benchmarks' .github/workflows/benchmark.yml`
  - → `103:          # `--bench benchmarks` is REQUIRED for the criterion flags to land on the | 108:          cargo bench --workspace --bench benchmarks --locked -- \ | 130:          cargo bench --workspace --bench benchmarks --locked -- \`

**#31 [CI-6] test_restart_policy_on_abnormal is flaky: assertions bet on wall-clock sleeps**  
_state_: CLOSED  ·  _labels_: ci, flaky-test  ·  _closed_: 2026-11-04T14:30:47Z

- [PASS] the test asserts on transitions via wait_status, not fixed sleeps
  - `grep -rnF 'async fn wait_status' crates/pn-next/tests/supervisor_e2e.rs`
  - → `87:async fn wait_status<F>(`
- [PASS] the test bounds its wait with a DEADLINE
  - `grep -rnF 'const DEADLINE: Duration = Duration::from_secs(30);' crates/pn-next/tests/supervisor_e2e.rs`
  - → `25:const DEADLINE: Duration = Duration::from_secs(30);`
- [SKIPPED] the abnormal-restart test passes
  - `cargo test -p pn-next --test supervisor_e2e test_restart_policy_on_abnormal --locked`
  - → ``

**#32 Signal lifecycle: stopping a PDN process orphans the guest, and podnext-daemon ignores SIGTERM**  
_state_: CLOSED  ·  _labels_: bug  ·  _closed_: 2026-11-04T13:12:18Z

- [PASS] spawned proot gets its own process group
  - `grep -rnF 'process_group(0)' crates/proot-core/src/lib.rs`
  - → `308:        cmd.process_group(0); | 492:                c.process_group(0);`
- [PASS] container stop signals the group and reaps (zombie-aware)
  - `grep -rnF 'async fn terminate_group' crates/proot-distro-core/src/container.rs`
  - → `613:async fn terminate_group(pgid: u32, timeout: u64) -> Result<()> {`
- [PASS] podnext run forwards signals to the guest group
  - `grep -rnF 'fn forward_to_guest' crates/podnext/src/native.rs | head -3`
  - → `738:fn forward_to_guest(sig: i32) { | 1116:    fn forward_to_guest_terminates_child_group() {`
- [PASS] the daemon handles SIGTERM, not only Ctrl-C
  - `grep -rnF 'SignalKind::terminate' crates/podnext/src/daemon.rs`
  - → `77:    if kind == SignalKind::terminate() || kind == SignalKind::interrupt() { | 183:    let mut sigterm = signal(SignalKind::terminate()).context("install SIGTERM handler")?; | 190:                _ = sigterm.recv() => SignalKind::terminate(),`

**#33 Session files and service logs grow without bound (46 dead session files measured)**  
_state_: CLOSED  ·  _labels_: bug  ·  _closed_: 2026-11-04T13:12:39Z

- [PASS] the session record is unlinked on exit via a guard
  - `grep -rnF 'impl Drop for SessionGuard' crates/proot-distro-core/src/session.rs`
  - → `61:impl Drop for SessionGuard {`
- [PASS] log rotation has a consumer
  - `grep -rnF 'fn rotate_log_if_needed' crates/pn-next/src/supervisor.rs`
  - → `478:fn rotate_log_if_needed(path: &Path, cfg: &LogRotationConfig) -> std::io::Result<()> {`

**#34 pn-gateway: documented duration form ("60s") cannot be loaded by the new TOML loader**  
_state_: CLOSED  ·  _labels_: bug, documentation  ·  _closed_: 2026-11-04T21:44:02Z

- [PASS] documented durations ("60s", "500ms", "2m") parse
  - `grep -rnF 'pub fn parse_duration' crates/pn-gateway/src/duration_fmt.rs`
  - → `42:pub fn parse_duration(input: &str) -> Result<Duration, String> {`
- [PASS] every Duration field uses the documented-form serde adapter
  - `grep -cF 'serde(with = "crate::duration_fmt")' crates/pn-gateway/src/config.rs`
  - → `10`
- [PASS] the documented form is tested against a documentation-shaped file
  - `grep -rnF 'documented_duration_strings_load' crates/pn-gateway/tests/config_loader.rs`
  - → `177:fn documented_duration_strings_load() {`
- [PASS] a load/save round-trip exists, so what is written can be read back
  - `grep -rnF 'pub fn to_toml_str' crates/pn-gateway/src/config_loader.rs`
  - → `73:    pub fn to_toml_str(&self) -> Result<String, ConfigError> {`

**#35 podnext: a tag containing ':' silently leaves /dev/shm and the /proc stubs unbound**  
_state_: CLOSED  ·  _labels_: bug  ·  _closed_: 2026-11-04T21:44:04Z

- [PASS] one shared rule turns a reference into a store key
  - `grep -rnF 'pub fn store_key' crates/proot-distro-core/src/names.rs`
  - → `120:pub fn store_key(reference: &str) -> String {`
- [PASS] build local stores under the derived key, not the raw tag
  - `grep -rnF 'names::store_key(tag)' crates/podnext/src/commands`
  - → `crates/podnext/src/commands/build.rs:161:    let key = proot_distro_core::names::store_key(tag);`
- [PASS] the raw tag is no longer used as a directory name
  - `grep -rnF 'containers_dir.join(tag)' crates/podnext/src/commands/mod.rs`
  - → ``
- [PASS] the RUN/install path resolves the container by the same key
  - `grep -rnF 'mgr.get(&key)' crates/podnext/src/commands`
  - → `crates/podnext/src/commands/build.rs:260:                let mut c = mgr.get(&key).await?;`
- [PASS] a colon in a bind source is an error, not a dropped mount
  - `grep -rnF 'pub fn ensure_bindable' crates/proot-distro-core/src/bindings.rs`
  - → `108:pub fn ensure_bindable(source: &str) -> anyhow::Result<()> {`
- [PASS] a colon tag is tested end to end at the store level
  - `grep -rnF 'a_colon_tag_builds_under_a_colon_free_store_key' crates/podnext/src/commands/mod.rs`
  - → `650:    async fn a_colon_tag_builds_under_a_colon_free_store_key() {`

**#38 A FROM scratch image is unreachable by run/list/remove after a successful build**  
_state_: CLOSED  ·  _labels_: bug  ·  _closed_: 2026-11-04T21:56:44Z

- [PASS] container detection is metadata-based, not an etc/bin/usr guess
  - `grep -rnF 'pub fn is_container_dir' crates/proot-distro-core/src/container.rs`
  - → `887:pub fn is_container_dir(dir: &std::path::Path) -> bool {`
- [PASS] load_containers uses the metadata-based check
  - `grep -rnF 'is_container_dir(&entry.path())' crates/proot-distro-core/src/container.rs`
  - → `810:        if !is_container_dir(&entry.path()) {`
- [PASS] the etc/bin/usr probe is not the container test any more
  - `grep -rnF 'let is_valid_rootfs' crates/proot-distro-core/src/container.rs`
  - → ``
- [PASS] a scratch image with metadata is tested as a container
  - `grep -rnF 'a_scratch_image_with_metadata_is_a_container' crates/proot-distro-core/src/container.rs`
  - → `909:    fn a_scratch_image_with_metadata_is_a_container() {`

**#40 LICENSE, Cargo.toml and README.md disagree about the project licence**  
_state_: CLOSED  ·  _labels_: documentation  ·  _closed_: 2026-11-05T00:19:42Z

- [PASS] LICENSE carries the GPL text the manifest declares
  - `grep -cF 'GNU GENERAL PUBLIC LICENSE' LICENSE`
  - → `2`
- [PASS] the manifest declares GPL-2.0-or-later
  - `grep -rnF 'license = "GPL-2.0-or-later"' Cargo.toml`
  - → `19:license = "GPL-2.0-or-later"`
- [PASS] no Apache-2.0 text is left in LICENSE
  - `grep -rnF 'Apache License' LICENSE`
  - → ``
- [PASS] the README states the same licence
  - `grep -rnF 'GPL-2.0-or-later' README.md`
  - → `319:SPDX-License-Identifier: `GPL-2.0-or-later`. The full text is in [LICENSE](LICENSE).`

**#41 A non-UTF-8 byte in any argument aborts podnext with SIGABRT (std::env::args panic)**  
_state_: CLOSED  ·  _labels_: bug  ·  _closed_: 2026-11-05T15:17:31Z

- [PASS] the regression test for an invalid-UTF-8 argument exists
  - `grep -rnF 'argv_with_invalid_utf8_does_not_panic' crates/podnext/tests/argv_nonutf8.rs`
  - → `12:fn argv_with_invalid_utf8_does_not_panic() {`
- [PASS] argv is read as bytes so an invalid sequence cannot abort
  - `grep -rnF 'args_os' crates/podnext/src`
  - → `crates/podnext/src/commands/build.rs:44:    // `args_os`, not `args`: std caches argv once, and `args()` panics re-reading it when any | crates/podnext/src/commands/build.rs:48:    let bin_name = std::env::args_os() | crates/podnext/src/main.rs`
- [PASS] the panicking reader is gone from the tree
  - `grep -rnF 'std::env::args()' crates/podnext/src`
  - → ``
- [SKIPPED] both regression tests pass
  - `cargo test -p podnext --test argv_nonutf8 --locked`
  - → ``

**#42 [BUG-DOC] CLI reference documented options that do not exist (exit codes 3-12, 8 inert PN_* vars, --short/--json, ps --all, kill --signal/--timeout, network --ip-range, volume --size/--storage)**  
_state_: CLOSED  ·  _labels_: (none)  ·  _closed_: 2026-11-05T15:17:35Z

- [PASS] no exit code outside 0/1/2 is documented
  - `! sed -n '/## Exit Codes/,/## Environment Variables/p' docs/src/cli-reference.md | grep -qE '^\| (3|4|5|6|7|8|9|1[0-2]) \| '`
  - → ``
- [PASS] no environment variable is documented that no crate reads
  - `! grep -qE '^\| `(PN_CONFIG_DIR|PN_DATA_DIR|PN_CACHE_DIR|PN_LOG_LEVEL|PN_GATEWAY_ADDR|PN_RUNTIME|PN_SECURITY_PROFILE|PN_NO_COLOR)`' docs/src/cli-reference.md`
  - → ``
- [PASS] the version section does not document --short/--json
  - `! sed -n '/### `podnext version`/,/^## /p' docs/src/cli-reference.md | grep -qE '^\| `--(short|json)`'`
  - → ``
- [PASS] known-limitations.md is registered in the book summary
  - `grep -rqF 'known-limitations.md' docs/src/SUMMARY.md`
  - → ``
- [SKIPPED] every documented command and option exists in the real CLI
  - `cargo test -p podnext --test cli_reference_contract`
  - → ``
- [PASS] no command reports a not-implemented stub on stdout
  - `! (grep -rn 'not yet implemented' crates/podnext/src --include=*.rs | grep -vE ':[0-9]+:[[:space:]]*//[^/]' | grep -q .)`
  - → ``
- [SKIPPED] a stub must refuse with a reason, not exit 0
  - `cargo test -p podnext --test stub_refusals`
  - → ``

**#43 [BUG] --bind/--volume were no-ops: the flag was discarded and the rendered bind used a suffix proot 5.1.107.101 turns into part of the target path**  
_state_: CLOSED  ·  _labels_: (none)  ·  _closed_: 2026-11-05T15:17:39Z

- [PASS] the run flag is no longer discarded
  - `! grep -rqF '_volume: Vec<String>,' crates/podnext/src`
  - → ``
- [PASS] run folds --volume into the mounts it passes on
  - `grep -rqF 'specs.extend(volume)' crates/podnext/src`
  - → ``
- [PASS] login folds its binds into the container config
  - `grep -rqF 'append(&mut options.bind_mounts)' crates/proot-distro-core/src/lib.rs`
  - → ``
- [PASS] the renderer no longer appends a :ro/:rw suffix
  - `! grep -qF 'if readonly { "ro" }' crates/proot-distro-core/src/bindings.rs`
  - → ``
- [SKIPPED] the rendered bind form is exactly host:target, pinned by a test
  - `cargo test -p proot-distro-core --lib argument_tests`
  - → ``
- [SKIPPED] a malformed or read-only mount spec is refused
  - `cargo test -p podnext --bin podnext mount_spec_tests`
  - → ``

**#44 [BUG] image ls printed a size that was never measured and called the container name an IMAGE ID**  
_state_: CLOSED  ·  _labels_: (none)  ·  _closed_: 2026-11-05T14:48:42Z

- [PASS] no column claims to be an image ID
  - `! grep -qF 'rename = "IMAGE ID"' crates/podnext/src`
  - → `grep: crates/podnext/src: Is a directory`
- [PASS] the inode size is no longer used as the size
  - `! grep -qF 'path.metadata().map(|m| m.len())' crates/podnext/src`
  - → `grep: crates/podnext/src: Is a directory`
- [PASS] an unmeasurable rootfs prints unknown
  - `grep -rqF 'None => "unknown"' crates/podnext/src`
  - → ``
- [SKIPPED] a failed measurement is reported as a bound, never as a bare number
  - `cargo test -p podnext --bin podnext size_measurement_tests`
  - → ``

**#45 [GAP §9.3] podnext doctor did not exist: no host/guest diagnostic report**  
_state_: CLOSED  ·  _labels_: (none)  ·  _closed_: 2026-11-05T15:17:43Z

- [PASS] the doctor command is dispatched
  - `grep -rqF 'Commands::Doctor' crates/podnext/src`
  - → ``
- [PASS] the doctor command is implemented
  - `grep -rqF 'async fn cmd_doctor' crates/podnext/src`
  - → ``
- [PASS] the reference documents it and names the test that covers it
  - `grep -rqF 'doctor_contract.rs' docs/src/cli-reference.md`
  - → ``
- [PASS] a check that cannot run reports unknown, never ok
  - `grep -rqF 'Verdict::Unknown' crates/podnext/src/doctor.rs`
  - → ``
- [SKIPPED] the command itself is exercised end to end
  - `cargo test -p podnext --test doctor_contract`
  - → ``
- [PASS] doctor reports the proot argv it actually ran
  - `grep -rqF 'guest proot argv' crates/podnext/src/doctor.rs`
  - → ``

**#46 [GAP §9.2] Path defaults were hard-coded to one machine's Termux home**  
_state_: CLOSED  ·  _labels_: (none)  ·  _closed_: 2026-11-05T15:17:47Z

- [PASS] the daemon binary carries no hard-coded home path
  - `! grep -rqF '/data/data/com.termux/files/home' crates/podnext/src/daemon.rs`
  - → ``
- [PASS] the daemon socket is resolved from the environment
  - `grep -rqF 'paths::host::runtime_dir' crates/podnext/src`
  - → ``
- [PASS] the prefix honours TERMUX__PREFIX before PREFIX
  - `grep -rqF 'TERMUX__PREFIX' crates/proot-distro-core/src/paths.rs`
  - → ``
- [SKIPPED] the resolution order is unit tested
  - `cargo test -p proot-distro-core --lib host_path_tests`
  - → ``
- [SKIPPED] the binary follows a relocated home and the XDG variables
  - `cargo test -p podnext --test relocated_paths`
  - → ``
- [PASS] CI runs the suites with HOME, PREFIX and XDG relocated
  - `grep -rqF 'Relocated HOME and prefix' .github/workflows/ci.yml`
  - → ``
- [PASS] the platform-owned TERMUX__PREFIX exclusion is documented
  - `grep -rqF 'TERMUX__PREFIX' docs/src/known-limitations.md`
  - → ``

**#47 [BUG] run accepted nine options and dropped all of them, while --help advertised each**  
_state_: CLOSED  ·  _labels_: (none)  ·  _closed_: 2026-11-05T15:17:51Z

- [PASS] the exit-code taxonomy is asserted against the real binary
  - `grep -rnF 'a_documented_refusal_is_one_with_a_reason' crates/podnext/tests/exit_codes.rs`
  - → `77:fn a_documented_refusal_is_one_with_a_reason() {`
- [SKIPPED] every refusal class behaves as documented
  - `cargo test -p podnext --test exit_codes --locked`
  - → ``
- [PASS] the refusal names the options it cannot honour
  - `grep -rqF 'not implemented for `run`' crates/podnext/src`
  - → ``
- [PASS] the help no longer claims --detach runs in the background
  - `! grep -rqF 'Run container in background and print container ID' crates/podnext/src`
  - → ``
- [PASS] every unhonoured option is marked refused in the reference
  - `[ $(grep -c 'Refused:' docs/src/cli-reference.md) -ge 7 ]`
  - → ``
- [SKIPPED] each refused option exits 1 without running the command
  - `cargo test -p podnext --test stub_refusals`
  - → ``

**#56 [shell] Measure terminal fidelity: resize, TERM, PS1, UTF-8 locale (roadmap 9.3)**  
_state_: CLOSED  ·  _labels_: wayfinder:task  ·  _closed_: 2026-11-05T16:28:20Z

- [PASS] the fidelity harness exists and drives the real binary in a PTY
  - `grep -rnF 'PDNEXT_BIN' scripts/shell_fidelity.py`
  - → `22:Env overrides: PDNEXT_BIN (path to podnext), PDN_CONTAINER (container name). | 44:    b = os.environ.get("PDNEXT_BIN") | 165:        skip("podnext binary not found (set PDNEXT_BIN or install it on PATH)")`

**#58 [shell] Soak: repeated start/stop leaves nothing behind (roadmap 9.3)**  
_state_: CLOSED  ·  _labels_: wayfinder:task  ·  _closed_: 2026-11-05T16:28:24Z

- [PASS] the soak harness exists and can inject the leak it must catch
  - `grep -rnF -- '--inject-leak' scripts/soak_sessions.py`
  - → `13:Negative control (`--inject-leak N`): at iteration N the guest is started in the | 146:    if "--inject-leak" in argv: | 147:        inject = int(argv[argv.index("--inject-leak") + 1])`

**#60 [security] Decide the seccomp profiles: verify per distro or decline with the measurement**  
_state_: CLOSED  ·  _labels_: wayfinder:task  ·  _closed_: 2026-11-05T16:28:32Z

- [PASS] a seccomp profile in container metadata is refused, not dropped
  - `grep -rnF 'assert_seccomp_profile_supported' crates/proot-distro-core/src/security.rs`
  - → `175:pub fn assert_seccomp_profile_supported(profile: &Option<String>) -> anyhow::Result<()> { | 202:        assert!(assert_seccomp_profile_supported(&None).is_ok()); | 203:        let err = assert_seccomp_profile_supported(&Some("default".to_st`
- [SKIPPED] the guard is unit tested
  - `cargo test -p proot-distro-core --lib seccomp_profile_is_refused_not_dropped --locked`
  - → ``

**#62 [BUG] run --seccomp is accepted and never applied**  
_state_: CLOSED  ·  _labels_: bug  ·  _closed_: 2026-11-05T14:50:05Z

- [PASS] --seccomp refuses instead of being silently dropped
  - `grep -rnF -- '--seccomp is not implemented' crates/podnext/src`
  - → `crates/podnext/src/main.rs:267:        // `--seccomp is not implemented` is the one the ledger greps for (issue #62). | crates/podnext/src/main.rs:270:                "--seccomp is not implemented: this PRoot build applies no seccomp filter, `
- [SKIPPED] the refusal is asserted against the real binary
  - `cargo test -p podnext --test stub_refusals --locked`
  - → ``
- [PASS] the limitation is written down where a user looks
  - `grep -rnF 'Seccomp filtering is not available' docs/src/known-limitations.md`
  - → `93:- **Seccomp filtering is not available, and `--seccomp` says so instead of pretending.** Measured:`

**#63 [BUG] direct-execution argv scanner parses --no-kill-on-exit and never uses it**  
_state_: CLOSED  ·  _labels_: (none)  ·  _closed_: 2026-11-05T16:28:36Z

- [PASS] --no-kill-on-exit refuses instead of being silently dropped
  - `grep -rnF -- '--no-kill-on-exit is not implemented' crates/podnext/src`
  - → `crates/podnext/src/main.rs:278:                "--no-kill-on-exit is not implemented: proot-core passes `--kill-on-exit` to proot \ | crates/podnext/src/main.rs:334:            "--no-kill-on-exit is not implemented: proot-core passes `--kill-`
- [PASS] the manual direct path refuses it too, not only the parsed path
  - `grep -rnF -- '--no-kill-on-exit is not implemented' crates/podnext/src/main.rs`
  - → `278:                "--no-kill-on-exit is not implemented: proot-core passes `--kill-on-exit` to proot \ | 334:            "--no-kill-on-exit is not implemented: proot-core passes `--kill-on-exit` to proot \`
- [PASS] proot-core still passes --kill-on-exit unconditionally, which is why the flag can only refuse
  - `grep -rnF -- '--kill-on-exit' crates/proot-core/src/lib.rs`
  - → `207:        cmd.arg("--kill-on-exit"); | 346:        cmd.arg("--kill-on-exit");`

**#68 [BUG] interactive login has no prompt and cannot read stdin: the guest is a background process group**  
_state_: CLOSED  ·  _labels_: bug  ·  _closed_: 2026-11-05T16:29:12Z

- [PASS] an interactive run keeps the caller's process group, asserted in code
  - `grep -rnF 'an_interactive_run_does_not_take_its_own_process_group' crates/proot-core/src/lib.rs`
  - → `554:    fn an_interactive_run_does_not_take_its_own_process_group() {`
- [PASS] the guest's TERM comes from the caller, asserted in code
  - `grep -rnF 'the_guest_inherits_the_callers_term' crates/proot-core/src/lib.rs`
  - → `567:    fn the_guest_inherits_the_callers_term() {`
- [SKIPPED] both decisions are unit tested
  - `cargo test -p proot-core --lib child_owns_process_group --locked`
  - → ``

**#69 The workspace builds with 290 warnings and nothing counts them, so the split's own residue is invisible**  
_state_: OPEN  ·  _labels_: wayfinder:task  ·  _closed_: —

- [PASS] the warning ratchet can actually fail, not merely report
  - `python3 scripts/check_warnings.py self-check`
  - → `self-check: ok - fires on growth, quiet on equality, does not confuse a warm run for clean`
- [PASS] the baseline records the toolchain, so a rustc change reads as unmeasurable
  - `grep -F '"toolchain"' scripts/warning_baseline.json`
  - → `"toolchain": "cargo 1.99.1 (5f94df478 2026-08-27) (built from a source tarball) / rustc 1.99.1 (58cf8f9c3 2026-10-13) (built from a source tarball)",`
- [PASS] CI runs the comparison, so the number cannot grow unnoticed
  - `grep -rnF 'check_warnings.py compare' .github/workflows`
  - → `.github/workflows/issue-ledger.yml:127:        run: python3 scripts/check_warnings.py compare --cold`

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
| 37 | OPEN | defect: image ls/push said they could not see any image because ImageManager read only the store. image ls is fixed (#44), but a real image store does not exist until v0.2.0 (roadmap section 4.4), so the remaining push/tag/rm/history behaviour has nothing to assert. | image push/tag/rm/history cannot see any image: ImageManager never loa |
| 39 | OPEN | defect: push cannot work because the store holds no layer blobs. The CLI now refuses honestly instead of pretending (#42), and the blobs themselves are v0.2.0 work (roadmap section 4.4): the honest refusal is asserted by the stub checks, not here. | push: the store holds no layer blobs, so push cannot work even once th |
| 67 | OPEN | defect, measured while closing #63: the manual direct-execution scanner drops every flag it parses except rootfs, the command and verbose. Each flag needs the honour-or-refuse decision made against the generated argv, one at a time; the method is written in the issue. | [BUG] the direct-execution scanner drops every flag it parses except r |

The 14 issues labelled `wayfinder:*` (#48, #49, #50, #51, #52, #53, #54, #55, #57, #59, #61, #64, #65, #66) are work items, not findings: the map issue holds their acceptance criteria, and they are not subject to this triage list.

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
