# Documentation ↔ Configuration Drift Audit

**Scope:** every configuration example in `docs/src/*.md`. This set holds fenced TOML blocks and `[section]` tables. The audit checked each key against the configurability structs that exist in `crates/`.
**Trigger:** issue #27, which states that the documented gateway fail-safe config does not exist in code.
**Status:** read-only audit. No product code was changed.
**`docs/book/` is excluded from every search.** `docs/book/searchindex-*.js` is a single ~1.4 MB line that swamps grep output.

---

## Summary

Issue #27 is not a `[gateway.fail_safe]` problem. It is a **whole-surface** problem:

* **261** distinct configuration keys are documented across `docs/src/*.md`. These keys have **365** documented occurrences, with one count per file that holds the key.
* **227** of them, or 87 %, do not map to any real struct field. There are **199 absent** and **28 renamed**.
* **28** map cleanly.
* **6** are external-tool config, not PDN keys. The containerd `plugins."io.containerd..."` keys are the example.
* **`deny_unknown_fields` appears 0 times in the entire `crates/` tree.** So an unknown key is silently discarded wherever a struct *is* deserialized. Nothing makes a wrong key fail loudly.
* **`pn-gateway` has no `toml` dependency at all.** **No crate loads a global `config.toml`.**
  The only TOML files that any crate reads are `networks.toml`, `volumes.toml`, supervisor *service* TOMLs, a socket config, and a policy-engine file.
  The "Global Configuration" example in `configuration.md:18-152` lists `[general]`, `[gateway]`, `[gateway.fail_safe]`, `[gateway.guardrails]`, `[gateway.health]`, `[gateway.metrics]`, `[grpc]`, `[api]`, `[tracing]`, and `[plugin]`.
  That example is **inert by construction**. Nothing reads that file, so those keys have neither effect nor error.

The nine keys in issue #27 are the visible corner of a 227-key drift.

---

## Method

1. Extract every fenced TOML block and every `[section]` or `key = value` line from `docs/src/*.md`.
   The set has 12 files and 43 TOML-ish blocks. Exclude `docs/book/`.
2. For each documented key, locate the target struct and the real field.
   Read the defining struct in the crate that owns that surface.
3. Record the documented key, the documented location, the real field or none, the struct location, and the verdict.
   The verdict is `match`, `renamed`, `absent`, or `external`.
4. For each non-matching key, decide the user-visible result. The result is either a silent ignore or a hard error.
   Use the serde setup and the code path for that decision.

Evidence commands, all reproducible:

```
grep -rn 'deny_unknown_fields' crates/                       # 0 hits
grep -rn 'toml' crates/*/Cargo.toml                          # pn-next, podnext, proot-distro-core only - NOT pn-gateway
grep -rn 'config\.toml' crates/ --include='*.rs'             # only /etc/containerd/config.toml
grep -rn 'toml::from_str' crates/ --include='*.rs'           # networks.toml, volumes.toml, service/socket/policy TOML
```

### Struct locations for the real field column

| Surface | Struct | Location | Deserialized from a user file? |
|---|---|---|---|
| `[gateway.fail_safe]` | `FailSafeConfig` | `crates/pn-gateway/src/config.rs:190` | **No** — crate has no `toml` dep |
| `[gateway.guardrails]` | `GuardrailConfig` | `crates/pn-gateway/src/config.rs:259` | **No** |
| `[gateway.health]` | `HealthConfig` | `crates/pn-gateway/src/config.rs:340` | **No** |
| `[gateway.metrics]` | `MetricsConfig` | `crates/pn-gateway/src/config.rs:436` | **No** |
| `[container]` | `ContainerConfig` | `crates/proot-distro-core/src/container.rs:352` | **No** — created via CLI/D-Bus |
| `[container.resources]` | `ResourceLimits` | `crates/proot-distro-core/src/container.rs:410` | No |
| `[container.security]` | `SecurityConfig` | `crates/proot-distro-core/src/container.rs:467` | No |
| `[container.network]` | `NetworkConfig` | `crates/proot-distro-core/src/container.rs:429` | No |
| `[container.gpu]` | `GpuConfig` | `crates/proot-distro-core/src/gpu.rs:35` | No |
| `[profile]` | `SecurityProfileConfig` | `crates/proot-compat/src/security.rs:14` | **No** — registered programmatically |
| `[profile.seccomp]` | `SeccompProfile` | `crates/proot-compat/src/security.rs:56` | No |
| `[network]` | `NetworkConfig` (persisted) | `crates/proot-distro-core/src/network.rs:10` | **Yes** — `networks.toml` |
| `[storage]` | `VolumeConfig` (persisted) | `crates/proot-distro-core/src/storage.rs:10` | **Yes** — `volumes.toml` |

---

## 1. Root cause: the documented global config surface has no loader

Two independent facts make the whole `configuration.md` global example inert:

1. **`pn-gateway` cannot read TOML.** `toml` is declared in `pn-next`, `podnext`, and `proot-distro-core` Cargo.toml. No other crate declares it.
   `pn-gateway` builds its config programmatically. The builder is `GatewayConfigBuilder` at `crates/pn-gateway/src/config.rs:564`.
2. **No crate loads `$HOME/.config/pn-next/config.toml`.** The only TOML reads in the tree are `networks.toml`, `volumes.toml`, supervisor service files, a socket file, and a policy file.
   The directory tree in `getting-started.md:145` lists a `config.toml` that nothing opens.

Every documented key in `[general]`, `[gateway*]`, `[grpc*]`, `[api*]`, `[tracing*]`, and `[plugin*]` is **unreachable**, not merely mismatched.
A user who writes such a file gets no effect and no error. The file is never parsed.

Only two surfaces land on a *live* loader. They are `[network]` or `[network.config]` with `networks.toml`, and `[storage]` or `[storage.config]` with `volumes.toml`.
A key mismatch matters on these surfaces, because code parses those files. Section 3 gives the detail.

---

## 2. Full key table

Verdicts: **match** means that the documented key equals a real field name. **renamed** means that a real field exists under a different name, path, or shape.
**absent** means that no corresponding field exists anywhere.
**external** means that the key is not a PDN config key. It is a containerd runtime config key.

