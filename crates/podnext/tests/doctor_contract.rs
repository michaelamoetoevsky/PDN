//! Roadmap section 9.3: `podnext doctor` reports host and guest diagnostics, and a failing
//! check is the only thing that changes the exit status. These tests drive the real binary,
//! which is also what lets the command carry the `[stable]` label in the reference.

use std::process::Command;

fn run(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_podnext"))
        .args(args)
        .output()
        .expect("spawn podnext");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn the_host_checks_run_and_pass_on_this_machine() {
    let (code, stdout, stderr) = run(&["doctor"]);
    assert_eq!(code, 0, "no host check should fail here; stderr={stderr:?}");
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
    assert!(
        stderr.is_empty(),
        "a passing doctor writes nothing to stderr, got {stderr:?}"
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
        stdout.contains("fail"),
        "the verdict must be printed as failing: {stdout}"
    );
}

#[test]
fn json_output_carries_a_verdict_for_every_check() {
    let (code, stdout, stderr) = run(&["doctor", "--format", "json"]);
    assert_eq!(code, 0, "stderr={stderr:?}");
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("doctor --format json is JSON");
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
