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
fn attach_without_a_running_session_refuses_with_a_reason() {
    // PDN#57: `attach` reconnects to a running detached session and never starts one, so with
    // nothing running it must say so and exit 1 - not silently open a fresh shell.
    let (code, stdout, stderr) = run(&["attach", "no-such-container-xyz"]);
    assert_eq!(code, 1, "stdout={stdout:?} stderr={stderr:?}");
    assert!(
        stderr.contains("No running session"),
        "the refusal must name the state it found: {stderr:?}"
    );
    assert!(
        !stderr.contains("not fully implemented"),
        "the old warning must be gone: {stderr:?}"
    );
}

/// `run` accepted nine options and dropped every one of them, so the command that ran was not
/// the one the caller asked for. They are refused by name now, and the refusal names the flag.
#[test]
fn run_refuses_the_options_it_cannot_honour() {
    let cases: [&[&str]; 8] = [
        &["run", "no-such-container-xyz", "--rm", "--", "true"],
        &[
            "run",
            "no-such-container-xyz",
            "--name",
            "other",
            "--",
            "true",
        ],
        &[
            "run",
            "no-such-container-xyz",
            "--network",
            "net1",
            "--",
            "true",
        ],
        &[
            "run",
            "no-such-container-xyz",
            "-p",
            "8080:80",
            "--",
            "true",
        ],
        &[
            "run",
            "no-such-container-xyz",
            "--entrypoint",
            "/bin/sh",
            "--",
            "true",
        ],
        &["run", "no-such-container-xyz", "-u", "1000", "--", "true"],
        &[
            "run",
            "no-such-container-xyz",
            "--workdir",
            "/tmp",
            "--",
            "true",
        ],
        // Issue #62: this one was parsed and dropped, so "accepted" and "enforced" were the same
        // word. It refuses like the rest now.
        &["run", "no-such-container-xyz", "--seccomp", "--", "true"],
    ];
    for args in cases {
        let (code, stdout, stderr) = run(args);
        assert_eq!(
            code, 1,
            "{args:?} must be refused: stdout={stdout:?} stderr={stderr:?}"
        );
        let flag = args.iter().find(|a| a.starts_with('-')).expect("a flag");
        assert!(
            stderr.contains(*flag),
            "{args:?} must name the option it refuses: {stderr:?}"
        );
        assert!(
            stdout.is_empty(),
            "{args:?} must not print a run header before refusing: {stdout:?}"
        );
    }
}

/// The two compatibility flags stay accepted, because stdio is inherited and they describe it.
#[test]
fn run_still_accepts_tty_and_interactive() {
    // No container needed: reaching the container lookup proves the flags were not refused.
    let (code, _stdout, stderr) = run(&["run", "no-such-container-xyz", "-t", "-i", "--", "true"]);
    assert_eq!(code, 1, "an unknown container still fails");
    assert!(
        !stderr.contains("not implemented"),
        "-t/-i must not be refused: {stderr:?}"
    );
    assert!(
        stderr.contains("not found") || stderr.contains("no-such-container-xyz"),
        "the failure must be the container lookup: {stderr:?}"
    );
}