| Documented key | Documented at (file:line) | Real field / struct.field | crate:file:line | Verdict |
|---|---|---|---|---|
| `container.gpu.devices` | advanced.md:246, cli-reference.md:606, configuration.md:300, migration-guide.md:293 | GpuConfig.devices | crates/proot-distro-core/src/gpu.rs:35 | match |
| `container.gpu.enabled` | advanced.md:245, cli-reference.md:605, configuration.md:299, migration-guide.md:292 | GpuConfig.enabled | crates/proot-distro-core/src/gpu.rs:35 | match |
| `container.name` | cli-reference.md:570, configuration.md:160, migration-guide.md:120 | ContainerConfig.name | crates/proot-distro-core/src/container.rs:352 | match |
| `container.network.mode` | cli-reference.md:601, configuration.md:288, security.md:617 | NetworkConfig.mode | crates/proot-distro-core/src/container.rs:429 | match |
| `container.resources.cpu_limit` | cli-reference.md:588, configuration.md:260, migration-guide.md:137, security.md:476, security.md:607, troubleshooting.md:374 | ResourceLimits.cpu_limit | crates/proot-distro-core/src/container.rs:410 | match |
| `container.resources.memory_limit` | cli-reference.md:587, configuration.md:257, migration-guide.md:136, security.md:466, security.md:606, troubleshooting.md:373 | ResourceLimits.memory_limit | crates/proot-distro-core/src/container.rs:410 | match |
| `container.resources.pids_limit` | cli-reference.md:589, configuration.md:266, migration-guide.md:138, security.md:499, security.md:608, troubleshooting.md:375 | ResourceLimits.pids_limit | crates/proot-distro-core/src/container.rs:410 | match |
| `container.security.no_new_privileges` | cli-reference.md:596, configuration.md:275, migration-guide.md:142, security.md:442 | SecurityConfig.no_new_privileges | crates/proot-distro-core/src/container.rs:467 | match |
| `container.security.read_only_rootfs` | cli-reference.md:597, configuration.md:276, migration-guide.md:143, security.md:402, security.md:580 | SecurityConfig.read_only_rootfs | crates/proot-distro-core/src/container.rs:467 | match |
| `gateway.health.enabled` | configuration.md:97 | HealthConfig.enabled | crates/pn-gateway/src/config.rs:340 | match |
| `gateway.health.unhealthy_threshold` | configuration.md:101 | HealthConfig.unhealthy_threshold | crates/pn-gateway/src/config.rs:340 | match |
| `gateway.metrics.enabled` | configuration.md:105 | MetricsConfig.enabled | crates/pn-gateway/src/config.rs:436 | match |
| `gateway.metrics.interval` | configuration.md:108 | MetricsConfig.interval | crates/pn-gateway/src/config.rs:436 | match |
| `network.driver` | configuration.md:466 | NetworkConfig.driver | crates/proot-distro-core/src/network.rs:10 | match |
| `network.name` | configuration.md:465 | NetworkConfig.name | crates/proot-distro-core/src/network.rs:10 | match |
| `profile.apparmor.enforce` | configuration.md:379, migration-guide.md:220 | AppArmorProfile.enforce | crates/proot-compat/src/security.rs:147 | match |
| `profile.capabilities.add` | configuration.md:372, configuration.md:453, migration-guide.md:216 | CapabilityConfig.add | crates/proot-compat/src/security.rs:165 | match |
| `profile.capabilities.drop` | configuration.md:358, configuration.md:452, migration-guide.md:212 | CapabilityConfig.drop | crates/proot-compat/src/security.rs:165 | match |
| `profile.enabled` | configuration.md:324, configuration.md:421, configuration.md:440, migration-guide.md:205, security.md:117 | SecurityProfileConfig.enabled | crates/proot-compat/src/security.rs:14 | match |
| `profile.name` | configuration.md:322, configuration.md:419, configuration.md:438, migration-guide.md:203, security.md:115 | SecurityProfileConfig.name | crates/proot-compat/src/security.rs:14 | match |
| `profile.no_new_privileges` | configuration.md:326, configuration.md:423, configuration.md:442 | SecurityProfileConfig.no_new_privileges | crates/proot-compat/src/security.rs:14 | match |
| `profile.read_only_rootfs` | configuration.md:327, configuration.md:424, configuration.md:443 | SecurityProfileConfig.read_only_rootfs | crates/proot-compat/src/security.rs:14 | match |
| `profile.seccomp.architectures` | configuration.md:332, configuration.md:429, configuration.md:448, migration-guide.md:209, security.md:59, security.md:78, security.md:106, security.md:121 | SeccompProfile.architectures | crates/proot-compat/src/security.rs:56 | match |
| `profile.seccomp.default_action` | configuration.md:331, configuration.md:428, configuration.md:447, migration-guide.md:208, security.md:58, security.md:77, security.md:105, security.md:120 | SeccompProfile.default_action | crates/proot-compat/src/security.rs:56 | match |
| `profile.selinux.context` | configuration.md:389, migration-guide.md:223 | SelLinuxProfile.context | crates/proot-compat/src/security.rs:155 | match |
| `profile.user_namespace` | configuration.md:328, configuration.md:425, configuration.md:444 | SecurityProfileConfig.user_namespace | crates/proot-compat/src/security.rs:14 | match |
| `storage.driver` | configuration.md:527 | VolumeConfig.driver | crates/proot-distro-core/src/storage.rs:10 | match |
| `storage.name` | configuration.md:526 | VolumeConfig.name | crates/proot-distro-core/src/storage.rs:10 | match |
| `container.config.env.HOME` | configuration.md:180 | ContainerConfig.env_vars (Vec<(k,v)>, not a map) | crates/proot-distro-core/src/container.rs:359 | renamed |
| `container.config.env.LANG` | cli-reference.md:580, configuration.md:177, migration-guide.md:129 | ContainerConfig.env_vars (Vec<(k,v)>, not a map) | crates/proot-distro-core/src/container.rs:359 | renamed |
| `container.config.env.LC_ALL` | configuration.md:178 | ContainerConfig.env_vars (Vec<(k,v)>, not a map) | crates/proot-distro-core/src/container.rs:359 | renamed |
| `container.config.env.PATH` | configuration.md:179 | ContainerConfig.env_vars (Vec<(k,v)>, not a map) | crates/proot-distro-core/src/container.rs:359 | renamed |
| `container.config.env.SHELL` | configuration.md:181 | ContainerConfig.env_vars (Vec<(k,v)>, not a map) | crates/proot-distro-core/src/container.rs:359 | renamed |
| `container.config.env.TERM` | cli-reference.md:579, configuration.md:176, migration-guide.md:128 | ContainerConfig.env_vars (Vec<(k,v)>, not a map) | crates/proot-distro-core/src/container.rs:359 | renamed |
| `container.config.hostname` | cli-reference.md:574, configuration.md:167, migration-guide.md:124 | ContainerConfig.hostname (no [container.config] table) | crates/proot-distro-core/src/container.rs:352 | renamed |
| `container.config.mounts."/data/data/com.termux/files/home"` | cli-reference.md:584, configuration.md:204, migration-guide.md:133 | ContainerConfig.bind_mounts (Vec<BindMount>) | crates/proot-distro-core/src/container.rs:358 | renamed |
| `container.config.mounts."/sdcard"` | cli-reference.md:583, configuration.md:203, migration-guide.md:132 | ContainerConfig.bind_mounts (Vec<BindMount>) | crates/proot-distro-core/src/container.rs:358 | renamed |
| `container.config.mounts."/tmp"` | configuration.md:205 | ContainerConfig.bind_mounts (Vec<BindMount>) | crates/proot-distro-core/src/container.rs:358 | renamed |
| `container.config.working_dir` | cli-reference.md:575, configuration.md:169, migration-guide.md:125 | ContainerConfig.working_dir (no [container.config] table) | crates/proot-distro-core/src/container.rs:352 | renamed |
| `container.network.dns` | configuration.md:290 | NetworkConfig.dns_servers / port_mappings | crates/proot-distro-core/src/container.rs:429 | renamed |
| `container.network.ports` | configuration.md:294 | NetworkConfig.dns_servers / port_mappings | crates/proot-distro-core/src/container.rs:429 | renamed |
| `container.security.profile` | cli-reference.md:595, configuration.md:274, migration-guide.md:141 | SecurityConfig.seccomp_profile | crates/proot-distro-core/src/container.rs:470 | renamed |
| `gateway.fail_safe.circuit_breaker_threshold` | configuration.md:73 | FailSafeConfig.circuit_breaker.failure_threshold | crates/pn-gateway/src/config.rs:190 | renamed |
| `gateway.fail_safe.circuit_breaker_timeout` | configuration.md:74 | FailSafeConfig.circuit_breaker.timeout | crates/pn-gateway/src/config.rs:190 | renamed |
| `gateway.fail_safe.retry_max_attempts` | configuration.md:75 | FailSafeConfig.max_retries | crates/pn-gateway/src/config.rs:190 | renamed |
| `gateway.health.interval` | configuration.md:98 | HealthConfig.check_interval | crates/pn-gateway/src/config.rs:340 | renamed |
| `network.config.dns` | configuration.md:480 | NetworkConfig.dns_servers | crates/proot-distro-core/src/network.rs:10 | renamed |
| `network.config.enable_ipv6` | configuration.md:475 | NetworkConfig.enable_ipv6 (flat, no [network.config]) | crates/proot-distro-core/src/network.rs:10 | renamed |
| `network.config.gateway` | configuration.md:472 | NetworkConfig.gateway (flat, no [network.config]) | crates/proot-distro-core/src/network.rs:10 | renamed |
| `network.config.mtu` | configuration.md:474 | NetworkConfig.mtu (flat, no [network.config]) | crates/proot-distro-core/src/network.rs:10 | renamed |
| `network.config.subnet` | configuration.md:471 | NetworkConfig.subnet (flat, no [network.config]) | crates/proot-distro-core/src/network.rs:10 | renamed |
| `profile.apparmor.profile` | configuration.md:378, migration-guide.md:219 | AppArmorProfile.profile_name | crates/proot-compat/src/security.rs:147 | renamed |
| `profile.seccomp.allowed` | security.md:81 | SeccompProfile.syscalls (Vec<SeccompSyscallRule>) | crates/proot-compat/src/security.rs:56 | renamed |
| `profile.seccomp.blocked` | security.md:62 | SeccompProfile.syscalls (Vec<SeccompSyscallRule>) | crates/proot-compat/src/security.rs:56 | renamed |
| `profile.seccomp.rules` | configuration.md:431, configuration.md:449 | SeccompProfile.syscalls (Vec<SeccompSyscallRule>) | crates/proot-compat/src/security.rs:56 | renamed |
| `profile.type` | configuration.md:323, configuration.md:420, configuration.md:439, migration-guide.md:204, security.md:116 | SecurityProfileConfig.profile_type | crates/proot-compat/src/security.rs:14 | renamed |
| `api.server.addr` | configuration.md:129 | — (no such struct in pn-gateway) | — | absent |
| `api.server.cors_allowed_headers` | configuration.md:132 | — (no such struct in pn-gateway) | — | absent |
| `api.server.cors_allowed_methods` | configuration.md:131 | — (no such struct in pn-gateway) | — | absent |
| `api.server.cors_allowed_origins` | configuration.md:130 | — (no such struct in pn-gateway) | — | absent |
| `api.server.cors_max_age` | configuration.md:133 | — (no such struct in pn-gateway) | — | absent |
| `api.server.enabled` | configuration.md:128 | — (no such struct in pn-gateway) | — | absent |
| `container.checkpoint.compress` | advanced.md:183 | — | — | absent |
| `container.checkpoint.enabled` | advanced.md:179, configuration.md:304 | — | — | absent |
| `container.checkpoint.encrypt` | advanced.md:184 | — | — | absent |
| `container.checkpoint.interval` | advanced.md:180, configuration.md:305 | — | — | absent |
| `container.checkpoint.max_checkpoints` | advanced.md:181, configuration.md:306 | — | — | absent |
| `container.checkpoint.storage` | advanced.md:182, configuration.md:307 | — | — | absent |
| `container.config.annotations."org.opencontainers.image.title"` | configuration.md:189 | — (no such field under ContainerConfig) | — | absent |
| `container.config.annotations."org.opencontainers.image.version"` | configuration.md:190 | — (no such field under ContainerConfig) | — | absent |
| `container.config.annotations.cmd` | configuration.md:194 | — (no such field under ContainerConfig) | — | absent |
| `container.config.annotations.entrypoint` | configuration.md:193 | — (no such field under ContainerConfig) | — | absent |
| `container.config.annotations.stop_signal` | configuration.md:197 | — (no such field under ContainerConfig) | — | absent |
| `container.config.annotations.stop_timeout` | configuration.md:198 | — (no such field under ContainerConfig) | — | absent |
| `container.config.apparmor.enforce` | configuration.md:250 | — (no such field under ContainerConfig) | — | absent |
| `container.config.apparmor.profile` | configuration.md:249 | — (no such field under ContainerConfig) | — | absent |
| `container.config.capabilities.add` | configuration.md:242, security.md:231, security.md:592 | — (no such field under ContainerConfig) | — | absent |
| `container.config.capabilities.drop` | configuration.md:234, security.md:228, security.md:587 | — (no such field under ContainerConfig) | — | absent |
| `container.config.devices."/dev/dri/card0"` | configuration.md:220 | — (no such field under ContainerConfig) | — | absent |
| `container.config.devices."/dev/dri/renderD128"` | configuration.md:219 | — (no such field under ContainerConfig) | — | absent |
| `container.config.domainname` | configuration.md:168 | — | — | absent |
| `container.config.group` | configuration.md:171 | — | — | absent |
| `container.config.groups` | configuration.md:172 | — | — | absent |
| `container.config.labels."org.pdn.description"` | configuration.md:185 | — (no such field under ContainerConfig) | — | absent |
| `container.config.labels."org.pdn.maintainer"` | configuration.md:186 | — (no such field under ContainerConfig) | — | absent |
| `container.config.path_translation.guest_prefix` | migration-guide.md:319 | — (no such field under ContainerConfig) | — | absent |
| `container.config.path_translation.host_prefix` | migration-guide.md:318 | — (no such field under ContainerConfig) | — | absent |
| `container.config.seccomp.custom_rules` | configuration.md:246 | — (no such field under ContainerConfig) | — | absent |
| `container.config.seccomp.profile` | configuration.md:245 | — (no such field under ContainerConfig) | — | absent |
| `container.config.selinux.context` | configuration.md:253 | — (no such field under ContainerConfig) | — | absent |
| `container.config.selinux.enforce` | configuration.md:254 | — (no such field under ContainerConfig) | — | absent |
| `container.config.sysctls."kernel.shmmax"` | configuration.md:225 | — (no such field under ContainerConfig) | — | absent |
| `container.config.sysctls."net.ipv4.ip_forward"` | configuration.md:224 | — (no such field under ContainerConfig) | — | absent |
| `container.config.tmpfs."/run"` | configuration.md:214 | — (no such field under ContainerConfig) | — | absent |
| `container.config.tmpfs."/tmp"` | configuration.md:215 | — (no such field under ContainerConfig) | — | absent |
| `container.config.ulimits."nofile"` | configuration.md:229 | — (no such field under ContainerConfig) | — | absent |
| `container.config.ulimits."nproc"` | configuration.md:230 | — (no such field under ContainerConfig) | — | absent |
| `container.config.user` | cli-reference.md:576, configuration.md:170 | — | — | absent |
| `container.config.volumes."my-data"` | configuration.md:209 | — (no such field under ContainerConfig) | — | absent |
| `container.config.volumes."shared-cache"` | configuration.md:210 | — (no such field under ContainerConfig) | — | absent |
| `container.created` | configuration.md:162 | — | — | absent |
| `container.gpu.api` | advanced.md:250, cli-reference.md:607, configuration.md:301, migration-guide.md:294 | — (no `api`; use vulkan/opengl/drm/compute bools) | — | absent |
| `container.gpu.opengl.egl_platforms` | advanced.md:261 | — | — | absent |
| `container.gpu.vulkan.icd_paths` | advanced.md:254 | — | — | absent |
| `container.gpu.vulkan.layers` | advanced.md:258 | — | — | absent |
| `container.hooks.poststart` | configuration.md:311 | — | — | absent |
| `container.hooks.poststop` | configuration.md:313 | — | — | absent |
| `container.hooks.prestart` | configuration.md:310 | — | — | absent |
| `container.hooks.prestop` | configuration.md:312 | — | — | absent |
| `container.image` | advanced.md:342, cli-reference.md:571, configuration.md:161, migration-guide.md:121 | — (real: distro+version) | — | absent |
| `container.network.dns_options` | configuration.md:292 | — | — | absent |
| `container.network.dns_search` | configuration.md:291 | — | — | absent |
| `container.network.extra_hosts` | configuration.md:293 | — | — | absent |
| `container.network.ipc_mode` | configuration.md:295 | — | — | absent |
| `container.network.networks` | cli-reference.md:602, configuration.md:289, security.md:618 | — | — | absent |
| `container.network.service_mesh.config` | advanced.md:447 | — | — | absent |
| `container.network.service_mesh.enabled` | advanced.md:445 | — | — | absent |
| `container.network.service_mesh.sidecar` | advanced.md:446 | — | — | absent |
| `container.network.uts_mode` | configuration.md:296 | — | — | absent |
| `container.platform."linux/amd64".config.env.ARCH` | advanced.md:351 | — | — | absent |
| `container.platform."linux/amd64".resources.memory_limit` | advanced.md:350 | — | — | absent |
| `container.platform."linux/arm64".config.env.ARCH` | advanced.md:347 | — | — | absent |
| `container.platform."linux/arm64".resources.memory_limit` | advanced.md:346 | — | — | absent |
| `container.resources.cpu_period` | configuration.md:263, security.md:479 | — | — | absent |
| `container.resources.cpu_quota` | configuration.md:262, security.md:478 | — | — | absent |
| `container.resources.cpu_shares` | configuration.md:261, security.md:477 | — | — | absent |
| `container.resources.cpuset_cpus` | configuration.md:264, security.md:480 | — | — | absent |
| `container.resources.cpuset_mems` | configuration.md:265, security.md:481 | — | — | absent |
| `container.resources.io_max_read` | cli-reference.md:591 | — | — | absent |
| `container.resources.io_max_read_bps` | configuration.md:268, security.md:489, security.md:609, troubleshooting.md:376 | — | — | absent |
| `container.resources.io_max_read_iops` | configuration.md:270, security.md:491 | — | — | absent |
| `container.resources.io_max_write` | cli-reference.md:592 | — | — | absent |
| `container.resources.io_max_write_bps` | configuration.md:269, security.md:490, security.md:610, troubleshooting.md:377 | — | — | absent |
| `container.resources.io_max_write_iops` | configuration.md:271, security.md:492 | — | — | absent |
| `container.resources.io_weight` | cli-reference.md:590, configuration.md:267, security.md:488 | — | — | absent |
| `container.resources.memory_reservation` | configuration.md:259, security.md:468 | — | — | absent |
| `container.resources.memory_swap_limit` | configuration.md:258, security.md:467 | — | — | absent |
| `container.resources.memory_swappiness` | security.md:469 | — | — | absent |
| `container.security.gid_map` | security.md:457 | — | — | absent |
| `container.security.mask_paths` | configuration.md:278 | — | — | absent |
| `container.security.mask_paths.paths` | security.md:415 | — | — | absent |
| `container.security.readonly_paths` | configuration.md:282 | — | — | absent |
| `container.security.readonly_paths.paths` | security.md:430 | — | — | absent |
| `container.security.uid_map` | security.md:456 | — | — | absent |
| `container.security.user_namespace` | cli-reference.md:598, configuration.md:277, security.md:455, security.md:599 | — | — | absent |
| `container.updated` | configuration.md:163 | — | — | absent |
| `gateway.fail_safe.bulkhead_max_concurrent` | configuration.md:78 | — | — | absent |
| `gateway.fail_safe.bulkhead_queue_size` | configuration.md:79 | — | — | absent |
| `gateway.fail_safe.retry_base_delay` | configuration.md:76 | — | — | absent |
| `gateway.fail_safe.retry_max_delay` | configuration.md:77 | — | — | absent |
| `gateway.guardrails.allowed_images` | configuration.md:87, security.md:512 | — (only enabled/rules/strict_mode/audit_log exist) | — | absent |
| `gateway.guardrails.blocked_images` | configuration.md:88, security.md:513 | — (only enabled/rules/strict_mode/audit_log exist) | — | absent |
| `gateway.guardrails.blocked_syscalls` | configuration.md:90, security.md:515 | — (only enabled/rules/strict_mode/audit_log exist) | — | absent |
| `gateway.guardrails.max_containers` | configuration.md:83, security.md:508 | — (only enabled/rules/strict_mode/audit_log exist) | — | absent |
| `gateway.guardrails.max_containers_per_user` | configuration.md:86, security.md:511 | — (only enabled/rules/strict_mode/audit_log exist) | — | absent |
| `gateway.guardrails.max_cpu_total` | configuration.md:85, security.md:510 | — (only enabled/rules/strict_mode/audit_log exist) | — | absent |
| `gateway.guardrails.max_memory_total` | configuration.md:84, security.md:509 | — (only enabled/rules/strict_mode/audit_log exist) | — | absent |
| `gateway.guardrails.quotas.containers_per_user` | security.md:532 | — (only enabled/rules/strict_mode/audit_log exist) | — | absent |
| `gateway.guardrails.quotas.cpu_per_user` | security.md:530 | — (only enabled/rules/strict_mode/audit_log exist) | — | absent |
| `gateway.guardrails.quotas.memory_per_user` | security.md:531 | — (only enabled/rules/strict_mode/audit_log exist) | — | absent |
| `gateway.guardrails.quotas.networks_per_user` | security.md:534 | — (only enabled/rules/strict_mode/audit_log exist) | — | absent |
| `gateway.guardrails.quotas.volumes_per_user` | security.md:533 | — (only enabled/rules/strict_mode/audit_log exist) | — | absent |
| `gateway.guardrails.rate_limit.burst` | security.md:523 | — (only enabled/rules/strict_mode/audit_log exist) | — | absent |
| `gateway.guardrails.rate_limit.requests_per_minute` | security.md:522 | — (only enabled/rules/strict_mode/audit_log exist) | — | absent |
| `gateway.guardrails.required_security_profile` | configuration.md:89, security.md:514 | — (only enabled/rules/strict_mode/audit_log exist) | — | absent |
| `gateway.health.healthy_threshold` | configuration.md:100 | — | — | absent |
| `gateway.health.timeout` | configuration.md:99 | — | — | absent |
| `gateway.metrics.path` | configuration.md:107 | — (no port/path; exporters replaced) | — | absent |
| `gateway.metrics.port` | configuration.md:106 | — (no port/path; exporters replaced) | — | absent |
| `general.cache_dir` | configuration.md:24 | — (no such struct in pn-gateway) | — | absent |
| `general.config_dir` | configuration.md:25 | — (no such struct in pn-gateway) | — | absent |
| `general.data_dir` | configuration.md:23 | — (no such struct in pn-gateway) | — | absent |
| `general.default_cpu_limit` | configuration.md:46 | — (no such struct in pn-gateway) | — | absent |
| `general.default_dns` | configuration.md:51 | — (no such struct in pn-gateway) | — | absent |
| `general.default_dns_search` | configuration.md:52 | — (no such struct in pn-gateway) | — | absent |
| `general.default_memory_limit` | configuration.md:45 | — (no such struct in pn-gateway) | — | absent |
| `general.default_network` | configuration.md:50 | — (no such struct in pn-gateway) | — | absent |
| `general.default_pids_limit` | configuration.md:47 | — (no such struct in pn-gateway) | — | absent |
| `general.default_registry` | configuration.md:59 | — (no such struct in pn-gateway) | — | absent |
| `general.default_runtime` | configuration.md:41 | — (no such struct in pn-gateway) | — | absent |
| `general.default_security_profile` | configuration.md:42 | — (no such struct in pn-gateway) | — | absent |
| `general.default_storage_driver` | configuration.md:55 | — (no such struct in pn-gateway) | — | absent |
| `general.default_volume_driver` | configuration.md:56 | — (no such struct in pn-gateway) | — | absent |
| `general.enable_criu` | configuration.md:64 | — (no such struct in pn-gateway) | — | absent |
| `general.enable_gpu` | configuration.md:65 | — (no such struct in pn-gateway) | — | absent |
| `general.enable_metrics` | configuration.md:67 | — (no such struct in pn-gateway) | — | absent |
| `general.enable_plugins` | configuration.md:66 | — (no such struct in pn-gateway) | — | absent |
| `general.enable_tracing` | configuration.md:68 | — (no such struct in pn-gateway) | — | absent |
| `general.gateway_addr` | configuration.md:36 | — (no such struct in pn-gateway) | — | absent |
| `general.gateway_retry_attempts` | configuration.md:38 | — (no such struct in pn-gateway) | — | absent |
| `general.gateway_timeout` | configuration.md:37 | — (no such struct in pn-gateway) | — | absent |
| `general.image_pull_policy` | configuration.md:61 | — (no such struct in pn-gateway) | — | absent |
| `general.insecure_registries` | configuration.md:60 | — (no such struct in pn-gateway) | — | absent |
| `general.log_dir` | configuration.md:26 | — (no such struct in pn-gateway) | — | absent |
| `general.log_file` | configuration.md:31 | — (no such struct in pn-gateway) | — | absent |
| `general.log_format` | configuration.md:30 | — (no such struct in pn-gateway) | — | absent |
| `general.log_level` | configuration.md:29 | — (no such struct in pn-gateway) | — | absent |
| `general.log_max_files` | configuration.md:33 | — (no such struct in pn-gateway) | — | absent |
| `general.log_rotation` | configuration.md:32 | — (no such struct in pn-gateway) | — | absent |
| `grpc.client.max_recv_msg_size` | configuration.md:122 | — (no such struct in pn-gateway) | — | absent |
| `grpc.client.max_send_msg_size` | configuration.md:123 | — (no such struct in pn-gateway) | — | absent |
| `grpc.client.timeout` | configuration.md:121 | — (no such struct in pn-gateway) | — | absent |
| `grpc.server.addr` | configuration.md:113 | — (no such struct in pn-gateway) | — | absent |
| `grpc.server.keepalive_permit_without_stream` | configuration.md:118 | — (no such struct in pn-gateway) | — | absent |
| `grpc.server.keepalive_time` | configuration.md:116 | — (no such struct in pn-gateway) | — | absent |
| `grpc.server.keepalive_timeout` | configuration.md:117 | — (no such struct in pn-gateway) | — | absent |
| `grpc.server.max_recv_msg_size` | configuration.md:114 | — (no such struct in pn-gateway) | — | absent |
| `grpc.server.max_send_msg_size` | configuration.md:115 | — (no such struct in pn-gateway) | — | absent |
| `network.config.dns_search` | configuration.md:481 | — | — | absent |
| `network.config.enable_icc` | configuration.md:485 | — | — | absent |
| `network.config.internal` | configuration.md:484 | — | — | absent |
| `network.config.ip_range` | configuration.md:473 | — | — | absent |
| `network.config.ipv6_gateway` | configuration.md:477 | — | — | absent |
| `network.config.ipv6_subnet` | configuration.md:476 | — | — | absent |
| `network.config.labels."org.pdn.managed"` | configuration.md:490 | — | — | absent |
| `network.config.labels."org.pdn.type"` | configuration.md:489 | — | — | absent |
| `network.created` | configuration.md:467 | — (no `created`) | — | absent |
| `plugin.manager.allow_unsigned` | configuration.md:150 | — (no such struct in pn-gateway) | — | absent |
| `plugin.manager.enabled` | configuration.md:145 | — (no such struct in pn-gateway) | — | absent |
| `plugin.manager.max_plugins` | configuration.md:151 | — (no such struct in pn-gateway) | — | absent |
| `plugin.manager.plugin_dirs` | configuration.md:146 | — (no such struct in pn-gateway) | — | absent |
| `pod.containers` | migration-guide.md:255 | — (no PodConfig struct) | — | absent |
| `pod.name` | migration-guide.md:254 | — (no PodConfig struct) | — | absent |
| `pod.networks.default` | migration-guide.md:258 | — (no PodConfig struct) | — | absent |
| `profile.apparmor.custom_rules` | configuration.md:381 | — | — | absent |
| `profile.description` | configuration.md:325, configuration.md:422, configuration.md:441 | — | — | absent |
| `profile.mask_paths.paths` | configuration.md:398 | — (no mask_paths/readonly_paths on profile) | — | absent |
| `profile.readonly_paths.paths` | configuration.md:407 | — (no mask_paths/readonly_paths on profile) | — | absent |
| `profile.seccomp.rules."arch_prctl"` | security.md:149 | — (rules is a Vec, not a map) | — | absent |
| `profile.seccomp.rules."bpf"` | configuration.md:350, security.md:135 | — (rules is a Vec, not a map) | — | absent |
| `profile.seccomp.rules."clone"` | configuration.md:342, security.md:125 | — (rules is a Vec, not a map) | — | absent |
| `profile.seccomp.rules."delete_module"` | configuration.md:353 | — (rules is a Vec, not a map) | — | absent |
| `profile.seccomp.rules."finit_module"` | configuration.md:352 | — (rules is a Vec, not a map) | — | absent |
| `profile.seccomp.rules."init_module"` | configuration.md:354 | — (rules is a Vec, not a map) | — | absent |
| `profile.seccomp.rules."kcmp"` | configuration.md:351 | — (rules is a Vec, not a map) | — | absent |
| `profile.seccomp.rules."mmap"` | security.md:152 | — (rules is a Vec, not a map) | — | absent |
| `profile.seccomp.rules."openat"` | security.md:129, security.md:143 | — (rules is a Vec, not a map) | — | absent |
| `profile.seccomp.rules."process_vm_readv"` | configuration.md:347 | — (rules is a Vec, not a map) | — | absent |
| `profile.seccomp.rules."process_vm_writev"` | configuration.md:348 | — (rules is a Vec, not a map) | — | absent |
| `profile.seccomp.rules."ptrace"` | configuration.md:346, security.md:134 | — (rules is a Vec, not a map) | — | absent |
| `profile.seccomp.rules."unshare"` | configuration.md:343 | — (rules is a Vec, not a map) | — | absent |
| `profile.seccomp.rules."userfaultfd"` | configuration.md:349 | — (rules is a Vec, not a map) | — | absent |
| `profile.selinux.custom_rules` | configuration.md:392 | — | — | absent |
| `profile.selinux.enforce` | configuration.md:390, migration-guide.md:224 | — | — | absent |
| `storage.config.fs_type` | configuration.md:533 | — | — | absent |
| `storage.config.labels."org.pdn.managed"` | configuration.md:543 | — (VolumeConfig has only name/driver/labels/options) | — | absent |
| `storage.config.labels."org.pdn.type"` | configuration.md:542 | — (VolumeConfig has only name/driver/labels/options) | — | absent |
| `storage.config.mount_options` | configuration.md:534 | — | — | absent |
| `storage.config.path` | configuration.md:532 | — | — | absent |
| `storage.config.quota_enabled` | configuration.md:537 | — | — | absent |
| `storage.config.quota_size` | configuration.md:538 | — | — | absent |
| `storage.created` | configuration.md:528 | — (no `created`) | — | absent |
| `tracing.exporter.endpoint` | configuration.md:139 | — (no such struct in pn-gateway) | — | absent |
| `tracing.exporter.sampling_ratio` | configuration.md:140 | — (no such struct in pn-gateway) | — | absent |
| `tracing.exporter.type` | configuration.md:138 | — (no such struct in pn-gateway) | — | absent |
| `plugins."io.containerd.grpc.v1.cri".containerd.runtimes.pdn.base_runtime_spec` | advanced.md:607 | containerd runtime config | n/a | external |
| `plugins."io.containerd.grpc.v1.cri".containerd.runtimes.pdn.options.BinaryName` | advanced.md:609, api-reference.md:517 | containerd runtime config | n/a | external |
| `plugins."io.containerd.grpc.v1.cri".containerd.runtimes.pdn.options.RootDir` | advanced.md:610, api-reference.md:518 | containerd runtime config | n/a | external |
| `plugins."io.containerd.grpc.v1.cri".containerd.runtimes.pdn.options.SystemdCgroup` | advanced.md:611 | containerd runtime config | n/a | external |
| `plugins."io.containerd.grpc.v1.cri".containerd.runtimes.pdn.privileged_without_host_devices` | advanced.md:606 | containerd runtime config | n/a | external |
| `plugins."io.containerd.grpc.v1.cri".containerd.runtimes.pdn.runtime_type` | advanced.md:605, api-reference.md:515 | containerd runtime config | n/a | external |

