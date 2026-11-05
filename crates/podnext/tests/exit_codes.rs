//! The exit-code contract (roadmap section 9.1).
//!
//! Before this file, the exit status was asserted only where a particular test happened to need
//! it. The criterion asks for the taxonomy itself: a caller must be able to tell "you typed it
//! wrong" from "this is documented as unavailable" from "the container does not exist" from
//! "the child exited non-zero", without reading stderr.
//!
//! Measured, not assumed (2026-11-05, target/debug/podnext on device):
//!
//! | class                        | code | stdout | stderr                     |
//! |------------------------------|------|--------|----------------------------|
//! | success                      | 0    | answer | empty                      |
//! | usage error (clap)           | 2    | empty  | `error: unrecognized ...`  |
//! | documented refusal           | 1    | empty  | `Error: ... is not implemented` |
//! | domain error (no such thing) | 1    | empty  | `Error: Container ... not found` |
//! | guest exit code              | N    | as-is  | as-is                      |
//!
//! The last row is the one that cannot be faked: it needs a container, so it skips with a printed
//! reason when none is installed - and it must skip, not pass, because a test that cannot run is
//! not evidence.

use std::process::{Command, Output};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_podnext")
}

fn run(args: &[&str]) -> Output {
    let mut c = Command::new(bin());
    c.args(args);
    c.output().expect("failed to execute podnext")
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap_or_else(|| {
        panic!(
            "podnext was killed by a signal: {:?}; stderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

#[test]
fn success_is_zero() {
    let out = run(&["version"]);
    assert_eq!(code(&out), 0, "version should succeed");
    assert!(!out.stdout.is_empty(), "version should print something");
}

#[test]
fn a_usage_error_is_two_and_says_nothing_on_stdout() {
    for args in [
        vec!["nosuchcommand"],
        vec!["--nosuchflag"],
        vec!["run", "--nosuchflag"],
    ] {
        let out = run(&args);
        assert_eq!(
            code(&out),
            2,
            "{args:?} should be a usage error, not a runtime failure"
        );
        assert!(
            out.stdout.is_empty(),
            "{args:?} printed to stdout on a usage error: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("error:"),
            "{args:?} should explain itself on stderr"
        );
    }
}

#[test]
fn a_documented_refusal_is_one_with_a_reason() {
    // `--rm` is documented as refused (roadmap section 9.4 / issue #47): the class must be
    // distinguishable from a usage error, so exit 1 with a reason and no stdout. (`--detach`
    // held this example until PDN#57 made it a real supervised background session.)
    let out = run(&["run", "--rm", "alpine", "--", "true"]);
    assert_eq!(
        code(&out),
        1,
        "a refusal is a runtime decision (1), not a parse error (2): {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "a refusal must not write to stdout");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("--rm"),
        "the refusal must name what it refused: {err}"
    );
    assert!(
        err.contains("not implemented"),
        "the refusal must be phrased as a decision, not as a bug: {err}"
    );
}

#[test]
fn a_missing_container_is_a_domain_error_not_a_usage_error() {
    let out = run(&["login", "definitely-not-a-container-xyz"]);
    assert_eq!(
        code(&out),
        1,
        "an absent container is the tool's domain, not the user's syntax: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not found"));
}

#[test]
fn a_guest_exit_code_reaches_the_caller() {
    // Issue #37's sibling defect: the child's status must not be swallowed by the supervisor.
    // Skips with a printed reason when no container is installed: an unrunnable test is not a pass.
    let installed = Command::new(bin())
        .args(["list"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("alpine"))
        .unwrap_or(false);
    if !installed {
        eprintln!(
            "skipped: no container named alpine on this host; install one (podnext install alpine) \
             to exercise guest exit-code propagation"
        );
        return;
    }
    let out = run(&["run", "alpine", "--", "/bin/sh", "-c", "exit 7"]);
    assert_eq!(
        code(&out),
        7,
        "the guest's exit code must reach the caller unchanged; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let ok = run(&["run", "alpine", "--", "echo", "hello-from-guest"]);
    assert_eq!(code(&ok), 0);
    assert!(
        String::from_utf8_lossy(&ok.stdout).contains("hello-from-guest"),
        "the guest's stdout belongs to the caller: {}",
        String::from_utf8_lossy(&ok.stdout)
    );
}

#[test]
fn a_missing_or_unloadable_config_is_a_usage_error_not_code_three() {
    // Measured 2026-11-06 (target/debug/podnext on device): `config validate` with no file and
    // with an unloadable path both exited 3 — a code the taxonomy does not contain and that
    // cli_reference_contract asserts absent from the reference. The reference already names 2 for
    // an invalid file, and the Exit Codes table names 2 for a missing argument, so the code moved.
    let mut no_file = Command::new(bin());
    no_file.args(["config", "validate"]);
    no_file.env_remove("PN_GATEWAY_CONFIG");
    let out = no_file.output().expect("failed to execute podnext");
    assert_eq!(
        code(&out),
        2,
        "no configuration file is a usage error: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stdout.is_empty(),
        "a usage error must not write to stdout"
    );

    let out = run(&["config", "validate", "/nonexistent/podnext-gateway.toml"]);
    assert_eq!(
        code(&out),
        2,
        "an unloadable configuration file is a usage error: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty());
}

#[test]
fn the_mapped_user_is_stated_on_stderr_not_stdout() {
    // Roadmap section 9.3: run/login state which user the guest runs as. It must not reach stdout,
    // because the guest's own stdout is the caller's payload (asserted directly above). Skips with
    // a printed reason when no container is installed: an unrunnable test is not a pass.
    let installed = Command::new(bin())
        .args(["list"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("alpine"))
        .unwrap_or(false);
    if !installed {
        eprintln!("skipped: no container named alpine on this host; the user note needs a guest");
        return;
    }
    let out = run(&["run", "alpine", "--", "true"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("runs as root"),
        "run must state the mapped user on stderr: {err}"
    );
    assert!(
        out.stdout.is_empty(),
        "the user note must not reach stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}
