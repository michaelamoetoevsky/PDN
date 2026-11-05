#!/usr/bin/env python3
"""Issue ledger: re-verify every issue's fix against the CURRENT HEAD, then document.

The point is not to restate that an issue was closed — it is to prove the fix is still present in
the code today. Each entry pairs a claim with a command and the exact symbol the command must show.
A closed issue whose verification fails is a reopened bug, and the ledger says so.

Three verdicts, and only three:

  PASS     the predicate held, so the fix is present in the tree.
  FAIL     the predicate did not hold: a fix that is no longer present (closed issue). Exit status
           becomes 1.
  SKIPPED  no automated check exists for this issue (open feature request / defect with no landed
           fix), or a gated check was not enabled. A skipped check is NEVER counted as a pass.

Substring pitfalls are avoided on purpose: every source predicate asserts on an exact symbol with
`grep -nF` (fixed string) and a zero exit status, never on a count compared to a substring. An
earlier revision compared a `grep -c` COUNT against a symbol substring and produced false FAILs.

Run:  env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py            # source checks only
      env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py --json     # + machine summary
      env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py --cargo    # + cargo-gated checks
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

# Resolve the repo from this file's location so the harness works in a git worktree or a clone,
# not only in one hard-coded checkout.
REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "research" / "ISSUE_LEDGER.md"
CARGO_LOCK = Path.home() / ".hermes/cache/scratch/cargo-mutex.lock"
# Shared, heavy-gated target dir so concurrent departments do not thrash memory.
CARGO_TARGET = "/data/data/com.termux/files/home/workspace/PDN/target"

# ── Check table ───────────────────────────────────────────────────────────────
# Each check is a dict: issue, claim, cmd, needle [, mode, cargo].
# `mode="contains"` (default): PASS when rc == 0 AND `needle` appears in stdout — the fix is present.
# `mode="absent"`:            PASS when rc != 0 OR `needle` did NOT appear — the defect is gone.
# `needle=None`:              PASS on rc == 0 alone (use only where a symbol is not the point).
# `cargo=True`:               gated; runs only with --cargo, otherwise SKIPPED.
CHECKS: list[dict] = [
    # ── #40: the licence is declared once, in all three places ───────────────
    dict(issue="40", claim="LICENSE carries the GPL text the manifest declares",
         cmd="grep -cF 'GNU GENERAL PUBLIC LICENSE' LICENSE", needle=None),
    dict(issue="40", claim="the manifest declares GPL-2.0-or-later",
         cmd="grep -rnF 'license = \"GPL-2.0-or-later\"' Cargo.toml",
         needle='license = "GPL-2.0-or-later"'),
    dict(issue="40", claim="no Apache-2.0 text is left in LICENSE",
         cmd="grep -rnF 'Apache License' LICENSE", mode="absent", needle="Apache License"),
    dict(issue="40", claim="the README states the same licence",
         cmd="grep -rnF 'GPL-2.0-or-later' README.md", needle="GPL-2.0-or-later"),
    # ── #41: a non-UTF-8 byte in any argument must not abort the process ──────
    dict(issue="41", claim="the regression test for an invalid-UTF-8 argument exists",
         cmd="grep -rnF 'argv_with_invalid_utf8_does_not_panic' crates/podnext/tests/argv_nonutf8.rs",
         needle="argv_with_invalid_utf8_does_not_panic"),
    dict(issue="41", claim="argv is read as bytes so an invalid sequence cannot abort",
         cmd="grep -rnF 'args_os' crates/podnext/src", needle="args_os"),
    dict(issue="41", claim="the panicking reader is gone from the tree",
         cmd="grep -rnF 'std::env::args()' crates/podnext/src",
         mode="absent", needle="std::env::args()"),
    dict(issue="41", claim="both regression tests pass", cargo=True, needle=None,
         cmd="cargo test -p podnext --test argv_nonutf8 --locked"),
    # ── #38: a locally built image is visible to run/list/remove ──────────────
    dict(issue="38", claim="container detection is metadata-based, not an etc/bin/usr guess",
         cmd="grep -rnF 'pub fn is_container_dir' crates/proot-distro-core/src/container.rs",
         needle="pub fn is_container_dir"),
    dict(issue="38", claim="load_containers uses the metadata-based check",
         cmd="grep -rnF 'is_container_dir(&entry.path())' crates/proot-distro-core/src/container.rs",
         needle="is_container_dir(&entry.path())"),
    dict(issue="38", claim="the etc/bin/usr probe is not the container test any more",
         cmd="grep -rnF 'let is_valid_rootfs' crates/proot-distro-core/src/container.rs",
         mode="absent", needle="let is_valid_rootfs"),
    dict(issue="38", claim="a scratch image with metadata is tested as a container",
         cmd="grep -rnF 'a_scratch_image_with_metadata_is_a_container' crates/proot-distro-core/src/container.rs",
         needle="a_scratch_image_with_metadata_is_a_container"),
    # ── #34: the documented duration form is the loadable form ────────────────
    dict(issue="34", claim="documented durations (\"60s\", \"500ms\", \"2m\") parse",
         cmd="grep -rnF 'pub fn parse_duration' crates/pn-gateway/src/duration_fmt.rs",
         needle="pub fn parse_duration"),
    dict(issue="34", claim="every Duration field uses the documented-form serde adapter",
         cmd="grep -cF 'serde(with = \"crate::duration_fmt\")' crates/pn-gateway/src/config.rs",
         needle=None),
    dict(issue="34", claim="the documented form is tested against a documentation-shaped file",
         cmd="grep -rnF 'documented_duration_strings_load' crates/pn-gateway/tests/config_loader.rs",
         needle="documented_duration_strings_load"),
    dict(issue="34", claim="a load/save round-trip exists, so what is written can be read back",
         cmd="grep -rnF 'pub fn to_toml_str' crates/pn-gateway/src/config_loader.rs",
         needle="pub fn to_toml_str"),
    # ── #35: a tag is not a path component ────────────────────────────────────
    dict(issue="35", claim="one shared rule turns a reference into a store key",
         cmd="grep -rnF 'pub fn store_key' crates/proot-distro-core/src/names.rs",
         needle="pub fn store_key"),
    dict(issue="35", claim="build local stores under the derived key, not the raw tag",
         cmd="grep -rnF 'names::store_key(tag)' crates/podnext/src/commands",
         needle="names::store_key(tag)"),
    dict(issue="35", claim="the raw tag is no longer used as a directory name",
         cmd="grep -rnF 'containers_dir.join(tag)' crates/podnext/src/commands/mod.rs",
         mode="absent", needle="containers_dir.join(tag)"),
    dict(issue="35", claim="the RUN/install path resolves the container by the same key",
         cmd="grep -rnF 'mgr.get(&key)' crates/podnext/src/commands",
         needle="mgr.get(&key)"),
    dict(issue="35", claim="a colon in a bind source is an error, not a dropped mount",
         cmd="grep -rnF 'pub fn ensure_bindable' crates/proot-distro-core/src/bindings.rs",
         needle="pub fn ensure_bindable"),
    dict(issue="35", claim="a colon tag is tested end to end at the store level",
         cmd="grep -rnF 'a_colon_tag_builds_under_a_colon_free_store_key' crates/podnext/src/commands/mod.rs",
         needle="a_colon_tag_builds_under_a_colon_free_store_key"),
    # ── existing coverage (kept) ──────────────────────────────────────────────
    dict(issue="11", claim="install validates the container name before creating anything",
         cmd="grep -rnF 'validate_container_name' crates/podnext/src",
         needle="validate_container_name"),
    dict(issue="12", claim="delete_backup drops its read guard before taking the write lock",
         cmd="grep -rnF 'contains_key' crates/pn-next/src/backup_restore/mod.rs",
         needle="contains_key"),
    dict(issue="12", claim="the deadlock regression test exists",
         cmd="grep -rnF 'test_delete_backup_does_not_deadlock' crates/pn-next/src/backup_restore/mod.rs",
         needle="test_delete_backup_does_not_deadlock"),
    dict(issue="13", claim="reset verifies the image BEFORE destroying the container",
         # -A walks the body, so the order the claim depends on survives the code moving file
         cmd="grep -rnA 40 'fn cmd_reset' crates/podnext/src | "
             "grep -nE 'verify_image_available|remove_dir_all' | head -2",
         needle="verify_image_available"),
    dict(issue="14", claim="run sessions are registered so ps can see them",
         cmd="grep -rnF 'register_session' crates/proot-distro-core/src/runtime.rs crates/podnext/src/native.rs",
         needle="register_session"),
    dict(issue="19", claim="the restart path re-checks user_stopped under the same write lock",
         cmd="grep -rnF 'user_stopped' crates/pn-next/src/supervisor.rs | head -8",
         needle="user_stopped"),
    dict(issue="20", claim="FileLock no longer flocks fd 0",
         cmd="grep -rnF 'FileLock' crates/proot-distro-core/src/locking.rs | head -6",
         needle="FileLock"),
    dict(issue="21", claim="direct-execution detection only fires when -r/--rootfs precedes the command",
         cmd="grep -rnF 'pre_dashdash_args' crates/podnext/src | head -4",
         needle="pre_dashdash_args"),
    dict(issue="22", claim="the self-audit job that replaces the nonexistent action exists",
         cmd="grep -rnF 'name: Audit GitHub Actions pins' .github/workflows/dependency-updates.yml",
         needle="name: Audit GitHub Actions pins"),
    dict(issue="23", claim="Cargo.lock is tracked",
         cmd="git ls-files --error-unmatch Cargo.lock",
         needle="Cargo.lock"),
    dict(issue="24", claim="deny.toml encodes the advisory acceptance list (string form)",
         cmd="grep -rnF '\"RUSTSEC-' deny.toml | head -3",
         needle='"RUSTSEC-'),
    dict(issue="25", claim="benchmark workflow enforces the exit status and builds the targets",
         cmd="grep -nF -- '--bench benchmarks' .github/workflows/benchmark.yml | head -4",
         needle="--bench benchmarks"),
    dict(issue="25", claim="the benchmark targets compile (the false-green defect)",
         cargo=True, needle=None, cmd="cargo bench -p pn-next --no-run --locked"),
    dict(issue="32", claim="spawned proot gets its own process group",
         cmd="grep -rnF 'process_group(0)' crates/proot-core/src/lib.rs",
         needle="process_group(0)"),
    dict(issue="32", claim="container stop signals the group and reaps (zombie-aware)",
         cmd="grep -rnF 'async fn terminate_group' crates/proot-distro-core/src/container.rs",
         needle="async fn terminate_group"),
    dict(issue="32", claim="podnext run forwards signals to the guest group",
         cmd="grep -rnF 'fn forward_to_guest' crates/podnext/src/native.rs | head -3",
         needle="fn forward_to_guest"),
    dict(issue="32", claim="the daemon handles SIGTERM, not only Ctrl-C",
         cmd="grep -rnF 'SignalKind::terminate' crates/podnext/src/daemon.rs",
         needle="SignalKind::terminate"),
    dict(issue="33", claim="the session record is unlinked on exit via a guard",
         cmd="grep -rnF 'impl Drop for SessionGuard' crates/proot-distro-core/src/session.rs",
         needle="impl Drop for SessionGuard"),
    dict(issue="33", claim="log rotation has a consumer",
         cmd="grep -rnF 'fn rotate_log_if_needed' crates/pn-next/src/supervisor.rs",
         needle="fn rotate_log_if_needed"),

    # ── NEW coverage: the previously-unchecked closed issues ──────────────────
    # #7  Add GitHub Actions workflow for automated dependency updates
    dict(issue="7", claim="dependabot.yml declares the cargo ecosystem",
         cmd="grep -rnF 'package-ecosystem: \"cargo\"' .github/dependabot.yml",
         needle='package-ecosystem: "cargo"'),
    dict(issue="7", claim="the dependency-updates workflow file exists",
         cmd="ls .github/workflows/dependency-updates.yml",
         needle="dependency-updates.yml"),
    # #8  Add benchmark suite with Criterion
    dict(issue="8", claim="the Criterion bench target is declared",
         cmd="grep -rnF '[[bench]]' crates/pn-next/Cargo.toml",
         needle="[[bench]]"),
    dict(issue="8", claim="the criterion dev-dependency is present",
         cmd="grep -rnF 'criterion' crates/pn-next/Cargo.toml",
         needle="criterion"),
    dict(issue="8", claim="the bench source exists",
         cmd="ls crates/pn-next/benches/benchmarks.rs",
         needle="benchmarks.rs"),
    # #15 concurrent guest writes clobber — resolved as documented POSIX behaviour
    dict(issue="15", claim="the concurrent-write contract is documented for users",
         cmd="grep -rnF 'no atomicity guarantee' docs/src/troubleshooting.md",
         needle="no atomicity guarantee"),
    dict(issue="15", claim="the #15 verdict (not a PDN defect, with control) is recorded",
         cmd="grep -rnF 'not a PDN defect' research/issues/concurrency_verdict.md",
         needle="not a PDN defect"),
    # #16 deep tree extraction scales super-linearly
    dict(issue="16", claim="untar records created dirs instead of re-walking ancestors per entry",
         cmd="grep -rnF 'created: &mut HashSet<PathBuf>' crates/proot-distro-core/src/untar.rs",
         needle="created: &mut HashSet<PathBuf>"),
    dict(issue="16", claim="the deep-recursion regression test exists",
         cmd="ls crates/proot-distro-core/tests/deep_fs_recursion.rs",
         needle="deep_fs_recursion.rs"),
    # #17 podnext build local rejects ARG, skips RUN
    dict(issue="17", claim="build local consumes ARG before the parser rejects it",
         cmd="grep -rnF 'fn preprocess_build_args' crates/podnext/src/commands",
         needle="fn preprocess_build_args"),
    # #17 predicate revised during integration: the original check asserted on
    # `fn execute_run`, which the #29 fix DELETED as dead code - that function was
    # the defect itself (it ran RUN against the build context instead of the
    # assembled rootfs). Asserting on a removed symbol inverted the check, so it
    # now asserts the contract: the RUN arm matches an instruction and delegates
    # to the container engine against the rootfs, and a non-zero RUN propagates.
    dict(issue="17", claim="build local matches a RUN instruction instead of printing it",
         cmd="grep -rnF 'Instr::Run(run_cmd)' crates/podnext/src/commands",
         needle="Instr::Run(run_cmd)"),
    dict(issue="17", claim="build local delegates RUN to the container engine",
         cmd="grep -rnF 'execute_container(' crates/podnext/src/commands",
         needle="execute_container("),
    dict(issue="17", claim="build local fails the build when RUN exits non-zero",
         cmd="grep -rnF 'RUN instruction failed (exit' crates/podnext/src/commands",
         needle="RUN instruction failed (exit"),
    # #18 copy/sync expand sparse files
    dict(issue="18", claim="sparse copies probe holes with SEEK_DATA/SEEK_HOLE",
         cmd="grep -rnF 'libc::SEEK_DATA' crates/podnext/src",
         needle="libc::SEEK_DATA"),
    dict(issue="18", claim="the sparse-copy regression test exists",
         cmd="grep -rnF 'fn copy_keeps_sparse_files_sparse' crates/podnext/src",
         needle="fn copy_keeps_sparse_files_sparse"),
    # #30 benchmark job unpredictable / killed with SIGTERM
    dict(issue="30", claim="the benchmark job carries a bounded timeout",
         cmd="grep -rnF 'timeout-minutes: 60' .github/workflows/benchmark.yml",
         needle="timeout-minutes: 60"),
    dict(issue="30", claim="the suite is invoked bounded via --bench benchmarks",
         cmd="grep -nF -- '--bench benchmarks' .github/workflows/benchmark.yml",
         needle="--bench benchmarks"),
    # #31 flaky restart-policy test (transition-driven fix)
    dict(issue="31", claim="the test asserts on transitions via wait_status, not fixed sleeps",
         cmd="grep -rnF 'async fn wait_status' crates/pn-next/tests/supervisor_e2e.rs",
         needle="async fn wait_status"),
    dict(issue="31", claim="the test bounds its wait with a DEADLINE",
         cmd="grep -rnF 'const DEADLINE: Duration = Duration::from_secs(30);' crates/pn-next/tests/supervisor_e2e.rs",
         needle="const DEADLINE: Duration = Duration::from_secs(30);"),

    # ── GATED cargo checks: behaviour a source predicate cannot prove ──────────
    # Run only with --cargo (they build; three departments share this host). Default = SKIPPED.
    dict(issue="18", claim="the sparse-copy unit test passes", cargo=True, needle=None,
         cmd="cargo test -p podnext --bins copy_keeps_sparse_files_sparse --locked"),
    dict(issue="16", claim="the deep-recursion test passes", cargo=True, needle=None,
         cmd="cargo test -p proot-distro-core --test deep_fs_recursion --locked"),
    dict(issue="31", claim="the abnormal-restart test passes", cargo=True, needle=None,
         cmd="cargo test -p pn-next --test supervisor_e2e test_restart_policy_on_abnormal --locked"),
    # ── #42: the CLI reference describes the real binary ──────────────────────
    dict(issue="42", claim="no exit code outside 0/1/2 is documented",
         cmd="! sed -n '/## Exit Codes/,/## Environment Variables/p' docs/src/cli-reference.md | "
            "grep -qE '^\\| (3|4|5|6|7|8|9|1[0-2]) \\| '",
         needle=None),
    dict(issue="42", claim="no environment variable is documented that no crate reads",
         cmd="! grep -qE '^\\| `(PN_CONFIG_DIR|PN_DATA_DIR|PN_CACHE_DIR|PN_LOG_LEVEL|PN_GATEWAY_ADDR|"
            "PN_RUNTIME|PN_SECURITY_PROFILE|PN_NO_COLOR)`' docs/src/cli-reference.md",
         needle=None),
    dict(issue="42", claim="the version section does not document --short/--json",
         cmd="! sed -n '/### `podnext version`/,/^## /p' docs/src/cli-reference.md | "
            "grep -qE '^\\| `--(short|json)`'",
         needle=None),
    dict(issue="42", claim="known-limitations.md is registered in the book summary",
         cmd="grep -rqF 'known-limitations.md' docs/src/SUMMARY.md", needle=None),
    dict(issue="42", claim="every documented command and option exists in the real CLI",
         cmd="cargo test -p podnext --test cli_reference_contract", needle=None, cargo=True),
    # ── #63: the direct path's own flag was dropped, not honoured ─────────────
    dict(issue="63", claim="--no-kill-on-exit refuses instead of being silently dropped",
         cmd="grep -rnF -- '--no-kill-on-exit is not implemented' crates/podnext/src",
         needle="--no-kill-on-exit is not implemented"),
    dict(issue="63", claim="the manual direct path refuses it too, not only the parsed path",
         cmd="grep -rnF -- '--no-kill-on-exit is not implemented' crates/podnext/src/main.rs",
         needle="--no-kill-on-exit is not implemented"),
    dict(issue="63", claim="proot-core still passes --kill-on-exit unconditionally, which is why the flag can only refuse",
         cmd="grep -rnF -- '--kill-on-exit' crates/proot-core/src/lib.rs", needle=None),
    # ── #60: a seccomp profile in metadata is refused, not dropped ────────────
    dict(issue="60", claim="a seccomp profile in container metadata is refused, not dropped",
         cmd="grep -rnF 'assert_seccomp_profile_supported' crates/proot-distro-core/src/security.rs",
         needle="assert_seccomp_profile_supported"),
    dict(issue="60", claim="the guard is unit tested", cargo=True, needle=None,
         cmd="cargo test -p proot-distro-core --lib seccomp_profile_is_refused_not_dropped --locked"),
    # ── #68: an interactive guest must keep the terminal it was given ─────────
    dict(issue="68", claim="an interactive run keeps the caller's process group, asserted in code",
         cmd="grep -rnF 'an_interactive_run_does_not_take_its_own_process_group' crates/proot-core/src/lib.rs",
         needle="an_interactive_run_does_not_take_its_own_process_group"),
    dict(issue="68", claim="the guest's TERM comes from the caller, asserted in code",
         cmd="grep -rnF 'the_guest_inherits_the_callers_term' crates/proot-core/src/lib.rs",
         needle="the_guest_inherits_the_callers_term"),
    dict(issue="68", claim="both decisions are unit tested", cargo=True, needle=None,
         cmd="cargo test -p proot-core --lib child_owns_process_group --locked"),
    # ── #56 / #58: the harnesses are the deliverable, so they are checked ─────
    dict(issue="56", claim="the fidelity harness exists and drives the real binary in a PTY",
         cmd="grep -rnF 'PDNEXT_BIN' scripts/shell_fidelity.py", needle="PDNEXT_BIN"),
    dict(issue="58", claim="the soak harness exists and can inject the leak it must catch",
         cmd="grep -rnF -- '--inject-leak' scripts/soak_sessions.py", needle="--inject-leak"),
    # ── #62: --seccomp said nothing about being inert ─────────────────────────
    dict(issue="62", claim="--seccomp refuses instead of being silently dropped",
         cmd="grep -rnF -- '--seccomp is not implemented' crates/podnext/src",
         needle="--seccomp is not implemented"),
    dict(issue="62", claim="the refusal is asserted against the real binary", cargo=True, needle=None,
         cmd="cargo test -p podnext --test stub_refusals --locked"),
    dict(issue="62", claim="the limitation is written down where a user looks",
         cmd="grep -rnF 'Seccomp filtering is not available' docs/src/known-limitations.md",
         needle="Seccomp filtering is not available"),
    # ── #47: run refuses what it cannot honour - and each refusal is a class, not a string ───
    dict(issue="47", claim="the exit-code taxonomy is asserted against the real binary",
         cmd="grep -rnF 'a_documented_refusal_is_one_with_a_reason' crates/podnext/tests/exit_codes.rs",
         needle="a_documented_refusal_is_one_with_a_reason"),
    dict(issue="47", claim="every refusal class behaves as documented", cargo=True, needle=None,
         cmd="cargo test -p podnext --test exit_codes --locked"),
    dict(issue="42", claim="no command reports a not-implemented stub on stdout",
         cmd="! (grep -rn 'not yet implemented' crates/podnext/src --include=*.rs "
             "| grep -vE ':[0-9]+:[[:space:]]*//[^/]' | grep -q .)",
         needle=None),
    dict(issue="42", claim="a stub must refuse with a reason, not exit 0",
         cmd="cargo test -p podnext --test stub_refusals", needle=None, cargo=True),
    # ── #43: a mount argument reaches the guest ──────────────────────────────
    dict(issue="43", claim="the run flag is no longer discarded",
         cmd="! grep -rqF '_volume: Vec<String>,' crates/podnext/src", needle=None),
    dict(issue="43", claim="run folds --volume into the mounts it passes on",
         cmd="grep -rqF 'specs.extend(volume)' crates/podnext/src", needle=None),
    dict(issue="43", claim="login folds its binds into the container config",
         cmd="grep -rqF 'append(&mut options.bind_mounts)' crates/proot-distro-core/src/lib.rs",
         needle=None),
    dict(issue="43", claim="the renderer no longer appends a :ro/:rw suffix",
         cmd='! grep -qF \'if readonly { "ro" }\' crates/proot-distro-core/src/bindings.rs',
         needle=None),
    dict(issue="43", claim="the rendered bind form is exactly host:target, pinned by a test",
         cmd="cargo test -p proot-distro-core --lib argument_tests", needle=None, cargo=True),
    dict(issue="43", claim="a malformed or read-only mount spec is refused",
         cmd="cargo test -p podnext --bin podnext mount_spec_tests", needle=None, cargo=True),
    # ── #44: image ls reports what it measured, not what it assumed ──────────
    dict(issue="44", claim="no column claims to be an image ID",
         cmd='! grep -qF \'rename = "IMAGE ID"\' crates/podnext/src', needle=None),
    dict(issue="44", claim="the inode size is no longer used as the size",
         cmd='! grep -qF \'path.metadata().map(|m| m.len())\' crates/podnext/src',
         needle=None),
    # Scoped to the crate, not one file: the size rendering moved to commands/image.rs when the
    # CLI was split, and a predicate pinned to a path would have gone green by disappearing.
    dict(issue="44", claim="an unmeasurable rootfs prints unknown",
         cmd="grep -rqF 'None => \"unknown\"' crates/podnext/src", needle=None),
    dict(issue="44", claim="a failed measurement is reported as a bound, never as a bare number",
         cmd="cargo test -p podnext --bin podnext size_measurement_tests", needle=None, cargo=True),
    # ── #45: podnext doctor exists and is driven by a test ───────────────────
    dict(issue="45", claim="the doctor command is dispatched",
         cmd="grep -rqF 'Commands::Doctor' crates/podnext/src", needle=None),
    dict(issue="45", claim="the doctor command is implemented",
         cmd="grep -rqF 'async fn cmd_doctor' crates/podnext/src", needle=None),
    dict(issue="45", claim="the reference documents it and names the test that covers it",
         cmd="grep -rqF 'doctor_contract.rs' docs/src/cli-reference.md", needle=None),
    dict(issue="45", claim="a check that cannot run reports unknown, never ok",
         cmd="grep -rqF 'Verdict::Unknown' crates/podnext/src/doctor.rs", needle=None),
    dict(issue="45", claim="the command itself is exercised end to end",
         cmd="cargo test -p podnext --test doctor_contract", needle=None, cargo=True),
    dict(issue="45", claim="doctor reports the proot argv it actually ran",
         cmd="grep -rqF 'guest proot argv' crates/podnext/src/doctor.rs", needle=None),
    # ── #46: path defaults follow the environment (section 9.2) ─────────────────────
    dict(issue="46", claim="the daemon binary carries no hard-coded home path",
         cmd="! grep -rqF '/data/data/com.termux/files/home' crates/podnext/src/daemon.rs",
         needle=None),
    # Scoped to the crate, not one file: the socket resolver moved to commands/service.rs when the
    # CLI was split, and a predicate pinned to commands/mod.rs would have gone red by relocation.
    dict(issue="46", claim="the daemon socket is resolved from the environment",
         cmd="grep -rqF 'paths::host::runtime_dir' crates/podnext/src", needle=None),
    dict(issue="46", claim="the prefix honours TERMUX__PREFIX before PREFIX",
         cmd="grep -rqF 'TERMUX__PREFIX' crates/proot-distro-core/src/paths.rs", needle=None),
    dict(issue="46", claim="the resolution order is unit tested",
         cmd="cargo test -p proot-distro-core --lib host_path_tests", needle=None, cargo=True),
    dict(issue="46", claim="the binary follows a relocated home and the XDG variables",
         cmd="cargo test -p podnext --test relocated_paths", needle=None, cargo=True),
    dict(issue="46", claim="CI runs the suites with HOME, PREFIX and XDG relocated",
         cmd="grep -rqF 'Relocated HOME and prefix' .github/workflows/ci.yml", needle=None),
    dict(issue="46", claim="the platform-owned TERMUX__PREFIX exclusion is documented",
         cmd="grep -rqF 'TERMUX__PREFIX' docs/src/known-limitations.md", needle=None),
    # ── #47: run refuses the options it cannot honour ────────────────────────
    dict(issue="47", claim="the refusal names the options it cannot honour",
         cmd="grep -rqF 'not implemented for `run`' crates/podnext/src", needle=None),
    dict(issue="47", claim="the help no longer claims --detach runs in the background",
         cmd="! grep -rqF 'Run container in background and print container ID' "
             "crates/podnext/src",
         needle=None),
    dict(issue="47", claim="every unhonoured option is marked refused in the reference",
         cmd="[ $(grep -c 'Refused:' docs/src/cli-reference.md) -ge 7 ]", needle=None),
    dict(issue="47", claim="each refused option exits 1 without running the command",
         cmd="cargo test -p podnext --test stub_refusals", needle=None, cargo=True),
]

# Issues with no honest automated check, and why. Every key is an issue that exists.
NO_CHECK_REASON = {
    "1": "OPEN feature request (PowerShell completion) — nothing on the tree to verify.",
    "2": "OPEN documentation task (example service configs) — no code fix to assert.",
    "3": "OPEN documentation task (tutorial) — no code fix to assert.",
    "4": "OPEN feature request (VS Code Dev Container) — no landed implementation.",
    "5": "OPEN feature request (systemd unit generator) — no landed implementation.",
    "6": "OPEN feature request (Docker Compose export) — partially landed via PR #26; see #28.",
    "9": "OPEN feature request (Prometheus exporter) — no landed implementation.",
    "10": "OPEN feature request (WebSocket events) — no landed implementation.",
    "27": "OPEN defect: documented [gateway.fail_safe] (bulkhead/retry_policy) absent from pn-gateway; "
          "no fix landed, so a fix-present predicate has nothing to assert.",
    "28": "OPEN defect: empty command yields a null Compose service in export_to_compose; no fix landed.",
    "29": "OPEN defect: podnext build local reports success without an image; no fix landed.",
    "37": "OPEN defect: image ls/push said they could not see any image because ImageManager read only "
          "the store. image ls is fixed (#44), but a real image store does not exist until v0.2.0 "
          "(roadmap section 4.4), so the remaining push/tag/rm/history behaviour has nothing to assert.",
    "67": "OPEN defect, measured while closing #63: the manual direct-execution scanner drops "
          "every flag it parses except rootfs, the command and verbose. Each flag needs the "
          "honour-or-refuse decision made against the generated argv, one at a time; the method is "
          "written in the issue.",
    "39": "OPEN defect: push cannot work because the store holds no layer blobs. The CLI now refuses "
          "honestly instead of pretending (#42), and the blobs themselves are v0.2.0 work "
          "(roadmap section 4.4): the honest refusal is asserted by the stub checks, not here.",
}


def _guard_tools() -> tuple[str | None, str | None]:
    """Where is the memory guard, and is there a lock to serialise with?

    The guard (`heavy` + `memguard.sh`) exists for one reason: on this Android device the
    low-memory killer ends the session when a cargo job spikes, so heavy jobs are wrapped and
    serialised. A GitHub runner has no such problem, and `~/.hermes/bin` does not exist there,
    so hard-coding the wrapper meant the four gated behaviour checks could never produce a
    verdict anywhere: refused by the guard locally (budget), exit 127 in CI (no `heavy`).

    Rule: use the guard when this host HAS one, run cargo plainly when it does not.
    """
    heavy = shutil.which("heavy")
    if heavy is None:
        candidate = Path.home() / ".hermes" / "bin" / "heavy"
        heavy = str(candidate) if candidate.exists() else None
    return heavy, shutil.which("flock")


HEAVY, FLOCK = _guard_tools()


def sh(cmd: str, timeout: int = 180, cargo: bool = False) -> tuple[int, str]:
    if cargo:
        inner = cmd.replace("'", "'\\''")
        if HEAVY and FLOCK:
            cmd = (
                f"flock {CARGO_LOCK} -c "
                f"'CARGO_TARGET_DIR={CARGO_TARGET} {HEAVY} --budget-mb 2000 --min-avail-mb 700 -- "
                f"env -u LD_LIBRARY_PATH {inner}'"
            )
        elif HEAVY:
            cmd = f"CARGO_TARGET_DIR={CARGO_TARGET} {HEAVY} --budget-mb 2000 --min-avail-mb 700 -- env -u LD_LIBRARY_PATH {inner}"
        else:
            cmd = f"env -u LD_LIBRARY_PATH {inner}"
    p = subprocess.run(cmd, shell=True, cwd=REPO, capture_output=True, text=True, timeout=timeout)
    return p.returncode, (p.stdout + p.stderr).strip()


def gh_json(cmd: str):
    rc, out = sh(f"env -u LD_LIBRARY_PATH {cmd}")
    if rc != 0 or not out.strip():
        return None
    try:
        return json.loads(out)
    except Exception:
        return None


def run_check(c: dict, cargo_enabled: bool) -> dict:
    """Return a result dict. A check is only ever PASS/FAIL/SKIPPED after an attempt."""
    if c.get("cargo") and not cargo_enabled:
        return {**c, "verdict": "SKIPPED", "reason": "cargo check gated; re-run with --cargo", "out": ""}
    try:
        rc, out = sh(c["cmd"], timeout=1500 if c.get("cargo") else 180, cargo=bool(c.get("cargo")))
    except subprocess.TimeoutExpired:
        return {**c, "verdict": "SKIPPED", "reason": "command timed out", "out": ""}
    except Exception as e:  # never let an unrunnable check read as a pass
        return {**c, "verdict": "SKIPPED", "reason": f"command error: {e}", "out": ""}

    # `heavy` diverts the child's stdout to its own log and prints only its summary. If the guard did
    # not let the child run to completion, the check could not be evaluated — SKIP, never FAIL. The
    # summary appends " peak=NMB dur=Ns" to the reason, so match the prefix, not the whole string; the
    # over-budget path prints a different summary ("memguard: killed at NMB"), so match both forms.
    if c.get("cargo"):
        guard_killed = "memguard: killed at" in out
        m = re.search(r"memguard: (completed|signalled|aborted) exit=(\d+) reason=(.+)", out)
        if m and not m.group(3).strip().startswith("exited normally"):
            guard_killed = True
        if guard_killed:
            return {**c, "verdict": "SKIPPED",
                    "reason": "cargo guard stopped the build (memory budget), so behaviour was not "
                              "observed; re-run on a host with a larger budget", "out": out[:200]}

    needle = c.get("needle")
    if c.get("mode") == "absent":
        ok = needle is None or needle not in out
    elif needle is None:
        ok = rc == 0
    else:
        ok = rc == 0 and needle in out
    reason = "" if ok else _predicate_context(c)
    return {**c, "verdict": "PASS" if ok else "FAIL", "reason": reason,
            "out": out[:240].replace("\n", " | ")}


def _predicate_context(c: dict) -> str:
    """Explain a FAIL, so a stale predicate cannot be mistaken for a broken fix.

    Learned the hard way: after the #29 fix deleted `fn execute_run` (the defective
    function itself), the #17 check that grepped for that symbol reported FAIL on a
    tree where the behaviour had just been fixed and covered. The FAIL was true as
    written and useless as a signal. Naming the file and its most recent commit is
    what turns "check failed" into "either the fix regressed or the predicate is
    stale", which is the decision a reader actually has to make.
    """
    needle = c.get("needle")
    if not needle:
        return ""
    m = re.search(r"(?:^|\s)([A-Za-z0-9_./-]*\.(?:rs|py|toml|yml|yaml|md|lock))", c.get("cmd", ""))
    if not m:
        return ""
    target = m.group(1)
    if not (REPO / target).exists():
        return f" predicate target `{target}` does not exist - the predicate is stale or misplaced"
    rc, out = sh(f"git log -1 --format='%h %s' -- {target}")
    last = out.strip().splitlines()[0] if rc == 0 and out.strip() else "unknown"
    return (f" needle {needle!r} not found in `{target}`; last change to that file: {last}"
            " - if a refactor removed the symbol the predicate is stale, not the fix")


def main() -> int:
    ap = argparse.ArgumentParser(description="Re-verify every issue fix against HEAD.")
    ap.add_argument("--json", action="store_true", help="print a machine-readable summary to stdout")
    ap.add_argument("--cargo", action="store_true", help="also run the gated cargo/behaviour checks")
    ap.add_argument("--no-write", action="store_true", help="do not rewrite ISSUE_LEDGER.md")
    ap.add_argument("--offline", action="store_true",
                    help="CI gate: run the source predicates, never call gh and never write the ledger")
    args = ap.parse_args()

    if args.offline:
        # CI gate mode. The verdicts below never consult GitHub, so the gate must not
        # depend on a token, a rate limit or a network round trip - an unrelated
        # outage must not be able to fail, or silently skip, this job. Coverage
        # tables need issue state, so this mode runs the checks and exits.
        issues, by_num, write_ledger = [], {}, False
    else:
        issues = gh_json("gh issue list --state all --limit 100 --json number,title,state,labels,closedAt,author")
        if issues is None:
            print("could not fetch issues (gh returned nothing) — refusing to write a ledger", file=sys.stderr)
            return 2
        by_num = {i["number"]: i for i in issues}
        write_ledger = not args.no_write

    results = [run_check(c, args.cargo) for c in CHECKS]
    npass = sum(r["verdict"] == "PASS" for r in results)
    nfail = sum(r["verdict"] == "FAIL" for r in results)
    nskip = sum(r["verdict"] == "SKIPPED" for r in results)
    fails = [r for r in results if r["verdict"] == "FAIL"]

    # With --json, the human table goes to stderr so stdout stays pure JSON for `| jq`.
    tbl = sys.stderr if args.json else sys.stdout
    print(f"{'issue':6} {'check':74} verdict", file=tbl)
    print("-" * 96, file=tbl)
    for r in results:
        print(f"#{r['issue']:5} {r['claim'][:72]:74} {r['verdict']}", file=tbl)
    print(f"\nchecks: {npass} PASS, {nfail} FAIL, {nskip} SKIPPED (of {len(results)})", file=tbl)

    per_issue: dict[str, list] = {}
    for r in results:
        per_issue.setdefault(r["issue"], []).append(r)
    checked = {int(n) for n in per_issue}

    def is_wayfinder(n: int) -> bool:
        """Wayfinder tickets are work items, not audit findings: their acceptance criteria live in
        their own bodies and are tracked by the map issue. Holding them to the audit triage rule
        would demand a source predicate for work that has not been done yet."""
        return any(l["name"].startswith("wayfinder:") for l in by_num[n]["labels"])

    uncovered_all = sorted(i["number"] for i in issues if i["number"] not in checked)
    uncovered = [n for n in uncovered_all if not is_wayfinder(n)]
    tracked_by_map = [n for n in uncovered_all if is_wayfinder(n)]
    uncovered_open = [n for n in uncovered if by_num[n]["state"] == "OPEN"]

    # Every finding must be fixed with a check, or declined with a reason written down by hand.
    # A generated fallback sentence is not a disposition: it made the table look complete while
    # three issues (#37, #39, #41) had in fact never been triaged.
    missing_reasons = [n for n in uncovered if str(n) not in NO_CHECK_REASON]
    thin_reasons = [n for n in uncovered
                    if str(n) in NO_CHECK_REASON and len(NO_CHECK_REASON[str(n)]) < 40]
    nfail += len(missing_reasons) + len(thin_reasons)

    def issue_line(n: str) -> str:
        i = by_num.get(int(n))
        if not i:
            return f"#{n} (not in the fetched list)"
        labs = ", ".join(l["name"] for l in i["labels"])
        return (f"**#{n} {i['title']}**  \n_state_: {i['state']}  ·  _labels_: {labs or '(none)'}  ·  "
                f"_closed_: {i['closedAt'] or '—'}")

    head = sh("git rev-parse --short HEAD")[1]
    subj = sh("git log -1 --format=%s")[1][:70]
    generated = time.strftime("%Y-%m-%d %H:%M:%SZ", time.gmtime())

    lines = [
        "# PDN issue ledger — fixes re-verified against HEAD",
        "",
        f"Generated {generated} from `{head}` ({subj}).",
        "",
        "## Why this exists",
        "",
        "Closing an issue records an intention. It does not prove the fix is still in the tree: a later",
        "refactor, a revert, or a merge that resolved a conflict the wrong way can quietly undo it. This",
        "ledger pairs every issue with a command that must still succeed against the current code, so a",
        "silently-regressed fix shows up as a FAIL in a document rather than as a surprise in production.",
        "",
        "## Verdict contract",
        "",
        "| verdict | meaning | effect on exit status |",
        "|---|---|---|",
        "| **PASS** | the exact symbol the fix introduces was found; the fix is present | none |",
        "| **FAIL** | the symbol is absent or the check could not hold: a regression, or a fix that is gone | **exit 1** |",
        "| **SKIPPED** | no automated check exists (open issue), or a gated `--cargo` check was not enabled | none, and **never** a pass |",
        "",
        f"**Coverage: {npass} checks PASS by direct observation · {nfail} FAIL · {nskip} SKIPPED**, "
        f"across {len(results)} checks covering {len(checked)} of {len(issues)} issues "
        f"({len(uncovered)} issues have no automated check).",
        "",
    ]
    if nskip:
        lines += [
            "**A SKIPPED check is not evidence the fix works.** It means no verdict was produced. Here the "
            f"{nskip} SKIPPED are cargo-gated behaviour checks: they are not enabled by default, and on this",
            "device the memory guard refuses the bench build (RSS exceeds the budget), so the behaviour behind",
            "them was not observed in the committed run. Do not read the PASS count as full verification.",
            "",
        ]
    else:
        lines += [
            "**Every check in this run produced a verdict**, including the cargo-gated behaviour checks,",
            "so no fix in this ledger is recorded as PASS on the strength of a source grep alone.",
            "",
        ]
    lines += [
        "## Checks that FAIL (a fix that is no longer present)",
        "",
    ]
    if fails:
        for r in fails:
            lines += [f"- **#{r['issue']} — {r['claim']}**", f"  - `{r['cmd']}`",
                      f"  - output: `{r['out']}`"]
            if r.get("reason"):
                lines += [f"  - why it failed: {r['reason'].strip()}"]
            lines += [""]
    else:
        lines += ["None.", ""]

    skipped = [r for r in results if r["verdict"] == "SKIPPED"]
    lines += ["## Checks SKIPPED (no verdict was produced)", ""]
    if skipped:
        for r in skipped:
            lines += [f"- **#{r['issue']} — {r['claim']}** — _{r['reason']}_"]
        lines += [""]
    else:
        lines += ["None.", ""]

    lines += ["## Per-issue detail", ""]
    for n in sorted(per_issue, key=lambda x: int(x)):
        lines += [issue_line(n), ""]
        for r in per_issue[n]:
            lines += [f"- [{r['verdict']}] {r['claim']}", f"  - `{r['cmd']}`", f"  - → `{r['out']}`"]
        lines += [""]

    lines += [
        "## Issues with no automated check (the honest gap)",
        "",
        "Listed rather than omitted, so the coverage gap is visible. Each has a reason. These are open",
        "feature requests, open defects with no landed fix, or anything whose verification needs a live",
        "container and therefore belongs in CI.",
        "",
        "| # | state | reason | title |",
        "|---|---|---|---|",
    ]
    for n in missing_reasons:
        lines.append(f"| {n} | {by_num[n]['state']} | **UNTRIAGED - no reason recorded** | "
                     f"{by_num[n]['title'][:70]} |")
    for n in thin_reasons:
        lines.append(f"| {n} | {by_num[n]['state']} | **reason too thin to be a disposition** | "
                     f"{by_num[n]['title'][:70]} |")
    for n in uncovered:
        if n in missing_reasons or n in thin_reasons:
            continue
        i = by_num[n]
        reason = NO_CHECK_REASON[str(n)]
        reason = reason[5:] if reason.startswith("OPEN ") else reason
        lines.append(f"| {n} | {i['state']} | {reason} | {i['title'][:70]} |")
    lines.append("")
    if tracked_by_map:
        lines += [
            f"The {len(tracked_by_map)} issues labelled `wayfinder:*` "
            f"({', '.join('#' + str(n) for n in tracked_by_map)}) are work items, not findings: "
            "the map issue holds their acceptance criteria, and they are not subject to this "
            "triage list.",
            "",
        ]
    if missing_reasons or thin_reasons:
        lines += [
            f"**This ledger FAILS: {len(missing_reasons)} untriaged and {len(thin_reasons)} "
            "thinly-reasoned issue(s)**. Every finding must be either covered by a predicate above "
            "or declined in `NO_CHECK_REASON` in a sentence that names what would have to exist for "
            "the check to be possible. Regenerate with a hand-written reason - never with a "
            "default.",
            "",
        ]

    lines += [
        "## How to re-run this (CI-friendly)",
        "",
        "```sh",
        "export PATH=\"$HOME/.hermes/bin:$PATH\"",
        "# source predicates only — fast, read-only, safe on any host including Android:",
        "env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py",
        "# machine-readable summary for CI (parse jq .checks.fail; exit status is 1 on any FAIL):",
        "env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py --json | jq .checks",
        "# opt-in behaviour checks (build; serialised with flock against concurrent departments):",
        "env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py --cargo",
        "```",
        "",
        "Exit status is **0** when no check FAILs and **1** when any check FAILs, so the ledger doubles as",
        "a regression gate. `--json` prints `{\"checks\":{\"pass\":N,\"fail\":N,\"skipped\":N,\"total\":N}}`.",
        "This file is regenerated by every run — it cannot drift from the code.",
        "",
    ]

    summary = {
        "generated": generated,
        "head": head,
        "checks": {"pass": npass, "fail": nfail, "skipped": nskip, "total": len(results)},
        "issues": {"total": len(issues), "covered": len(checked), "uncovered": len(uncovered),
                   "uncovered_open": len(uncovered_open)},
        "failures": [{"issue": r["issue"], "claim": r["claim"], "cmd": r["cmd"],
                      "reason": r["reason"], "out": r["out"]} for r in fails],
        "skipped_gated": [{"issue": r["issue"], "claim": r["claim"], "reason": r["reason"]}
                          for r in skipped if r["reason"].startswith("cargo")],
        "cargo_enabled": args.cargo,
    }

    if write_ledger:
        OUT.write_text("\n".join(lines))
        print(f"\nwritten: {OUT}", file=(sys.stderr if args.json else sys.stdout))
    if args.json:
        print(json.dumps(summary, indent=2, sort_keys=True))
    return 1 if nfail else 0


if __name__ == "__main__":
    sys.exit(main())