**Totals:** 261 keys. The breakdown is 28 match, 28 renamed, 199 absent, and 6 external. That gives 227 non-matching keys.

Non-matching keys per top-level section, absent plus renamed:

| Section | absent+renamed | note |
|---|---|---|
| `container.*` | 98 | largest drift — a documented container spec with almost no backing struct |
| `general.*` | 30 | no `GeneralConfig` struct exists anywhere |
| `gateway.*` | 27 | #27; `fail_safe`/`guardrails`/`health`/`metrics` |
| `profile.*` | 25 | seccomp `rules` shape + mask/readonly paths |
| `network.*` | 14 | persisted `networks.toml` uses a flat schema |
| `grpc.*` | 9 | no gRPC server/client config struct |
| `storage.*` | 8 | persisted `volumes.toml` is `{name,driver,labels,options}` |
| `api.*` | 6 | real `ApiConfig` is `{host,port,...}` under `[api]`, not `[api.server]` |
| `plugin.*` | 4 | real `PluginConfig` has different keys |
| `tracing.*` | 3 | no tracing config struct |
| `pod.*` | 3 | no `PodConfig` struct |

---

## 3. Consequence per missing key

There are exactly two possible fates for a documented key that has no field. Which one applies
depends on whether the owning surface is ever deserialized from a file.

