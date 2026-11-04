//! Integration tests for the gateway TOML loader (issue #27 follow-up).
//!
//! These exercise the *file-shaped* config: a global `config.toml` whose
//! gateway settings live under `[gateway]`.

use pn_gateway::{ConfigError, GatewayConfig};

/// A realistic global config: unrelated sibling sections plus a full `[gateway]`
/// table. Durations use serde's `Duration` table encoding (`{ secs, nanos }`).
const REALISTIC: &str = r#"
[general]
log_level = "debug"
gateway_addr = "unix:///run/pn-gateway.sock"

[gateway]
gateway_id = "8f14e45f-ea0c-4d1b-9c3a-2b0a5e6f7c8d"
name = "pn-gateway"
version = "0.1.0"

[gateway.proot_config]
rootfs = "/srv/rootfs"
seccomp_enabled = true
syscall_trace = false
max_tracees = 1000
timeout = { secs = 300, nanos = 0 }
bind_mounts = []
env_vars = {}

[gateway.distro_config]
default_distro = "ubuntu:22.04"
default_arch = "aarch64"
auto_update = true
cache_dir = "/var/cache/pn-gateway"
distro_sources = []

[gateway.compat_config]
platform_shim_enabled = true
path_translation_enabled = true
seccomp_bypass_enabled = true
stub_fs_enabled = true
mount_helper_enabled = true
cgroup_v2_enabled = true
custom_stub_paths = []
custom_mount_points = []

[gateway.fail_safe]
enabled = true
max_retries = 7
retry_delay = { secs = 5, nanos = 0 }
fallback_actions = ["Retry", "FallbackToHost"]
auto_recovery = true
circuit_breaker = { enabled = true, failure_threshold = 9, timeout = { secs = 90, nanos = 0 }, half_open_requests = 3 }

[gateway.guardrails]
enabled = true
strict_mode = true
audit_log = true
rules = []

[gateway.health]
enabled = true
check_interval = { secs = 30, nanos = 0 }
unhealthy_threshold = 3
checks = []
recovery_actions = []

[gateway.metrics]
enabled = true
interval = { secs = 60, nanos = 0 }
retention = { secs = 604800, nanos = 0 }
exporters = ["Prometheus", "JsonFile"]

[gateway.network]
enabled = true
bridge_name = "pn0"
subnet = "10.0.0.0/24"
dns_servers = ["8.8.8.8", "1.1.1.1"]
port_mappings = []

[gateway.storage]
root_dir = "/var/lib/pn-gateway"
max_size = 1000
cleanup_interval = { secs = 3600, nanos = 0 }
backup_enabled = true
backup_interval = { secs = 86400, nanos = 0 }
backup_retention = 7

[gateway.logging]
level = "info"
format = "json"
output = ["stdout"]
file_rotation = { enabled = true, max_size = 1048576, max_files = 10, compress = true }
"#;

#[test]
fn parses_realistic_config_into_struct() {
    let cfg = GatewayConfig::from_toml_str(REALISTIC).expect("realistic config must load");
    assert_eq!(cfg.name, "pn-gateway");
    assert_eq!(cfg.version, "0.1.0");
    assert_eq!(cfg.distro_config.default_distro, "ubuntu:22.04");
    assert_eq!(cfg.network.bridge_name, "pn0");
    assert_eq!(cfg.logging.format, "json");
    // Sibling section (`[general]`) is owned by other subsystems and ignored.
}

#[test]
fn documented_fail_safe_keys_populate_the_struct() {
    let cfg = GatewayConfig::from_toml_str(REALISTIC).expect("realistic config must load");
    assert_eq!(cfg.fail_safe.max_retries, 7);
    assert_eq!(cfg.fail_safe.circuit_breaker.failure_threshold, 9);
    assert_eq!(
        cfg.fail_safe.circuit_breaker.timeout,
        std::time::Duration::from_secs(90)
    );
}

#[test]
fn unknown_key_is_rejected_and_named() {
    let bad = REALISTIC.replace("[gateway.fail_safe]", "[gateway.fail_safe]\nbulkhead = 10");
    let err = GatewayConfig::from_toml_str(&bad).expect_err("unknown key must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("bulkhead"),
        "error must name the offending key, got: {msg}"
    );
}

#[test]
fn missing_file_differs_from_malformed_file() {
    let dir = tempfile::tempdir().expect("tempdir");

    let missing = dir.path().join("does-not-exist.toml");
    let err = GatewayConfig::from_toml_file(&missing).expect_err("missing file must fail");
    assert!(
        matches!(err, ConfigError::NotFound(_)),
        "missing file must be NotFound, got: {err:?}"
    );

    let malformed = dir.path().join("malformed.toml");
    std::fs::write(&malformed, "this is = = not toml\n").expect("write malformed fixture");
    let err = GatewayConfig::from_toml_file(&malformed).expect_err("malformed file must fail");
    assert!(
        matches!(err, ConfigError::Parse(_)),
        "malformed file must be Parse, got: {err:?}"
    );
}

