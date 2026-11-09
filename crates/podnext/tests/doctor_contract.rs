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
fn a_missing_proot_is_a_warning_not_a_failure() {
    // p7 step 8, verbatim from workbench/WORKLIST.md section Z: "doctor reports the proot binary as
    // optional (not a `fail` row)". The default engine is podnext's own, so a host without proot
    // runs containers and only `--engine proot` is affected. This replaces the test that asserted
    // the opposite - and it still detects a revert: restoring `Verdict::Fail` makes this row say
    // "fail" and the first assertion goes red.
    let (_code, stdout, _stderr) =
        run_with(&["doctor"], &[("PROOT_BIN", "/definitely/not/here/proot")]);
    let row = stdout
        .lines()
        .find(|line| line.contains("proot binary"))
        .unwrap_or_else(|| panic!("the proot row must still be reported: {stdout}"));
    assert!(
        row.trim_start().starts_with("warn"),
        "a missing proot must be a warning, not a failure: {row}"
    );
    assert!(
        row.contains("/definitely/not/here/proot"),
        "the row must carry the value it came from: {row}"
    );
    assert!(
        !stdout
            .lines()
            .any(|l| l.trim_start().starts_with("fail") && l.contains("proot binary")),
        "no failing row may name the proot binary any more: {stdout}"
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