### Part a. Silently ignored - no effect, no error

This fate applies to **every absent or renamed key on a surface that code never loads from a user file**.
It also applies to **every such key on a surface that *is* loaded but lacks `deny_unknown_fields`**.

* `deny_unknown_fields` count in `crates/` = **0**. Serde's default is to ignore unknown fields.
* So the two live loaders, `networks.toml` and `volumes.toml`, drop an unknown key without a message.
* No code loads the gateway, container, or profile surfaces from a file. Their keys are dropped even earlier, because no code reads the file.

This verdict covers the overwhelming majority. A user configures a control that no code applies and no code reports.

### Part b. Hard error - only for a value or type mismatch on a live loader

A hard error needs two conditions. First, code deserializes the surface from a file. Second, the wrong item is a value or type with no accepted alternative.
Two documented examples are wrong in exactly this way. Both sit on surfaces that are live:

* **Wrong enum spelling.** Docs write seccomp architectures as `"SCMP_ARCH_AARCH64"` and actions as `"errno"` or `"kill"`.
  The real enums are `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]`, so the correct literals are `"AARCH64"` and `"ERRNO"` or `"KILL"`. The source lines are `crates/proot-compat/src/security.rs:96,134`.
  A profile file with the documented strings fails with `unknown variant`. **In practice this never fires.**
  Code registers `SecurityProfileConfig` only programmatically, with `register_profile`. No code parses it from TOML.
  The doc seccomp block is unreachable and wrong.
