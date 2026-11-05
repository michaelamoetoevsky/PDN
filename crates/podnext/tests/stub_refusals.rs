//! Roadmap section 9.1: a command that cannot do its job must refuse, name the reason, and exit
//! non-zero. `build submit|status|cancel|list|artifacts|logs` and `watchdog events` used to print
//! "(not yet implemented)" on stdout and return `Ok(())` — exit 0 for something that did nothing,
//! which a script cannot distinguish from success.

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
fn build_service_subcommands_refuse_with_a_reason() {
    let cases: [&[&str]; 6] = [
        &["build", "submit", "--spec", "spec.yaml"],
        &["build", "status", "b1"],
        &["build", "cancel", "b1"],
        &["build", "list"],
        &["build", "artifacts", "b1"],
        &["build", "logs", "b1"],
    ];
    for args in cases {
        let (code, stdout, stderr) = run(args);
        assert_eq!(
            code, 1,
            "{args:?} must exit 1 (refusal), got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
        assert!(
            stderr.contains("is not implemented"),
            "{args:?} must name the reason on stderr: {stderr:?}"
        );
        assert!(
            !stdout.contains("not yet implemented") && !stdout.contains("not implemented"),
            "{args:?} must not report the non-action on stdout: {stdout:?}"
        );
    }
}

#[test]
fn watchdog_events_refuses_with_a_reason() {
    let (code, stdout, stderr) = run(&["watchdog", "events"]);
    assert_eq!(code, 1, "stdout={stdout:?} stderr={stderr:?}");
    assert!(stderr.contains("is not implemented"), "stderr={stderr:?}");
    assert!(
        !stdout.contains("Event streaming"),
        "the stub's stdout line is gone: {stdout:?}"
    );
}

#[test]
fn run_detach_refuses_instead_of_quietly_running_in_the_foreground() {
    let (code, stdout, stderr) = run(&["run", "no-such-container-xyz", "-d", "--", "true"]);
    assert_eq!(code, 1, "stdout={stdout:?} stderr={stderr:?}");
    assert!(
        stderr.contains("--detach is not implemented"),
        "stderr={stderr:?}"
    );
    assert!(
        !stderr.contains("not fully implemented"),
        "the old warning must be gone: {stderr:?}"
    );
}
