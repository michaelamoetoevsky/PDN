# Documentation ↔ Configuration Drift, Round 2

**Department:** Documentation Engineering, DE.
**Branch:** `dept/doc-drift`, a worktree of `72ef9d6`.
**Scope:** This round reconciles the residual documented-config-key drift in the files that DE owns.
It also adds a whole-file drift guard.
Truthfulness over volume.
Three doc files are complete.
The rest stay explicitly untouched.
See section 6.

---

## Summary

* **Re-derived, not inherited:** The three reconciled files document **142** distinct config keys.
  Each key is a bare TOML assignment.
  **113** of these keys, which is 80 %, have no backing struct field.
  The historical `227`/`261` numbers came from an older revision.
  That revision had no `deny_unknown_fields` and different line numbers.
  This document does **not** reproduce those numbers.
* **Reconciled:** This round renamed 3 tokens to their real field names in the markdown.
  The remaining 112 absent keys have a `# planned:` declaration in the docs.
  The test lists each key in its `PLANNED` allowlist with a reason.
  No config field was invented.
* **Guard:** `crates/pn-gateway/tests/doc_key_drift.rs` holds 2 tests, and both are green.
  A raw negative control proves that the guard bites.
  See section 4.
* **Not touched:** 5 doc files also contain TOML config blocks.
  See section 6.
  This report states that fact plainly and does not imply it.

---

## 1. Re-derived drift set with command and raw output

The set came from the **current tree**, not from `research/DOCS_CONFIG_DRIFT_AUDIT.md`.
The command follows.

```sh
cd /data/data/com.termux/files/home/.hermes/cache/scratch/wt-c3
env -u LD_LIBRARY_PATH python3 ~/.hermes/cache/scratch/doc_keys.py \
    docs/src/configuration.md docs/src/architecture.md docs/src/cli-reference.md
```

The raw output follows, in part.

```
docs/src/configuration.md: 140 keys
docs/src/architecture.md: 0 keys
docs/src/cli-reference.md: 20 keys
TOTAL distinct across files: 142
```

The extraction rules are identical to the Rust guard.
The extractor reads fenced `toml` blocks only.
A key is a **bare** `key = ...` assignment.
Quoted keys are dynamic map entries or example values, not field names.
Inline-table values contribute their bare sub-keys.
Assignments inside a dynamic-map table are map entries, not keys.
The dynamic-map tables are `env`, `labels`, `annotations`, `mounts`, `volumes`, `tmpfs`, `devices`, `sysctls`, `ulimits`, and `rules`.

`architecture.md` contributes **0** keys, because it has no ```toml blocks.
Its config sketches are ```rust.
For that reason, this round reconciles `architecture.md` in prose and type sketches only.
The guard does **not** cover `architecture.md`.

**Verdict split of the 142 keys.**
A key is real when its leaf name is a field of a deserialisable struct that `pn-gateway` can reach.

| Verdict | Count | Disposition |
|---|---|---|
| real field name | 35 | no action |
| renamed (real field under another name) | 3 | markdown fixed → leaves the drifted set |
| absent (no field anywhere) | 104 | declared `# planned:` + in `PLANNED` |
| documented only in a `# planned:` sentinel (not extracted as active) | 8 | in `PLANNED` |
| **total drifted reconciled** | **112** | allowlist entries |

The 8 sentinel-only keys are `bulkhead_max_concurrent`, `bulkhead_queue_size`, `retry_base_delay`, and `retry_max_delay`.
These are the fail-safe controls from issue #27.
The list also includes `entrypoint`, `cmd`, `stop_signal`, and `stop_timeout`.
Those keys appear inside the dynamic `[container.config.annotations]` table.
The extractor does not extract them as active keys.

---

## 2. Decision table

The brief allows three outcomes only.
For outcome a, the field exists under another name, so the markdown is fixed.
For outcome b, the field is absent and the feature is intended, so the key is planned with a note.
The guard does **not** invent a field.
For outcome c, the token was never a config key, because it is prose, metadata, or an example value.
The text stays, with a note added.

### 2.1 Outcome a, renamed keys, markdown fixed to the real field name