* **Wrong type.** Docs document `[container.config.env]` as a table of `KEY = "value"`.
  The real field is `env_vars: Vec<(String, String)>` at `container.rs:359`. It is an array of pairs, not a map.
  TOML rejects such tables when a parse occurs. Again the result is inert, because no code file-loads container configs.

**Net:** in the current tree **no** documented configuration key produces a hard error.
Code silently ignores each of the 227 non-matching keys. The file is never read, or the deserializer does not deny unknown fields.
The "loud failure" that a user hopes for does not exist.

### Part c. `pn-gateway` only, issue #27

`FailSafeConfig` at `crates/pn-gateway/src/config.rs:190` has `enabled`, `max_retries`, `retry_delay`, `fallback_actions`, `circuit_breaker`, and `auto_recovery`.
Documented `[gateway.fail_safe]` keys have **empty intersection** with it. Their names are `circuit_breaker_threshold`, `circuit_breaker_timeout`, `retry_max_attempts`, `retry_base_delay`, `retry_max_delay`, `bulkhead_max_concurrent`, and `bulkhead_queue_size`.
The same holds for `[gateway.guardrails]`. The real `GuardrailConfig` is `enabled`, `rules`, `strict_mode`, and `audit_log`. Its documented keys are `max_containers`, `max_memory_total`, `max_cpu_total`, `allowed_images`, `blocked_images`, `required_security_profile`, `blocked_syscalls`, plus `[gateway.guardrails.rate_limit]` and `[gateway.guardrails.quotas]`.
Code ignores all of them.