#[test]
fn loads_from_an_explicit_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, REALISTIC).expect("write config fixture");
    let cfg = GatewayConfig::from_toml_file(&path).expect("file config must load");
    assert_eq!(cfg.fail_safe.circuit_breaker.failure_threshold, 9);
}

/// Issue #34: the documentation writes durations as strings (`"60s"`), while
/// `Duration`'s native serde form is a table. Before `duration_fmt` existed, a
/// config written exactly as documented could not be loaded at all.
///
/// Derived from `REALISTIC` by rewriting only the duration values, so the
/// fixture stays field-complete and a missing field cannot be mistaken for a
/// duration problem.
fn as_documented() -> String {
    REALISTIC
        .replace("{ secs = 604800, nanos = 0 }", "\"7d\"")
        .replace("{ secs = 86400, nanos = 0 }", "\"1d\"")
        .replace("{ secs = 3600, nanos = 0 }", "\"1h\"")
        .replace("{ secs = 300, nanos = 0 }", "\"300s\"")
        .replace("{ secs = 90, nanos = 0 }", "\"90s\"")
        .replace("{ secs = 60, nanos = 0 }", "\"1m\"")
        .replace("{ secs = 30, nanos = 0 }", "\"30s\"")
        .replace("{ secs = 5, nanos = 0 }", "\"5000ms\"")
}

#[test]
fn documented_duration_strings_load() {
    use std::time::Duration;
    let text = as_documented();
    assert!(
        !text.contains("{ secs"),
        "fixture should contain no table-form durations, so the test cannot pass by accident"
    );
    let cfg = GatewayConfig::from_toml_str(&text)
        .expect("a config written exactly as documented must load");
    assert_eq!(cfg.proot_config.timeout, Duration::from_secs(300));
    assert_eq!(cfg.fail_safe.retry_delay, Duration::from_secs(5));
    assert_eq!(
        cfg.fail_safe.circuit_breaker.timeout,
        Duration::from_secs(90)
    );
    assert_eq!(cfg.health.check_interval, Duration::from_secs(30));
    assert_eq!(cfg.metrics.interval, Duration::from_secs(60));
    assert_eq!(cfg.metrics.retention, Duration::from_secs(7 * 86400));
    assert_eq!(cfg.storage.cleanup_interval, Duration::from_secs(3600));
    assert_eq!(cfg.storage.backup_interval, Duration::from_secs(86400));
}

#[test]
fn every_duration_field_still_accepts_the_native_form() {
    use std::time::Duration;
    // REALISTIC itself is the native form: it must keep loading, so no existing
    // deployment breaks.
    let cfg = GatewayConfig::from_toml_str(REALISTIC).expect("native duration form must load");
    assert_eq!(cfg.fail_safe.retry_delay, Duration::from_secs(5));
    assert_eq!(cfg.health.check_interval, Duration::from_secs(30));

    // A bare integer is seconds too.
    let bare = as_documented().replace("timeout = \"300s\"", "timeout = 300");
    let cfg = GatewayConfig::from_toml_str(&bare).expect("bare integer seconds must load");
    assert_eq!(cfg.proot_config.timeout, Duration::from_secs(300));
}

#[test]
fn a_duration_typo_is_rejected_and_names_the_value() {
    let bad = as_documented().replace("retry_delay = \"5000ms\"", "retry_delay = \"5 parsecs\"");
    let err = GatewayConfig::from_toml_str(&bad).expect_err("unknown unit must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("parsecs") || msg.contains("unknown duration unit"),
        "error must name the bad unit, got: {msg}"
    );
}

#[test]
fn durations_round_trip_in_the_documented_form() {
    let cfg = GatewayConfig::from_toml_str(&as_documented()).expect("load");
    let text = cfg.to_toml_str().expect("serialise");
    // Serialisation must emit strings, never the `{ secs, nanos }` table: that is
    // the whole point of the fix.
    assert!(
        !text.contains("{ secs"),
        "serialised config must not fall back to the table form, got:\n{text}"
    );
    // The largest exact unit wins, so 300s renders as "5m" and 604800s as "7d".
    // Asserting the exact renderings keeps a future change to that rule visible
    // rather than silent.
    for expected in [
        "timeout = \"5m\"",
        "retry_delay = \"5s\"",
        "check_interval = \"30s\"",
        "retention = \"7d\"",
        "backup_interval = \"1d\"",
    ] {
        assert!(
            text.contains(expected),
            "expected {expected} in serialised config, got:\n{text}"
        );
    }
    let again = GatewayConfig::from_toml_str(&text).expect("re-parse serialised config");
    assert_eq!(again.proot_config.timeout, cfg.proot_config.timeout);
    assert_eq!(again.fail_safe.retry_delay, cfg.fail_safe.retry_delay);
    assert_eq!(again.metrics.retention, cfg.metrics.retention);
}
