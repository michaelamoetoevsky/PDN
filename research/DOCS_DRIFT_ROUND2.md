# Documentation ↔ Configuration Drift — Round 2

**Department:** Documentation Engineering (DE)
**Branch:** `dept/doc-drift` (worktree of `72ef9d6`)
**Scope:** reconcile the residual documented-config-key drift in the files DE owns and add a
whole-file drift guard. Truthfulness over volume: three doc files finished completely, the rest
left explicitly untouched (see §6).

---

## TL;DR

* **Re-derived, not inherited:** **142** distinct config keys are documented (bare TOML
  assignments) in the three reconciled files; **113** of them (80 %) have no backing struct field.
  The historical `227`/`261` numbers were measured against an older revision (no
  `deny_unknown_fields`, different line numbers) and are **not** reproduced here.
* **Reconciled:** 3 tokens renamed to their real field names in the markdown; the remaining 112
  absent keys are declared `# planned:` in the docs **and** enumerated in the test's `PLANNED`
  allowlist with a reason each. No config field was invented.
* **Guard:** `crates/pn-gateway/tests/doc_key_drift.rs` — 2 tests, green. Proven to bite with a
  raw negative control (§4).
* **Not touched:** 5 doc files that also contain TOML config blocks (§6). Stated plainly, not
  implied.

---

## 1. Re-derived drift set (command + raw output)

The set was re-derived from the **current tree**, not read off
`research/DOCS_CONFIG_DRIFT_AUDIT.md`. Command:

```sh
cd /data/data/com.termux/files/home/.hermes/cache/scratch/wt-c3
env -u LD_LIBRARY_PATH python3 ~/.hermes/cache/scratch/doc_keys.py \
    docs/src/configuration.md docs/src/architecture.md docs/src/cli-reference.md
```

Raw output (excerpt):

```
docs/src/configuration.md: 140 keys
docs/src/architecture.md: 0 keys
docs/src/cli-reference.md: 20 keys
TOTAL distinct across files: 142
```