---

## 4. Section-by-section findings

### 4.1 `[general]` - 30/30 keys absent
No `GeneralConfig` struct exists in any crate. The command `grep -rn GeneralConfig crates/` returns nothing.
Directories, logging, defaults, image, and feature-flag keys have no target. Examples are `data_dir`, `log_level`, `default_runtime`, and `enable_criu`.
Code never loads the file, so those keys are inert.

### 4.2 `[gateway]` - 27/37 keys absent or renamed
* `[gateway.fail_safe]`: only 3 of 7 documented keys have *any* relative.
  The three are `retry_max_attempts`->`max_retries`, `circuit_breaker_threshold`->`circuit_breaker.failure_threshold`, and `circuit_breaker_timeout`->`circuit_breaker.timeout`.
  `retry_base_delay`, `retry_max_delay`, `bulkhead_max_concurrent`, and `bulkhead_queue_size` are absent.
  No backoff and no bulkhead exist. The documented safety controls do not exist. See #27.
* `[gateway.guardrails]`: the real struct stores *rules as condition strings*, not the numeric limits in the docs.
  An example is `condition = "container_count > 100"`. The `rate_limit` and `quotas` sub-tables have no struct.
* `[gateway.health]`: `interval`->`check_interval` is renamed. `timeout` and `healthy_threshold` are absent. Only `unhealthy_threshold` exists.
* `[gateway.metrics]`: `port` and `path` are absent. The real `MetricsConfig` is `enabled`, `interval`, `retention`, and `exporters`.