| Key (as documented) | Real field | Action |
|---|---|---|
| `profile` (in `[container.security]`, `[container.config.seccomp]`, `[container.config.apparmor]`, `[profile.apparmor]`) | `seccomp_profile` / `apparmor_profile` / `profile_name` | markdown fixed to the correct per-section field name |
| `dns` (in `[container.network]`, `[network.config]`) | `dns_servers` | markdown fixed |
| `type` (in `[profile]`) | `profile_type` | markdown fixed (the `[tracing.exporter] type` occurrence has no struct and stays planned) |
| `image` (in `[container]`) | `distro` + `version` | markdown fixed to the real field pair |

### 2.2 Outcome c, not a config key, noted, text kept

| Token | Verdict | Action |
|---|---|---|
| `created`, `updated` | container/network/volume metadata, not a struct field | removed from the active TOML and recorded as planned (shown here as metadata, not config) |
| env-var names, labels, annotations, syscall names, mount/tmpfs/device/ulimit paths | dynamic map entries / example values | left untouched; deliberately excluded by the extractor |

### 2.3 Outcome b, planned absent fields, marked planned, no field invented

Every key below has a `# planned:` line in the docs.
Each key also carries the same reason in the test's `PLANNED` allowlist.
Total entries: 112.

| Key | Verdict | Reason / action |
|---|---|---|
| `addr` | planned | no gRPC config struct exists in `pn-gateway` |
| `allow_unsigned` | planned | no plugin-manager config struct exists in `pn-gateway` |
| `allowed_images` | planned | no field on `GuardrailConfig` (real: `enabled`, `rules`, `strict_mode`, `audit_log`) |
| `api` | planned | no `api` field on `GpuConfig` |
| `blocked_images` | planned | no field on `GuardrailConfig` (real: `enabled`, `rules`, `strict_mode`, `audit_log`) |
| `blocked_syscalls` | planned | no field on `GuardrailConfig` (real: `enabled`, `rules`, `strict_mode`, `audit_log`) |
| `bulkhead_max_concurrent` | planned | documented fail-safe control with no `FailSafeConfig` field (issue #27) |
| `bulkhead_queue_size` | planned | documented fail-safe control with no `FailSafeConfig` field (issue #27) |
| `cache_dir` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `cmd` | planned | no `[container.config]` table; real `ContainerConfig` is flat |
| `config_dir` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `cors_allowed_headers` | planned | no `[api.server]` struct exists in `pn-gateway` |
| `cors_allowed_methods` | planned | no `[api.server]` struct exists in `pn-gateway` |
| `cors_allowed_origins` | planned | no `[api.server]` struct exists in `pn-gateway` |
| `cors_max_age` | planned | no `[api.server]` struct exists in `pn-gateway` |
| `cpu_period` | planned | no field on `ResourceLimits` (real: `cpu_limit`, `memory_limit`, `disk_limit`, `pids_limit`) |
| `cpu_quota` | planned | no field on `ResourceLimits` (real: `cpu_limit`, `memory_limit`, `disk_limit`, `pids_limit`) |
| `cpu_shares` | planned | no field on `ResourceLimits` (real: `cpu_limit`, `memory_limit`, `disk_limit`, `pids_limit`) |
| `cpuset_cpus` | planned | no field on `ResourceLimits` (real: `cpu_limit`, `memory_limit`, `disk_limit`, `pids_limit`) |
| `cpuset_mems` | planned | no field on `ResourceLimits` (real: `cpu_limit`, `memory_limit`, `disk_limit`, `pids_limit`) |
| `created` | planned | container/network/volume metadata, not a struct field |
| `custom_rules` | planned | no field anywhere; container `SecurityConfig` uses `seccomp_profile`, profile seccomp uses `syscalls` |
| `data_dir` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `default_cpu_limit` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `default_dns` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `default_dns_search` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `default_memory_limit` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `default_network` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `default_pids_limit` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `default_registry` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `default_runtime` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `default_security_profile` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `default_storage_driver` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `default_volume_driver` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `description` | planned | no `description` field on `SecurityProfileConfig` |
| `dns_options` | planned | no field on container `NetworkConfig` (real: `mode`, `bridge`, `ip_address`, `gateway`, `dns_servers`, `port_mappings`) |
| `dns_search` | planned | no field on container `NetworkConfig` (real: `mode`, `bridge`, `ip_address`, `gateway`, `dns_servers`, `port_mappings`) |
| `domainname` | planned | no `[container.config]` table; real `ContainerConfig` is flat |
| `enable_criu` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `enable_gpu` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `enable_icc` | planned | no field on network `NetworkConfig` (flat schema: `name`,`driver`,`subnet`,`gateway`,`dns_servers`,`enable_ipv6`,`mtu`,`containers`,`ips`) |
| `enable_metrics` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `enable_plugins` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `enable_tracing` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `endpoint` | planned | no tracing config struct exists in `pn-gateway` |
| `entrypoint` | planned | no `[container.config]` table; real `ContainerConfig` is flat |
| `extra_hosts` | planned | no field on container `NetworkConfig` (real: `mode`, `bridge`, `ip_address`, `gateway`, `dns_servers`, `port_mappings`) |
| `fs_type` | planned | no field on `VolumeConfig` (real: `name`, `driver`, `labels`, `options`) |
| `gateway_addr` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `gateway_retry_attempts` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `gateway_timeout` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `group` | planned | no `[container.config]` table; real `ContainerConfig` is flat |
| `groups` | planned | no `[container.config]` table; real `ContainerConfig` is flat |
| `healthy_threshold` | planned | no field on `HealthConfig` (real: `enabled`, `check_interval`, `checks`, `unhealthy_threshold`, `recovery_actions`) |
| `image_pull_policy` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `insecure_registries` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `internal` | planned | no field on network `NetworkConfig` (flat schema: `name`,`driver`,`subnet`,`gateway`,`dns_servers`,`enable_ipv6`,`mtu`,`containers`,`ips`) |
| `io_max_read` | planned | no field on `ResourceLimits` (real: `cpu_limit`, `memory_limit`, `disk_limit`, `pids_limit`) |
| `io_max_read_bps` | planned | no field on `ResourceLimits` (real: `cpu_limit`, `memory_limit`, `disk_limit`, `pids_limit`) |
| `io_max_read_iops` | planned | no field on `ResourceLimits` (real: `cpu_limit`, `memory_limit`, `disk_limit`, `pids_limit`) |
| `io_max_write` | planned | no field on `ResourceLimits` (real: `cpu_limit`, `memory_limit`, `disk_limit`, `pids_limit`) |
| `io_max_write_bps` | planned | no field on `ResourceLimits` (real: `cpu_limit`, `memory_limit`, `disk_limit`, `pids_limit`) |
| `io_max_write_iops` | planned | no field on `ResourceLimits` (real: `cpu_limit`, `memory_limit`, `disk_limit`, `pids_limit`) |
| `io_weight` | planned | no field on `ResourceLimits` (real: `cpu_limit`, `memory_limit`, `disk_limit`, `pids_limit`) |
| `ip_range` | planned | no field on network `NetworkConfig` (flat schema: `name`,`driver`,`subnet`,`gateway`,`dns_servers`,`enable_ipv6`,`mtu`,`containers`,`ips`) |
| `ipc_mode` | planned | no field on container `NetworkConfig` (real: `mode`, `bridge`, `ip_address`, `gateway`, `dns_servers`, `port_mappings`) |
| `ipv6_gateway` | planned | no field on network `NetworkConfig` (flat schema: `name`,`driver`,`subnet`,`gateway`,`dns_servers`,`enable_ipv6`,`mtu`,`containers`,`ips`) |
| `ipv6_subnet` | planned | no field on network `NetworkConfig` (flat schema: `name`,`driver`,`subnet`,`gateway`,`dns_servers`,`enable_ipv6`,`mtu`,`containers`,`ips`) |
| `keepalive_permit_without_stream` | planned | no gRPC config struct exists in `pn-gateway` |
| `keepalive_time` | planned | no gRPC config struct exists in `pn-gateway` |
| `keepalive_timeout` | planned | no gRPC config struct exists in `pn-gateway` |
| `log_dir` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `log_file` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `log_format` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `log_level` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `log_max_files` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `log_rotation` | planned | documented under `[general]`; no `GeneralConfig` struct exists and the global `config.toml` has no loader |
| `mask_paths` | planned | no field on container `SecurityConfig` |
| `max_checkpoints` | planned | no checkpoint support (no struct at all) |
| `max_containers` | planned | no field on `GuardrailConfig` (real: `enabled`, `rules`, `strict_mode`, `audit_log`) |
| `max_containers_per_user` | planned | no field on `GuardrailConfig` (real: `enabled`, `rules`, `strict_mode`, `audit_log`) |
| `max_cpu_total` | planned | no field on `GuardrailConfig` (real: `enabled`, `rules`, `strict_mode`, `audit_log`) |
| `max_memory_total` | planned | no field on `GuardrailConfig` (real: `enabled`, `rules`, `strict_mode`, `audit_log`) |
| `max_plugins` | planned | no plugin-manager config struct exists in `pn-gateway` |
| `max_recv_msg_size` | planned | no gRPC config struct exists in `pn-gateway` |
| `max_send_msg_size` | planned | no gRPC config struct exists in `pn-gateway` |
| `memory_reservation` | planned | no field on `ResourceLimits` (real: `cpu_limit`, `memory_limit`, `disk_limit`, `pids_limit`) |
| `memory_swap_limit` | planned | no field on `ResourceLimits` (real: `cpu_limit`, `memory_limit`, `disk_limit`, `pids_limit`) |
| `mount_options` | planned | no field on `VolumeConfig` (real: `name`, `driver`, `labels`, `options`) |
| `networks` | planned | no field on container `NetworkConfig` (real: `mode`, `bridge`, `ip_address`, `gateway`, `dns_servers`, `port_mappings`) |
| `path` | planned | no field on `MetricsConfig` (real: `enabled`, `interval`, `retention`, `exporters`) |
| `paths` | planned | no mask/readonly paths on `SecurityProfileConfig` |
| `plugin_dirs` | planned | no plugin-manager config struct exists in `pn-gateway` |
| `port` | planned | no field on `MetricsConfig` (real: `enabled`, `interval`, `retention`, `exporters`) |
| `ports` | planned | no field on container `NetworkConfig` (real: `mode`, `bridge`, `ip_address`, `gateway`, `dns_servers`, `port_mappings`) |
| `poststart` | planned | no container hooks support (no struct at all) |
| `poststop` | planned | no container hooks support (no struct at all) |
| `prestart` | planned | no container hooks support (no struct at all) |
| `prestop` | planned | no container hooks support (no struct at all) |
| `quota_enabled` | planned | no field on `VolumeConfig` (real: `name`, `driver`, `labels`, `options`) |
| `quota_size` | planned | no field on `VolumeConfig` (real: `name`, `driver`, `labels`, `options`) |
| `readonly_paths` | planned | no field on container `SecurityConfig` |
| `required_security_profile` | planned | no field on `GuardrailConfig` (real: `enabled`, `rules`, `strict_mode`, `audit_log`) |
| `retry_base_delay` | planned | documented fail-safe control with no `FailSafeConfig` field (issue #27) |
| `retry_max_delay` | planned | documented fail-safe control with no `FailSafeConfig` field (issue #27) |
| `sampling_ratio` | planned | no tracing config struct exists in `pn-gateway` |
| `stop_signal` | planned | no `[container.config]` table; real `ContainerConfig` is flat |
| `stop_timeout` | planned | no `[container.config]` table; real `ContainerConfig` is flat |
| `type` | planned | no tracing config struct exists in `pn-gateway` |
| `updated` | planned | container metadata, not a `ContainerConfig` field (real: `name`, `distro`, `version`, `arch`) |
| `user` | planned | no `[container.config]` table; real `ContainerConfig` is flat |
| `uts_mode` | planned | no field on container `NetworkConfig` (real: `mode`, `bridge`, `ip_address`, `gateway`, `dns_servers`, `port_mappings`) |

---

## 3. The guard test

`crates/pn-gateway/tests/doc_key_drift.rs` is a new file that DE owns.

* `documented_config_keys_are_real_or_planned` extracts every documented key from the listed files.
  The test asserts that each key is a real field of a deserialisable struct that this crate can reach.
  The extractor derives the field set from the serialised structs.
  Nobody copies the field set by hand.
  The test also accepts a key that is in the `PLANNED` allowlist.
  The test fails and names the offending keys.
* `planned_allowlist_matches_documented_declarations` asserts set equality between the `# planned:` declarations in the docs and the `PLANNED` allowlist.
  The two sets cannot drift apart.

The test's own header comment states the scope, and this section states it too.
The test covers the `toml` blocks of `DOC_FILES`.
`DOC_FILES` is `docs/src/configuration.md`, `docs/src/architecture.md`, and `docs/src/cli-reference.md`.
The test covers nothing else.
`architecture.md` yields zero keys, because it has no toml blocks.
No test covers its prose and type-sketch fixes.

**Ceiling, deliberate and documented in the test.**
The guard matches key **names**, not full nesting paths.
The guard accepts a documented key whose leaf name matches a real field, even when its section path is wrong.
For example, `[network.config] subnet` must be a flat `subnet`.
Path inference from prose headings is not decidable.
The project accepts this false-negative class and does not cry wolf.

## 4. Negative control, raw, the important part

The negative control **temporarily** added a fake key `drift_probe_unknown_key = 1` to the `[gateway.metrics]` block of `docs/src/configuration.md`.
The test ran again.
Then the probe was reverted.

```
$ flock $HOME/.hermes/cache/scratch/cargo-mutex.lock -c 'CARGO_TARGET_DIR=.../PDN/target \
    heavy --budget-mb 2000 --min-avail-mb 700 --label de-docdrift-NEGATIVE -- \
    env -u LD_LIBRARY_PATH cargo test -p pn-gateway --locked --test doc_key_drift'
memguard: de-docdrift-NEGATIVE pid=25088 pgid=25088 budget=2000MB min_avail=700MB
memguard: log=/data/data/com.termux/files/home/.hermes/logs/heavy/20261104T192941Z-de-docdrift-NEGATIVE.log
memguard: completed exit=101 reason=exited normally peak=0MB dur=2s
```

```
running 2 tests
test planned_allowlist_matches_documented_declarations ... ok
test documented_config_keys_are_real_or_planned ... FAILED

---- documented_config_keys_are_real_or_planned stdout ----
thread 'documented_config_keys_are_real_or_planned' panicked at crates/pn-gateway/tests/doc_key_drift.rs:215:5:
1 documented key(s) are neither a real config field nor listed in PLANNED (issue #27 / doc-drift):
  docs/src/configuration.md: `drift_probe_unknown_key`
Fix the markdown, or add the key to PLANNED with a reason. (281 documented keys checked across 3 file(s))

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

The probe was then removed.
`grep drift_probe_unknown_key docs/` gave no match.

## 5. Green run, raw output

```
$ flock ... -c '... heavy --label de-docdrift-alltests -- env -u LD_LIBRARY_PATH cargo test -p pn-gateway --locked'
memguard: log=/data/data/com.termux/files/home/.hermes/logs/heavy/20261104T192901Z-de-docdrift-alltests.log
memguard: completed exit=0 reason=exited normally peak=476MB dur=7s
```

```
Running unittests src/lib.rs (pn_gateway-...)
running 5 tests
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/doc_key_drift.rs (doc_key_drift-...)
running 2 tests
test planned_allowlist_matches_documented_declarations ... ok
test documented_config_keys_are_real_or_planned ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

   Doc-tests pn_gateway
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

`env -u LD_LIBRARY_PATH cargo fmt --all` is clean, with exit 0.
After the run, `git status` shows only the three DE-owned doc files as modified, plus the new `crates/pn-gateway/tests/` directory.

---

## 6. Items NOT reconciled, stated honestly

These `docs/src/*.md` files contain `toml` config blocks and are **not** touched this round.

| File | toml blocks | Why not |
|---|---|---|
| `docs/src/advanced.md` | 5 | out of the three-file scope chosen to finish completely |
| `docs/src/api-reference.md` | 1 | contains a containerd runtime blob (`external`, not a PDN key) |
| `docs/src/migration-guide.md` | 5 | same container/profile surface as configuration.md; not reconciled |
| `docs/src/security.md` | 23 | largest untackled block; `gateway.guardrails.*`, `container.security.*` |
| `docs/src/troubleshooting.md` | 1 | `container.resources.io_*` examples |

The guard **does not check these files**, because they are not in `DOC_FILES`.
Their drift is unchanged.
`research/DOCS_CONFIG_DRIFT_AUDIT.md` still describes it.
A natural next step extends `DOC_FILES` and the extractor to those files after reconciliation.

No test covers `docs/src/architecture.md` either, because it has no toml blocks.
A manual correction fixed its `GuardrailsConfig`, `HealthCheck` and `ProotConfig` type sketches.
A manual read alone confirms them.

**Not a fix this round.**
The global `config.toml` still has no loader.
`pn-gateway` has no `toml` dependency.
The docs now say so at the top of the global example.
The maintainer decides whether to implement a loader.

**Not verifiable here.**
This round has no end-to-end run of `podnext` against a documented `config.toml`.
No loader exists to exercise.
The inertness is structural, not observed at runtime.

This round did not modify `scripts/build_issue_ledger.py`.
This guard is a natural addition to that harness.
The harness is repo-wide in intent and already runs the crate tests.
This report flags the harness and does not edit an unowned file.

---

## 7. Exact reproduction

```sh
cd /data/data/com.termux/files/home/workspace/PDN
git worktree add --detach ~/.hermes/cache/scratch/wt-c3 72ef9d6
cd ~/.hermes/cache/scratch/wt-c3 && git switch -c dept/doc-drift

# re-derive the drift set
env -u LD_LIBRARY_PATH python3 ~/.hermes/cache/scratch/doc_keys.py \
  docs/src/configuration.md docs/src/architecture.md docs/src/cli-reference.md

# guard (green) + full crate suite
export PATH="$HOME/.hermes/bin:$PATH"
flock $HOME/.hermes/cache/scratch/cargo-mutex.lock -c \
  'CARGO_TARGET_DIR=/data/data/com.termux/files/home/workspace/PDN/target \
   heavy --budget-mb 2000 --min-avail-mb 700 -- \
   env -u LD_LIBRARY_PATH cargo test -p pn-gateway --locked'
```

Raw transcripts:
* `~/.hermes/logs/heavy/20261104T192841Z-de-docdrift-green1.log` is the guard green run.
* `~/.hermes/logs/heavy/20261104T192901Z-de-docdrift-alltests.log` is the full crate suite green run.
* `~/.hermes/logs/heavy/20261104T192941Z-de-docdrift-NEGATIVE.log` is the negative control FAIL, exit 101.

## Follow-up for this increment

The scope section names a gap, "drift left explicitly untouched".
The guard itself now bounds that gap, not the prose.
`crates/pn-gateway/tests/doc_key_drift.rs` scans **every** `docs/src/*.md` file that contains a TOML block.
The count is 8 files, up from 3.
Drift in `security.md`, `advanced.md`, `migration-guide.md`, `api-reference.md` and `troubleshooting.md` now fails CI.
The drift no longer goes unseen.

Two mechanisms keep that honest.
They do not invent configuration.

* `# not-pdn-config:` inside a block marks documentation of *another* tool's file.
  Examples are a containerd `config.toml`, an OCI runtime-spec or image-layout example, and the previous pn-next schema.
  The extractor skips those blocks entirely.
  `BinaryName`, `runtime_type` and `layers` are not this crate's keys.
* `# planned: k1, k2` declares documented keys with no backing field.
  The test lists each key in `PLANNED` with its reason.
  `security.md` holds security and quota controls.
  The controls are `uid_map`, `gid_map`, `allowed`, `blocked`, `burst`, `requests_per_minute`, `containers_per_user`, `cpu_per_user`, `memory_per_user`, `memory_swappiness`, `networks_per_user`, and `volumes_per_user`.
  The tests pin them that way.
  They are documented, not implemented, and visible as such.

Both guard tests pass: `cargo test -p pn-gateway --locked --test doc_key_drift` -> `2 passed; 0 failed`.