Extraction rules (identical to the Rust guard): fenced ```toml blocks only; a key is a **bare**
`key = ...` assignment (quoted keys are dynamic map entries / example values, not field names);
inline-table values contribute their bare sub-keys; assignments inside a dynamic-map table
(`env`, `labels`, `annotations`, `mounts`, `volumes`, `tmpfs`, `devices`, `sysctls`, `ulimits`,
`rules`) are map entries, not keys.

`architecture.md` contributes **0** keys — it has no ```toml blocks (its config sketches are
```rust), which is why its reconciliation in this round is prose/type-sketch only and is **not**
covered by the guard.

**Verdict split of the 142 keys** (real = leaf name is a field of a deserialisable struct
reachable from `pn-gateway`):

| Verdict | Count | Disposition |
|---|---|---|
| real field name | 35 | no action |
| renamed (real field under another name) | 3 | markdown fixed → leaves the drifted set |
| absent (no field anywhere) | 104 | declared `# planned:` + in `PLANNED` |
| documented only in a `# planned:` sentinel (not extracted as active) | 8 | in `PLANNED` |
| **total drifted reconciled** | **112** | allowlist entries |

The 8 sentinel-only keys are `bulkhead_max_concurrent`, `bulkhead_queue_size`, `retry_base_delay`,
`retry_max_delay` (the fail-safe controls from issue #27) plus `entrypoint`, `cmd`, `stop_signal`,
`stop_timeout` (documented inside the dynamic `[container.config.annotations]` table, so not
extracted as active).

---

## 2. Decision table

Three outcomes only, per the brief: **(a)** field exists under another name → markdown fixed;
**(b)** field absent, feature intended → marked planned with a note, **no field invented**;
**(c)** never a config key (prose / metadata / example value) → noted, text kept.

### 2.1 (a) Renamed → markdown fixed to the real field name

| Key (as documented) | Real field | Action |
|---|---|---|
| `profile` (in `[container.security]`, `[container.config.seccomp]`, `[container.config.apparmor]`, `[profile.apparmor]`) | `seccomp_profile` / `apparmor_profile` / `profile_name` | markdown fixed to the correct per-section field name |
| `dns` (in `[container.network]`, `[network.config]`) | `dns_servers` | markdown fixed |
| `type` (in `[profile]`) | `profile_type` | markdown fixed (the `[tracing.exporter] type` occurrence has no struct and stays planned) |
| `image` (in `[container]`) | `distro` + `version` | markdown fixed to the real field pair |

### 2.2 (c) Not a config key — noted, text kept

| Token | Verdict | Action |
|---|---|---|
| `created`, `updated` | container/network/volume metadata, not a struct field | removed from the active TOML and recorded as planned (shown here as metadata, not config) |
| env-var names, labels, annotations, syscall names, mount/tmpfs/device/ulimit paths | dynamic map entries / example values | left untouched; deliberately excluded by the extractor |

### 2.3 (b) Planned — absent fields, marked planned (no field invented)

Every key below is declared with a `# planned:` line in the docs **and** carries the same reason in
the test's `PLANNED` allowlist. 112 entries:

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

`crates/pn-gateway/tests/doc_key_drift.rs` (new file, DE-owned):

* `documented_config_keys_are_real_or_planned` — extracts every documented key from the listed
  files and asserts each is either a real field of a deserialisable struct reachable from this
  crate (the field set is derived by serialising the structs — never hand-copied) **or** listed in
  the `PLANNED` allowlist. Fails with the offending key(s) named.
* `planned_allowlist_matches_documented_declarations` — asserts set equality between the
  `# planned:` declarations in the docs and the `PLANNED` allowlist (they cannot drift apart).

**Scope is stated in the test's own header comment and here:** it covers the ```toml blocks of
`DOC_FILES` = `docs/src/configuration.md`, `docs/src/architecture.md`, `docs/src/cli-reference.md`
and nothing else. `architecture.md` yields zero keys (no toml blocks); its prose/type-sketch fixes
are **not** test-covered.

**Ceiling (deliberate, documented in the test):** the guard matches key **names**, not full nest-
ing paths. A documented key whose leaf name matches a real field is accepted even if its section
path is wrong (e.g. `[network.config] subnet` should be a flat `subnet`). Path inference from prose
headings is not decidable; this false-negative class is accepted rather than crying wolf.

## 4. Negative control (raw — the important part)

A fake key `drift_probe_unknown_key = 1` was **temporarily** added to the `[gateway.metrics]` block
of `docs/src/configuration.md`, the test re-run, then the probe reverted.

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

The probe was then removed; `grep drift_probe_unknown_key docs/` → no match.

## 5. Green run (raw)

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

`env -u LD_LIBRARY_PATH cargo fmt --all` → clean (exit 0); `git status` after the run shows only
the three DE-owned doc files modified plus the new `crates/pn-gateway/tests/` directory.

---

## 6. What was NOT reconciled (honest)

`docs/src/*.md` files that contain ```toml config blocks and were **not** touched this round:

| File | toml blocks | Why not |
|---|---|---|
| `docs/src/advanced.md` | 5 | out of the three-file scope chosen to finish completely |
| `docs/src/api-reference.md` | 1 | contains a containerd runtime blob (`external`, not a PDN key) |
| `docs/src/migration-guide.md` | 5 | same container/profile surface as configuration.md; not reconciled |
| `docs/src/security.md` | 23 | largest untackled block; `gateway.guardrails.*`, `container.security.*` |
| `docs/src/troubleshooting.md` | 1 | `container.resources.io_*` examples |

The guard **does not check these files** — they are not in `DOC_FILES`. Their drift is unchanged
and still described by `research/DOCS_CONFIG_DRIFT_AUDIT.md`. A natural next step is to extend
`DOC_FILES` and the extractor to those files once they are reconciled.

Also not test-covered: `docs/src/architecture.md` (no toml blocks) — its `GuardrailsConfig`,
`HealthCheck` and `ProotConfig` type sketches were corrected by hand and are only verified by
reading.

**Not a fix this round:** the global `config.toml` still has no loader (`pn-gateway` has no `toml`
dependency); the docs now say so at the top of the global example. Whether to implement a loader is
a maintainer decision.

**Not verifiable here:** an end-to-end run of `podnext` against a documented `config.toml` — there
is no loader to exercise, so the inertness is structural, not observed at runtime.

`scripts/build_issue_ledger.py` was **not** modified. This guard would be a natural addition to that
harness (it is repo-wide in intent and already runs the crate tests) — flagging it rather than
editing an unowned file.

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
* `~/.hermes/logs/heavy/20261104T192841Z-de-docdrift-green1.log` (guard green)
* `~/.hermes/logs/heavy/20261104T192901Z-de-docdrift-alltests.log` (full crate suite green)
* `~/.hermes/logs/heavy/20261104T192941Z-de-docdrift-NEGATIVE.log` (negative control FAIL, exit 101)

## Follow-up (this increment)

The gap named in the scope section ("drift left explicitly untouched") is now bounded by the guard
itself rather than by prose. `crates/pn-gateway/tests/doc_key_drift.rs` scans **every** `docs/src/*.md`
file that contains a TOML block (8 files, was 3), so drift in `security.md`, `advanced.md`,
`migration-guide.md`, `api-reference.md` and `troubleshooting.md` fails CI instead of going unseen.

Two mechanisms keep that honest instead of inventing configuration:

* `# not-pdn-config:` inside a block marks documentation of *another* tool's file (a containerd
  `config.toml`, an OCI runtime-spec or image-layout example, the previous pn-next schema). Those
  blocks are skipped entirely: `BinaryName`, `runtime_type` and `layers` are not this crate's keys.
* `# planned: k1, k2` declares documented keys with no backing field; each is listed in `PLANNED`
  with its reason. The security/quota controls in `security.md` (`uid_map`, `gid_map`, `allowed`,
  `blocked`, `burst`, `requests_per_minute`, `containers_per_user`, `cpu_per_user`, `memory_per_user`,
  `memory_swappiness`, `networks_per_user`, `volumes_per_user`) are pinned that way: documented,
  not implemented, and visible as such.

Both guard tests pass: `cargo test -p pn-gateway --locked --test doc_key_drift` -> `2 passed; 0 failed`.