### 4.3 `[container]` - 98 non-matching keys
The documented container spec is far richer than `ContainerConfig`.
The real struct has **no `[container.config]` table**. It is flat, with `hostname`, `working_dir`, `env_vars`, `bind_mounts`, `labels`, and `annotations`.
Keys that are absent outright are `image`, `created` or `updated`, `domainname`, `user`, `group`, `groups`, `entrypoint`, `cmd`, `stop_signal`, `stop_timeout`, `volumes`, and `tmpfs`.
Also missing are `devices`, `sysctls`, `ulimits`, the `seccomp`, `apparmor`, and `selinux` sub-tables, `checkpoint`, `hooks`, `platform`, and `path_translation`.
Also missing are `network.service_mesh` and the `gpu.api`, `gpu.vulkan`, and `gpu.opengl` keys.
`ResourceLimits` has only `cpu_limit`, `memory_limit`, `disk_limit`, and `pids_limit`.
The documented `cpu_shares`, `cpu_quota`, `cpu_period`, `cpuset_cpus`, `cpuset_mems`, `io_*`, `memory_swap_limit`, `memory_reservation`, and `memory_swappiness` are all absent.

### 4.4 `[profile]` - 25 non-matching keys
`SecurityProfileConfig` matches on `name`, `enabled`, `no_new_privileges`, `read_only_rootfs`, and `user_namespace`.
`type`->`profile_type` is renamed. `description` is absent.
For `[profile.seccomp]`, `rules` is documented as a **map keyed by syscall name**. The real field `syscalls` is a `Vec<SeccompSyscallRule>`.
Its shape is `{names:[...], action, args}`. The shape differs, so all 13 `profile.seccomp.rules.*` keys are absent.
`[profile.mask_paths]` and `[profile.readonly_paths]` have no backing field. `[profile.selinux].enforce` is absent. Only `context` matches.

