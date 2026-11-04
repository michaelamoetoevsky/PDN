//! Whole-file documentation ↔ configuration drift guard.
//!
//! ## Scope (honest, read this)
//!
//! This test scans the fenced ```toml blocks of the documentation files listed
//! in [`DOC_FILES`]. It does **not** scan every file under `docs/src/` — only
//! the files reconciled by the doc-drift round-2 increment (see
//! `research/DOCS_DRIFT_ROUND2.md`). A doc file that is not in `DOC_FILES` is
//! not checked.
//!
//! `docs/src/architecture.md` contains no ```toml blocks (its config sketches
//! are ```rust), so it contributes zero keys here. Its prose and type-sketch
//! reconciliation was done by hand and is **not** covered by this guard.
//!
//! ## What counts as a "documented key"
//!
//! A bare `key = ...` assignment inside a fenced ```toml block — quoted keys
//! (`"org.pdn.type" = ...`) are dynamic map entries, not config field names.
//! The bare sub-keys of an inline-table value (`circuit_breaker = { a = 1 }`)
//! are keys too. An assignment whose enclosing table is a dynamic map (`env`,
//! `labels`, `annotations`, `mounts`, `volumes`, `tmpfs`, `devices`, `sysctls`,
//! `ulimits`, `rules`) is a map entry, not a key, and is skipped. A
//! `# planned: k1, k2` comment line declares the listed keys as keys too.
//!
//! ## Verdict rule
//!
//! Every documented key must either
//!   1. exist as a field name of a deserialisable config struct reachable from
//!      this crate (the real field set is derived by serialising the structs —
//!      never hand-copied), or
//!   2. be listed in [`PLANNED`] below, with a reason.
//!
//! ## Ceiling (deliberate)
//!
//! The guard matches key **names**, not full nesting paths. A documented key
//! whose leaf name happens to match a real field is accepted even if its
//! section path is wrong (e.g. a `[network.config] subnet` that should be a
//! flat `subnet`). This is a known false-negative class, documented in
//! `research/DOCS_DRIFT_ROUND2.md`; it is accepted because path inference from
//! prose headings is not decidable and a wrong path check would cry wolf.

use std::collections::BTreeSet;

/// Documentation files covered by this guard. Keep in sync with the increment
/// report. Files not listed here are NOT checked.
const DOC_FILES: &[&str] = &[
    "docs/src/configuration.md",
    "docs/src/architecture.md",
    "docs/src/cli-reference.md",
];

/// Tables whose entries are dynamic map keys (env vars, labels, syscall names,
/// mount paths, ...), not configuration field names.
const DYNAMIC_TABLES: &[&str] = &[
    "env",
    "labels",
    "annotations",
    "mounts",
    "volumes",
    "tmpfs",
    "devices",
    "sysctls",
    "ulimits",
    "rules",
];

