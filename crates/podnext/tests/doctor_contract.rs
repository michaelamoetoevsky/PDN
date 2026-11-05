//! Roadmap section 9.3: `podnext doctor` reports host and guest diagnostics, and a failing check
//! is the only thing that changes the exit status. These tests drive the real binary, which is
//! also what lets the command carry the `[stable]` label in the reference.
//!
//! The verdict of a check depends on the host (a CI runner has no proot binary, this device
//! does), so the assertions are about the relationship between the verdicts printed and the
//! exit status, never about a fixed status for a host-dependent check.

use std::process::Command;

fn run_with(args: &[&str], env: &[(&str, &str)]) -> (i32, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_podnext"));
    cmd.args(args);
    for (key, value) in env {
        cmd.env(key, value);
    }
    let out = cmd.output().expect("spawn podnext");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn run(args: &[&str]) -> (i32, String, String) {
    run_with(args, &[])
}

/// Count the rows whose verdict column reads `fail`.
fn reported_failures(stdout: &str) -> usize {
    stdout
        .lines()
        .filter(|line| line.trim_start().starts_with("fail"))
        .count()
}

#[test]
fn the_host_checks_run_and_the_exit_status_follows_the_verdicts() {
    let (code, stdout, stderr) = run(&["doctor"]);
    for expected in [
        "proot binary",
        "temporary directory",
        "dns",
        "host kernel",
        "cgroups",
    ] {
        assert!(
            stdout.contains(expected),
            "doctor did not report `{expected}`: {stdout}"
        );
    }
    let failures = reported_failures(&stdout);
    if failures == 0 {
        assert_eq!(
            code, 0,
            "no failing row, so the command must succeed: {stdout}"
        );
        assert!(
            stderr.is_empty(),
            "a passing doctor writes nothing to stderr, got {stderr:?}"
        );
    } else {
        assert_eq!(code, 1, "{failures} failing row(s) must exit 1: {stdout}");
        assert!(
            stderr.contains("checks failed"),
            "the reason must be on stderr, not only in the table: {stderr:?}"
        );
    }
}

#[test]
fn a_failing_check_fails_the_command_and_names_itself() {
    // `PROOT_BIN` is read by the check, so a missing binary is a deterministic failure on any
    // machine instead of an assumption about what the host has installed.
    let (code, stdout, stderr) =
        run_with(&["doctor"], &[("PROOT_BIN", "/definitely/not/here/proot")]);
    assert_eq!(code, 1, "a check that fails must exit 1: {stdout}{stderr}");
    assert!(
        reported_failures(&stdout) >= 1 && stdout.contains("proot binary"),
        "the failing row must name the check: {stdout}"
    );
    assert!(
        stdout.contains("/definitely/not/here/proot"),
        "the row must carry the value it came from: {stdout}"
    );
}

#[test]
fn a_named_container_that_does_not_exist_fails_and_says_which() {
    let name = "podnext-doctor-no-such-container";
    let (code, stdout, _stderr) = run(&["doctor", "--container", name]);
    assert_eq!(
        code, 1,
        "an unresolvable container must fail the command: {stdout}"
    );
    assert!(
        stdout.contains(name),
        "the failing row must name the container: {stdout}"
    );
    assert!(
        reported_failures(&stdout) >= 1,
        "the verdict must be printed as failing: {stdout}"
    );
}

#[test]
fn json_output_carries_a_verdict_and_a_detail_for_every_check() {
    let (_code, stdout, _stderr) = run(&["doctor", "--format", "json"]);
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("doctor --format json is JSON whatever the verdicts");
    let checks = value.as_array().expect("an array of checks");
    assert!(!checks.is_empty(), "no checks reported");
    for check in checks {
        let verdict = check.get("verdict").and_then(|v| v.as_str()).unwrap_or("");
        assert!(
            ["ok", "warn", "fail", "unknown"].contains(&verdict),
            "unexpected verdict {verdict:?} in {check}"
        );
        assert!(
            !check
                .get("detail")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .is_empty(),
            "a check with no detail cannot be acted on: {check}"
        );
    }
}