### 4.5 `[network]` and `[storage]` - the two *live* loaders
Code persists these files as `HashMap<String, NetworkConfig>` and `HashMap<String, VolumeConfig>`.
The real flat `NetworkConfig` is `name`, `driver`, `subnet`, `gateway`, `dns_servers`, `enable_ipv6`, `mtu`, `containers`, and `ips`.
The docs wrap the same settings under `[network.config]`. Those names are `subnet`, `gateway`, `mtu`, `enable_ipv6`, and `dns`.
That path is **renamed and different**. Also absent are `ip_range`, `ipv6_*`, `internal`, `enable_icc`, `dns_search`, `labels`, and `created`.
`VolumeConfig` is `name`, `driver`, `labels`, and `options`. Everything under `[storage.config]` is absent.
Those keys are `path`, `fs_type`, `mount_options`, `quota_enabled`, and `quota_size`.
Code deserializes these blocks, but the deserializer drops the extra keys silently, because no `deny_unknown_fields` attribute exists.

### 4.6 External blocks, not PDN keys
`advanced.md` and `api-reference.md` document a containerd runtime config.
The section is `[plugins."io.containerd.grpc.v1.cri".containerd.runtimes.pdn]`.
That schema belongs to containerd, not to a PDN struct. It is out of scope and marked `external`.
The envoy service-mesh blob at `advanced.md:443-487` is also an opaque multi-line string value.

---

## 5. What a maintainer must decide

This audit raises the same two questions as #27, but for the whole surface. The decisions are
maintainer calls, not code facts:

1. **Is each documented section *planned* or *stale*?** For every absent block the maintainer must choose.
   The choice is *implement it* or *mark it as planned* or *delete it*.
   No third option keeps the docs honest, because no path reaches that code today.
2. **Is the config surface loadable at all?** There is no global `config.toml` loader today.
   Decide whether PDN intends to have one. If yes, someone must write the loader.
   The docs already assume that the loader exists.
   If no, the global example in `configuration.md` describes a file that the product never reads. The doc must state this.
3. **Make wrong keys fail loudly.** Add `#[serde(deny_unknown_fields)]` to the structs that *are* loaded.
   These are `NetworkConfig` and `VolumeConfig` in the persisted form, plus the supervisor service config.
   Then a key that does not exist produces an error, not silence. Today 0 structs have it.
4. **Make the docs verifiable.** Add a test that parses every example in `docs/src/` against the real structs.
   The test uses a round-trip or key-presence assertion.
   This guard is absent, and that absence let 227 keys drift. The benchmark suite was only an accidental canary. Issue #27 acknowledges this.
5. **Fix the value spelling too, not only the keys.** The seccomp `architectures` and `default_action` literals are wrong.
   The wrong pairs are `SCMP_ARCH_AARCH64` against `AARCH64`, and `errno` against `ERRNO`.
   A doc-verification test must check enum values, not only key names.
6. **Prioritise by blast radius.** If any of this becomes loadable, the *safety* surfaces matter most.
   The most important items are the bulkhead and retry controls of #27, the guardrail limits, and the security-profile syscall rules.
   No code currently implements or enforces these items.

---

## Certainty boundary

**Verified directly:** the 261-key inventory and the per-key verdicts, with struct fields read from source.
`deny_unknown_fields` = 0 across `crates/`. `pn-gateway` has no `toml` dependency.
No crate opens a global `config.toml`. The only `config.toml` reference is containerd's.
Also verified are the flat persisted schemas for `networks.toml` and `volumes.toml`, and the seccomp enum literals.

**Not verified:** an end-to-end run of `podnext` with a documented `config.toml`. No such loader exists to exercise.
Whether a given absent block is *planned* or *stale* is a maintainer decision that the code does not settle.
This report states the drift. It does not guess intent.