/// Documented keys that have no backing struct field today. Each entry carries
/// the reason it is listed. Generated from the round-2 reconciliation decision
/// table in `research/DOCS_DRIFT_ROUND2.md`.
#[rustfmt::skip]
const PLANNED: &[(&str, &str)] = &[
    ("addr", "no gRPC config struct exists in pn-gateway"),
    ("allow_unsigned", "no plugin-manager config struct exists in pn-gateway"),
    ("allowed_images", "no field on GuardrailConfig (real: enabled, rules, strict_mode, audit_log)"),
    ("api", "no `api` field on GpuConfig (real: enabled, devices, vulkan/opengl/drm/compute flags)"),
    ("blocked_images", "no field on GuardrailConfig (real: enabled, rules, strict_mode, audit_log)"),
    ("blocked_syscalls", "no field on GuardrailConfig (real: enabled, rules, strict_mode, audit_log)"),
    ("bulkhead_max_concurrent", "documented fail-safe control with no FailSafeConfig field (issue #27; planned, not implemented)"),
    ("bulkhead_queue_size", "documented fail-safe control with no FailSafeConfig field (issue #27; planned, not implemented)"),
    ("cache_dir", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("cmd", "no [container.config] table exists; the real ContainerConfig is flat"),
    ("config_dir", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("cors_allowed_headers", "no [api.server] struct exists in pn-gateway"),
    ("cors_allowed_methods", "no [api.server] struct exists in pn-gateway"),
    ("cors_allowed_origins", "no [api.server] struct exists in pn-gateway"),
    ("cors_max_age", "no [api.server] struct exists in pn-gateway"),
    ("cpu_period", "no field on ResourceLimits (real: cpu_limit, memory_limit, disk_limit, pids_limit)"),
    ("cpu_quota", "no field on ResourceLimits (real: cpu_limit, memory_limit, disk_limit, pids_limit)"),
    ("cpu_shares", "no field on ResourceLimits (real: cpu_limit, memory_limit, disk_limit, pids_limit)"),
    ("cpuset_cpus", "no field on ResourceLimits (real: cpu_limit, memory_limit, disk_limit, pids_limit)"),
    ("cpuset_mems", "no field on ResourceLimits (real: cpu_limit, memory_limit, disk_limit, pids_limit)"),
    ("created", "container metadata, not a ContainerConfig field (real: name, distro, version, arch)"),
    ("custom_rules", "no custom_rules field; container SecurityConfig uses seccomp_profile, profile seccomp uses `syscalls`"),
    ("data_dir", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("default_cpu_limit", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("default_dns", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("default_dns_search", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("default_memory_limit", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("default_network", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("default_pids_limit", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("default_registry", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("default_runtime", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("default_security_profile", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("default_storage_driver", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("default_volume_driver", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("description", "no `description` field on SecurityProfileConfig"),
    ("dns_options", "no field on container NetworkConfig (real: mode, bridge, ip_address, gateway, dns_servers, port_mappings)"),
    ("dns_search", "no field on container NetworkConfig (real: mode, bridge, ip_address, gateway, dns_servers, port_mappings)"),
    ("domainname", "no [container.config] table exists; the real ContainerConfig is flat"),
    ("enable_criu", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("enable_gpu", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("enable_icc", "no field on network NetworkConfig (flat schema: name, driver, subnet, gateway, dns_servers, enable_ipv6, mtu, containers, ips)"),
    ("enable_metrics", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("enable_plugins", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("enable_tracing", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("endpoint", "no tracing config struct exists in pn-gateway"),
    ("entrypoint", "no [container.config] table exists; the real ContainerConfig is flat"),
    ("extra_hosts", "no field on container NetworkConfig (real: mode, bridge, ip_address, gateway, dns_servers, port_mappings)"),
    ("fs_type", "no field on VolumeConfig (real: name, driver, labels, options)"),
    ("gateway_addr", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("gateway_retry_attempts", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("gateway_timeout", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("group", "no [container.config] table exists; the real ContainerConfig is flat"),
    ("groups", "no [container.config] table exists; the real ContainerConfig is flat"),
    ("healthy_threshold", "no field on HealthConfig (real: enabled, check_interval, checks, unhealthy_threshold, recovery_actions)"),
    ("image_pull_policy", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("insecure_registries", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("internal", "no field on network NetworkConfig (flat schema: name, driver, subnet, gateway, dns_servers, enable_ipv6, mtu, containers, ips)"),
    ("io_max_read", "no field on ResourceLimits (real: cpu_limit, memory_limit, disk_limit, pids_limit)"),
    ("io_max_read_bps", "no field on ResourceLimits (real: cpu_limit, memory_limit, disk_limit, pids_limit)"),
    ("io_max_read_iops", "no field on ResourceLimits (real: cpu_limit, memory_limit, disk_limit, pids_limit)"),
    ("io_max_write", "no field on ResourceLimits (real: cpu_limit, memory_limit, disk_limit, pids_limit)"),
    ("io_max_write_bps", "no field on ResourceLimits (real: cpu_limit, memory_limit, disk_limit, pids_limit)"),
    ("io_max_write_iops", "no field on ResourceLimits (real: cpu_limit, memory_limit, disk_limit, pids_limit)"),
    ("io_weight", "no field on ResourceLimits (real: cpu_limit, memory_limit, disk_limit, pids_limit)"),
    ("ip_range", "no field on network NetworkConfig (flat schema: name, driver, subnet, gateway, dns_servers, enable_ipv6, mtu, containers, ips)"),
    ("ipc_mode", "no field on container NetworkConfig (real: mode, bridge, ip_address, gateway, dns_servers, port_mappings)"),
    ("ipv6_gateway", "no field on network NetworkConfig (flat schema: name, driver, subnet, gateway, dns_servers, enable_ipv6, mtu, containers, ips)"),
    ("ipv6_subnet", "no field on network NetworkConfig (flat schema: name, driver, subnet, gateway, dns_servers, enable_ipv6, mtu, containers, ips)"),
    ("keepalive_permit_without_stream", "no gRPC config struct exists in pn-gateway"),
    ("keepalive_time", "no gRPC config struct exists in pn-gateway"),
    ("keepalive_timeout", "no gRPC config struct exists in pn-gateway"),
    ("log_dir", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("log_file", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("log_format", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("log_level", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("log_max_files", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("log_rotation", "documented under [general]; no GeneralConfig struct exists anywhere and the global config.toml has no loader"),
    ("mask_paths", "no field on container SecurityConfig (real: privileged, capabilities, seccomp_profile, apparmor_profile, selinux_label, no_new_privileges, read_only_rootfs)"),
    ("max_checkpoints", "no checkpoint support exists (no struct at all)"),
    ("max_containers", "no field on GuardrailConfig (real: enabled, rules, strict_mode, audit_log)"),
    ("max_containers_per_user", "no field on GuardrailConfig (real: enabled, rules, strict_mode, audit_log)"),
    ("max_cpu_total", "no field on GuardrailConfig (real: enabled, rules, strict_mode, audit_log)"),
    ("max_memory_total", "no field on GuardrailConfig (real: enabled, rules, strict_mode, audit_log)"),
    ("max_plugins", "no plugin-manager config struct exists in pn-gateway"),
    ("max_recv_msg_size", "no gRPC config struct exists in pn-gateway"),
    ("max_send_msg_size", "no gRPC config struct exists in pn-gateway"),
    ("memory_reservation", "no field on ResourceLimits (real: cpu_limit, memory_limit, disk_limit, pids_limit)"),
    ("memory_swap_limit", "no field on ResourceLimits (real: cpu_limit, memory_limit, disk_limit, pids_limit)"),
    ("mount_options", "no field on VolumeConfig (real: name, driver, labels, options)"),
    ("networks", "no field on container NetworkConfig (real: mode, bridge, ip_address, gateway, dns_servers, port_mappings)"),
    ("path", "no field on MetricsConfig (real: enabled, interval, retention, exporters)"),
    ("paths", "no mask_paths/readonly_paths on SecurityProfileConfig"),
    ("plugin_dirs", "no plugin-manager config struct exists in pn-gateway"),
    ("port", "no field on MetricsConfig (real: enabled, interval, retention, exporters)"),
    ("ports", "no field on container NetworkConfig (real: mode, bridge, ip_address, gateway, dns_servers, port_mappings)"),
    ("poststart", "no container hooks support exists (no struct at all)"),
    ("poststop", "no container hooks support exists (no struct at all)"),
    ("prestart", "no container hooks support exists (no struct at all)"),
    ("prestop", "no container hooks support exists (no struct at all)"),
    ("quota_enabled", "no field on VolumeConfig (real: name, driver, labels, options)"),
    ("quota_size", "no field on VolumeConfig (real: name, driver, labels, options)"),
    ("readonly_paths", "no field on container SecurityConfig (real: privileged, capabilities, seccomp_profile, apparmor_profile, selinux_label, no_new_privileges, read_only_rootfs)"),
    ("required_security_profile", "no field on GuardrailConfig (real: enabled, rules, strict_mode, audit_log)"),
    ("retry_base_delay", "documented fail-safe control with no FailSafeConfig field (issue #27; planned, not implemented)"),
    ("retry_max_delay", "documented fail-safe control with no FailSafeConfig field (issue #27; planned, not implemented)"),
    ("sampling_ratio", "no tracing config struct exists in pn-gateway"),
    ("stop_signal", "no [container.config] table exists; the real ContainerConfig is flat"),
    ("stop_timeout", "no [container.config] table exists; the real ContainerConfig is flat"),
    ("type", "no tracing config struct exists in pn-gateway"),
    ("updated", "container metadata, not a ContainerConfig field (real: name, distro, version, arch)"),
    ("user", "no [container.config] table exists; the real ContainerConfig is flat"),
    ("uts_mode", "no field on container NetworkConfig (real: mode, bridge, ip_address, gateway, dns_servers, port_mappings)"),
];

#[test]
fn documented_config_keys_are_real_or_planned() {
    let real = real_field_names();
    assert!(
        real.contains("fail_safe") && real.contains("max_retries"),
        "real field set looks wrong; the derivation is broken"
    );

    let planned: BTreeSet<&str> = PLANNED.iter().map(|(k, _)| *k).collect();
    let mut unresolved: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for rel in DOC_FILES {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(rel);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));
        let (active, declared) = extract_keys(&text);
        for key in active.iter().chain(declared.iter()) {
            checked += 1;
            if !real.contains(key) && !planned.contains(key.as_str()) {
                unresolved.push(format!("{rel}: `{key}`"));
            }
        }
    }

    unresolved.sort();
    unresolved.dedup();
    assert!(
        unresolved.is_empty(),
        "{} documented key(s) are neither a real config field nor listed in PLANNED \
         (issue #27 / doc-drift):\n  {}\nFix the markdown, or add the key to PLANNED with a \
         reason. ({checked} documented keys checked across {} file(s))",
        unresolved.len(),
        unresolved.join("\n  "),
        DOC_FILES.len(),
    );
}

/// Every [`PLANNED`] entry must actually be documented, and every `# planned:`
/// declaration in the docs must be listed in [`PLANNED`] — the two must not
/// drift apart.
#[test]
fn planned_allowlist_matches_documented_declarations() {
    let planned: BTreeSet<&str> = PLANNED.iter().map(|(k, _)| *k).collect();
    let mut declared: BTreeSet<String> = BTreeSet::new();
    for rel in DOC_FILES {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(rel);
        let text = std::fs::read_to_string(&path).unwrap();
        declared.extend(extract_keys(&text).1);
    }

    let missing_from_allowlist: Vec<&String> = declared
        .iter()
        .filter(|k| !planned.contains(k.as_str()))
        .collect();
    let missing_from_docs: Vec<&&str> =
        planned.iter().filter(|k| !declared.contains(**k)).collect();

    assert!(
        missing_from_allowlist.is_empty(),
        "`# planned:` keys in the docs missing from PLANNED: {missing_from_allowlist:?}"
    );
    assert!(
        missing_from_docs.is_empty(),
        "PLANNED entries not declared anywhere in the docs: {missing_from_docs:?}"
    );
}

/// Extract (active keys, `# planned:` keys) from the fenced toml blocks of one
/// document.
fn extract_keys(text: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut active = BTreeSet::new();
    let mut planned = BTreeSet::new();
    let mut in_toml = false;
    let mut sections: Vec<String> = Vec::new();

    for raw in text.lines() {
        let t = raw.trim();
        if let Some(rest) = t.strip_prefix("```") {
            if in_toml {
                in_toml = false;
            } else {
                in_toml = rest.trim().eq_ignore_ascii_case("toml");
            }
            sections.clear();
            continue;
        }
        if !in_toml || t.is_empty() || t.starts_with('#') {
            if in_toml {
                if let Some(rest) = t.strip_prefix("# planned:") {
                    for k in rest.split(',') {
                        let k = k.trim();
                        if !k.is_empty() {
                            planned.insert(k.to_string());
                        }
                    }
                }
            }
            continue;
        }
        if t.starts_with('[') {
            let inner = t.trim_start_matches('[').trim_end_matches(']');
            sections = inner
                .split('.')
                .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                .collect();
            continue;
        }
        let Some(eq) = t.find('=') else { continue };
        let key = t[..eq].trim();
        if key.starts_with('"') || key.starts_with('\'') {
            continue; // dynamic map entry
        }
        if sections
            .iter()
            .any(|s| DYNAMIC_TABLES.contains(&s.as_str()))
        {
            continue; // map entry inside a dynamic table
        }
        if is_ident(key) {
            active.insert(key.to_string());
        }
        let val = t[eq + 1..].trim();
        if val.starts_with('{') {
            for sub in inline_table_keys(val) {
                active.insert(sub);
            }
        }
    }
    (active, planned)
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && !s.chars().next().unwrap().is_ascii_digit()
}

/// Bare `key =` names inside an inline table `{ a = 1, b = { c = 2 } }`.
fn inline_table_keys(val: &str) -> Vec<String> {
    let body = val.trim().trim_start_matches('{').trim_end_matches('}');
    let mut out = Vec::new();
    let mut ident = String::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_alphanumeric() || c == '_' {
            ident.push(c);
            continue;
        }
        if c == '=' && !ident.is_empty() {
            out.push(std::mem::take(&mut ident));
            continue;
        }
        ident.clear();
    }
    out
}

/// The set of field names of every deserialisable config struct reachable from
/// this crate, at any nesting depth. Derived by serialising representative
/// instances — not hand-copied.
fn real_field_names() -> BTreeSet<String> {
    use pn_gateway::config as gw;
    let mut out = BTreeSet::new();

    macro_rules! add {
        ($v:expr) => {
            collect_keys(&serde_json::to_value($v).expect("serialisable"), &mut out)
        };
    }

    add!(gw::GatewayConfig::default());
    add!(gw::ProotConfig::default());
    add!(gw::DistroConfig::default());
    add!(gw::DistroSource {
        name: String::new(),
        versions: vec![],
        base_url: String::new(),
    });
    add!(gw::CompatConfig::default());
    add!(gw::FailSafeConfig::default());
    add!(gw::CircuitBreakerConfig::default());
    add!(gw::GuardrailConfig::default());
    add!(gw::GuardrailRule {
        name: String::new(),
        description: String::new(),
        condition: String::new(),
        action: gw::GuardrailAction::Warn,
        severity: gw::Severity::Info,
    });
    add!(gw::HealthConfig::default());
    add!(gw::HealthCheck {
        name: String::new(),
        check_type: gw::HealthCheckType::Process,
        target: String::new(),
        interval: std::time::Duration::from_secs(1),
        timeout: std::time::Duration::from_secs(1),
    });
    add!(gw::MetricsConfig::default());
    add!(gw::NetworkConfig::default());
    add!(gw::StorageConfig::default());
    add!(gw::LoggingConfig::default());
    add!(gw::FileRotationConfig::default());
    add!(gw::PortMapping {
        host_port: 0,
        container_port: 0,
        protocol: String::new(),
    });

    add!(proot_distro_core::ContainerConfig::default());
    add!(proot_distro_core::ResourceLimits::default());
    add!(proot_distro_core::container::NetworkConfig::default());
    add!(proot_distro_core::container::PortMapping {
        host_port: 0,
        container_port: 0,
        protocol: String::new(),
    });
    add!(proot_distro_core::SecurityConfig::default());
    add!(proot_distro_core::GpuConfig::default());
    add!(proot_distro_core::network::NetworkConfig::default());
    add!(proot_distro_core::VolumeConfig::default());

    add!(proot_compat::SecurityProfileConfig::default());
    add!(proot_compat::SeccompProfile::default());
    add!(proot_compat::CapabilityConfig::default());
    add!(proot_compat::AppArmorProfile {
        profile_name: String::new(),
        profile_content: String::new(),
        enforce: false,
    });
    add!(proot_compat::SelLinuxProfile {
        context: String::new(),
        level: None,
        role: None,
        type_: None,
        user: None,
    });

    out
}

fn collect_keys(v: &serde_json::Value, out: &mut BTreeSet<String>) {
    match v {
        serde_json::Value::Object(m) => {
            for (k, val) in m {
                out.insert(k.clone());
                collect_keys(val, out);
            }
        }
        serde_json::Value::Array(a) => {
            for e in a {
                collect_keys(e, out);
            }
        }
        _ => {}
    }
}
